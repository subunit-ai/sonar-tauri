use crate::bridge_client::{BridgeClient, BridgeClientError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime};
use tauri_plugin_notification::NotificationExt;
use tokio::time::{interval, sleep, Duration, MissedTickBehavior};

const CONSENT_REQUEST_EVENT: &str = "consent://request";
const CONSENT_STATE_EVENT: &str = "consent://state";
const OVERLAY_STATE_EVENT: &str = "overlay://state";
const OVERLAY_WINDOW_LABEL: &str = "overlay";
const OVERLAY_CONTROL_LABEL: &str = "overlayControl";
const OPERATOR_ACTIVE_WINDOW_MS: i128 = 40_000;
const OVERLAY_CONTROL_WIDTH: u32 = 170;
const OVERLAY_CONTROL_HEIGHT: u32 = 46;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConsentRequest {
    pub id: String,
    #[serde(default)]
    pub operator_id: Option<String>,
    #[serde(default)]
    pub cmd: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub scope: Option<Value>,
    #[serde(default)]
    pub expires_at: Option<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConsentState {
    #[serde(default = "default_remote_access")]
    pub remote_access: String,
    #[serde(default)]
    pub session_grant: Option<Value>,
    #[serde(default)]
    pub pending_count: u64,
    #[serde(default)]
    pub last_session_active_at: Option<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone)]
pub struct ConsentController {
    client: BridgeClient,
    seen_pending: Arc<Mutex<HashSet<String>>>,
    last_signed_operator_id: Arc<Mutex<Option<String>>>,
    overlay_active: Arc<Mutex<bool>>,
    overlay_dismissed: Arc<Mutex<bool>>,
}

#[derive(Clone, Debug, Serialize)]
struct OverlayState {
    operator: String,
}

#[derive(Serialize)]
struct AllowBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    remember_for_seconds: Option<u64>,
}

impl ConsentController {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            client: BridgeClient::new()?,
            seen_pending: Arc::new(Mutex::new(HashSet::new())),
            last_signed_operator_id: Arc::new(Mutex::new(None)),
            overlay_active: Arc::new(Mutex::new(false)),
            overlay_dismissed: Arc::new(Mutex::new(false)),
        })
    }

    pub fn start<R>(&self, app: AppHandle<R>)
    where
        R: Runtime,
    {
        let controller = self.clone();
        tauri::async_runtime::spawn(async move {
            sleep(Duration::from_secs(1)).await;

            let mut ticker = interval(Duration::from_secs(1));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

            loop {
                ticker.tick().await;
                controller.poll_once(&app).await;
            }
        });
    }

    pub async fn state(&self) -> Result<ConsentState, BridgeClientError> {
        self.client.get_authed_json("/consent/state").await
    }

    pub async fn allow(
        &self,
        id: &str,
        remember_for_seconds: Option<u64>,
    ) -> Result<(), BridgeClientError> {
        self.client
            .post_authed_json(
                &format!("/consent/{id}/allow"),
                &AllowBody {
                    remember_for_seconds,
                },
            )
            .await
    }

    pub async fn deny(&self, id: &str) -> Result<(), BridgeClientError> {
        self.client
            .post_authed_json(&format!("/consent/{id}/deny"), &serde_json::json!({}))
            .await
    }

    pub async fn revoke(&self) -> Result<(), BridgeClientError> {
        self.client
            .post_authed_json("/consent/revoke", &serde_json::json!({}))
            .await
    }

    pub async fn resume(&self) -> Result<(), BridgeClientError> {
        self.client
            .post_authed_json("/consent/resume", &serde_json::json!({}))
            .await
    }

    async fn pending(&self) -> Result<Vec<ConsentRequest>, BridgeClientError> {
        self.client.get_authed_json("/consent/pending").await
    }

    async fn poll_once<R>(&self, app: &AppHandle<R>)
    where
        R: Runtime,
    {
        match self.state().await {
            Ok(state) => {
                set_tray_tooltip(app, Some(state.remote_access.as_str()));
                self.reflect_overlay(app, &state);
                if let Err(error) = app.emit(CONSENT_STATE_EVENT, state) {
                    eprintln!("failed to emit consent state: {error}");
                }
            }
            Err(error) => {
                set_tray_tooltip(app, None);
                self.force_hide_overlay(app);
                eprintln!("failed to poll consent state: {error}");
            }
        }

        let pending = match self.pending().await {
            Ok(pending) => pending,
            Err(error) => {
                eprintln!("failed to poll pending consent requests: {error}");
                return;
            }
        };

        let new_rows = self.record_new_pending(&pending);
        for row in new_rows {
            if let Err(error) = app.emit(CONSENT_REQUEST_EVENT, &row) {
                eprintln!("failed to emit consent request {}: {error}", row.id);
            }
            raise_main_window(app);
            show_notification(app, &row);
        }
    }

    fn record_new_pending(&self, pending: &[ConsentRequest]) -> Vec<ConsentRequest> {
        let current_ids = pending
            .iter()
            .map(|row| row.id.clone())
            .collect::<HashSet<_>>();
        let mut seen_pending = self.lock_seen_pending();
        let mut new_rows = Vec::new();

        for row in pending {
            if let Some(operator_id) = signed_operator_id(row) {
                *self.lock_last_signed_operator_id() = Some(operator_id);
            }

            if seen_pending.insert(row.id.clone()) {
                new_rows.push(row.clone());
            }
        }

        seen_pending.retain(|id| current_ids.contains(id));
        new_rows
    }

    fn lock_seen_pending(&self) -> MutexGuard<'_, HashSet<String>> {
        self.seen_pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_last_signed_operator_id(&self) -> MutexGuard<'_, Option<String>> {
        self.last_signed_operator_id
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_overlay_active(&self) -> MutexGuard<'_, bool> {
        self.overlay_active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_overlay_dismissed(&self) -> MutexGuard<'_, bool> {
        self.overlay_dismissed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn dismiss_overlay<R>(&self, app: &AppHandle<R>)
    where
        R: Runtime,
    {
        *self.lock_overlay_dismissed() = true;
        self.hide_overlay_windows(app);
    }

    pub fn force_hide_overlay<R>(&self, app: &AppHandle<R>)
    where
        R: Runtime,
    {
        *self.lock_overlay_active() = false;
        *self.lock_overlay_dismissed() = false;
        self.hide_overlay_windows(app);
    }

    fn reflect_overlay<R>(&self, app: &AppHandle<R>, state: &ConsentState)
    where
        R: Runtime,
    {
        let active =
            state.remote_access == "active" && is_recent_activity(&state.last_session_active_at);

        if active {
            *self.lock_overlay_active() = true;
            if *self.lock_overlay_dismissed() {
                self.hide_overlay_windows(app);
            } else {
                self.show_overlay_windows(app);
            }
            return;
        }

        if *self.lock_overlay_active() {
            self.hide_overlay_windows(app);
        }
        *self.lock_overlay_active() = false;
        *self.lock_overlay_dismissed() = false;
    }

    fn show_overlay_windows<R>(&self, app: &AppHandle<R>)
    where
        R: Runtime,
    {
        let payload = OverlayState {
            operator: self
                .lock_last_signed_operator_id()
                .clone()
                .unwrap_or_else(|| "u1".to_string()),
        };

        if let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) {
            let _ = window.set_ignore_cursor_events(true);
            let _ = window.set_always_on_top(true);
            let _ = window.set_fullscreen(true);
            let _ = window.show();
        }

        if let Some(control) = app.get_webview_window(OVERLAY_CONTROL_LABEL) {
            let _ = control.set_ignore_cursor_events(false);
            let _ = control.set_always_on_top(true);
            let _ = control.set_size(PhysicalSize::new(
                OVERLAY_CONTROL_WIDTH,
                OVERLAY_CONTROL_HEIGHT,
            ));
            position_overlay_control(app, &control);
            let _ = control.show();
        }

        if let Err(error) = app.emit(OVERLAY_STATE_EVENT, payload) {
            eprintln!("failed to emit overlay state: {error}");
        }
    }

    fn hide_overlay_windows<R>(&self, app: &AppHandle<R>)
    where
        R: Runtime,
    {
        if let Some(control) = app.get_webview_window(OVERLAY_CONTROL_LABEL) {
            let _ = control.hide();
        }
        if let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) {
            let _ = window.hide();
        }
    }
}

#[tauri::command]
pub async fn consent_allow(
    controller: tauri::State<'_, ConsentController>,
    id: String,
    remember_for_seconds: Option<u64>,
) -> Result<(), String> {
    controller
        .allow(&id, remember_for_seconds)
        .await
        .map_err(error_to_string)
}

#[tauri::command]
pub async fn consent_deny(
    controller: tauri::State<'_, ConsentController>,
    id: String,
) -> Result<(), String> {
    controller.deny(&id).await.map_err(error_to_string)
}

#[tauri::command]
pub async fn consent_revoke(
    app: AppHandle,
    controller: tauri::State<'_, ConsentController>,
) -> Result<(), String> {
    controller.revoke().await.map_err(error_to_string)?;
    controller.force_hide_overlay(&app);
    Ok(())
}

#[tauri::command]
pub async fn consent_resume(controller: tauri::State<'_, ConsentController>) -> Result<(), String> {
    controller.resume().await.map_err(error_to_string)
}

#[tauri::command]
pub async fn consent_state(
    controller: tauri::State<'_, ConsentController>,
) -> Result<ConsentState, String> {
    controller.state().await.map_err(error_to_string)
}

#[tauri::command]
pub fn overlay_dismiss(
    app: AppHandle,
    controller: tauri::State<'_, ConsentController>,
) -> Result<(), String> {
    controller.dismiss_overlay(&app);
    Ok(())
}

fn raise_main_window<R>(app: &AppHandle<R>)
where
    R: Runtime,
{
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn show_notification<R>(app: &AppHandle<R>, row: &ConsentRequest)
where
    R: Runtime,
{
    let operator = row.operator_id.as_deref().unwrap_or("Unbekannter Operator");
    let command = if row.cmd.is_empty() {
        "Remote-Befehl wartet auf Freigabe".to_string()
    } else {
        truncate(&row.cmd.join(" "), 140)
    };
    let body = format!("{operator}: {command}");

    if let Err(error) = app
        .notification()
        .builder()
        .title("Forge-Freigabe erforderlich")
        .body(body)
        .show()
    {
        eprintln!("failed to show consent notification: {error}");
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        truncated.push_str("...");
    }
    truncated
}

fn signed_operator_id(row: &ConsentRequest) -> Option<String> {
    row.operator_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn is_recent_activity(value: &Option<Value>) -> bool {
    let Some(timestamp_ms) = parse_timestamp_ms(value) else {
        return false;
    };
    let Some(now_ms) = now_unix_millis() else {
        return false;
    };

    now_ms >= timestamp_ms && now_ms - timestamp_ms < OPERATOR_ACTIVE_WINDOW_MS
}

fn parse_timestamp_ms(value: &Option<Value>) -> Option<i128> {
    match value {
        Some(Value::Number(number)) => number
            .as_i64()
            .map(|value| normalize_timestamp_ms(value as i128)),
        Some(Value::String(raw)) => raw.parse::<i128>().ok().map(normalize_timestamp_ms),
        _ => None,
    }
}

fn normalize_timestamp_ms(value: i128) -> i128 {
    if value < 1_000_000_000_000 {
        value * 1000
    } else {
        value
    }
}

fn now_unix_millis() -> Option<i128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i128)
}

fn position_overlay_control<R>(app: &AppHandle<R>, control: &tauri::WebviewWindow<R>)
where
    R: Runtime,
{
    let _ = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| {
            let pos = monitor.position();
            let size = monitor.size();
            let width = size.width as i32;
            let height = size.height as i32;
            let control_width = OVERLAY_CONTROL_WIDTH as i32;
            let control_height = OVERLAY_CONTROL_HEIGHT as i32;

            let min_x = pos.x + 16;
            let max_x = pos.x + width - control_width - 16;
            let target_x = pos.x + (width - control_width) / 2;
            let x = if min_x <= max_x {
                target_x.clamp(min_x, max_x)
            } else {
                pos.x
            };

            let min_y = pos.y + 24;
            let max_y = pos.y + height - control_height - 24;
            let target_y = pos.y + (height / 2) + 215;
            let y = if min_y <= max_y {
                target_y.clamp(min_y, max_y)
            } else {
                pos.y
            };

            let _ = control.set_position(PhysicalPosition::new(x, y));
        })
        .or_else(|| {
            let _ = control.center();
            None
        });
}

fn set_tray_tooltip<R>(app: &AppHandle<R>, remote_access: Option<&str>)
where
    R: Runtime,
{
    let Some(tray) = app.tray_by_id("forge-tray") else {
        return;
    };

    let bridge_label = app
        .try_state::<crate::sidecar::BridgeSupervisor>()
        .map(|supervisor| {
            if supervisor.status().online {
                "Bridge online"
            } else {
                "Bridge offline"
            }
        })
        .unwrap_or("Bridge unbekannt");
    let remote_label = match remote_access {
        Some("active") => "Remote-Zugriff aktiv",
        Some("revoked") => "Remote-Zugriff gesperrt",
        Some(_) => "Remote-Zugriff unbekannt",
        None => "Remote-Zugriff unbekannt",
    };

    let _ = tray.set_tooltip(Some(format!("u1 Forge - {bridge_label} - {remote_label}")));
}

fn default_remote_access() -> String {
    "active".to_string()
}

fn error_to_string(error: BridgeClientError) -> String {
    error.to_string()
}
