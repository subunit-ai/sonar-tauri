mod auth;
mod bridge_client;
mod config;
mod consent;
mod sidecar;
mod supply_chain;
mod trace;

use serde::Serialize;
use std::{
    env, fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

const TRAY_ID: &str = "sonar-tray";
const MENU_SHOW: &str = "sonar-open";
const MENU_HELP: &str = "sonar-help";
const MENU_STOP: &str = "sonar-stop";
const MENU_QUIT: &str = "sonar-quit";

#[tauri::command]
fn bridge_status(supervisor: tauri::State<'_, sidecar::BridgeSupervisor>) -> sidecar::BridgeStatus {
    supervisor.status()
}

#[derive(Serialize)]
struct HelpRequestResult {
    delivered: bool,
    via: String,
    message: String,
}

#[tauri::command]
async fn help_request() -> Result<HelpRequestResult, String> {
    request_help().await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Crash/Error-Reporting. No-op solange SONAR_SENTRY_DSN nicht gesetzt ist (DSN lebt
    // in CI/Env, NIE im Repo). DSN wird zur Compile-Zeit eingebacken (option_env!), damit
    // ausgelieferte Binaries ohne Env-Var auf der Nutzer-Maschine melden. Leer = deaktiviert.
    // DSGVO: keine Trace-/Fensterinhalte in Events — nur Fehlertyp/Stacktrace/Plattform.
    let _sentry = sentry::init((
        option_env!("SONAR_SENTRY_DSN").unwrap_or(""),
        sentry::ClientOptions {
            release: sentry::release_name!(),
            ..Default::default()
        },
    ));

    // Alle States VOR dem Builder konstruieren und am Builder managen. Das Webview kann
    // Commands (account_state, bridge_status, consent_state, …) invoken, BEVOR ein erst in
    // setup() gemanagtes State verfügbar ist → "state() called before manage()"-Panic beim
    // Start (deterministisch auf langsameren Maschinen, z.B. Eriks Tablet). Builder-managed =
    // vor jedem Command garantiert vorhanden, kein Race.
    let supervisor = sidecar::BridgeSupervisor::new().expect("bridge supervisor init");
    let consent_controller = consent::ConsentController::new().expect("consent controller init");
    // Trace-Engine (Task-Mining) — lokale DB im Sonar-Config-Dir, Erfassung default AUS.
    let trace_db = dirs::config_dir()
        .expect("no config dir")
        .join("sonar")
        .join("trace.db");
    if let Some(parent) = trace_db.parent() {
        let _ = fs::create_dir_all(parent);
        // Verzeichnis 0700 — Trace-Daten (Fenstertitel) vor anderen lokalen Accounts schützen. (Codex-P1.3)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }
    let trace_engine = trace_engine::TraceEngine::new(trace_db).expect("trace engine init");

    // Klone für den setup()-Hook — start() braucht sie nach dem Move ins manage().
    let supervisor_for_setup = supervisor.clone();
    let consent_for_setup = consent_controller.clone();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("Sonar")
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .manage(config::AppState::load())
        .manage(supervisor)
        .manage(consent_controller)
        .manage(trace_engine)
        .setup(move |app| {
            supervisor_for_setup.start(app.handle().clone());
            consent_for_setup.start(app.handle().clone());
            setup_tray(app)?;
            if let Err(error) = app.autolaunch().enable() {
                eprintln!("failed to enable autostart: {error}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bridge_status,
            help_request,
            consent::consent_allow,
            consent::consent_deny,
            consent::consent_revoke,
            consent::consent_resume,
            consent::consent_state,
            consent::overlay_dismiss,
            auth::account_state,
            auth::account_login,
            auth::account_logout,
            trace::trace_status,
            trace::trace_start,
            trace::trace_stop,
            trace::trace_recent_events,
            trace::trace_app_usage
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| match event {
        tauri::RunEvent::WindowEvent {
            label,
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } if label == "main" => {
            api.prevent_close();
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.hide();
            }
        }
        tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit => {
            app_handle.state::<sidecar::BridgeSupervisor>().stop();
        }
        _ => {}
    });
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let show = MenuItemBuilder::with_id(MENU_SHOW, "Sonar öffnen").build(app)?;
    let help = MenuItemBuilder::with_id(MENU_HELP, "Hilfe anfordern").build(app)?;
    let stop = MenuItemBuilder::with_id(MENU_STOP, "Stop (Zugriff sperren)").build(app)?;
    let quit = MenuItemBuilder::with_id(MENU_QUIT, "Beenden").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&show)
        .item(&help)
        .item(&stop)
        .separator()
        .item(&quit)
        .build()?;

    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("Sonar - Bridge unbekannt - Remote-Zugriff unbekannt")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_SHOW => show_main_window(app),
            MENU_HELP => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let result = request_help().await;
                    notify_help_result(&app, result);
                });
            }
            MENU_STOP => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let controller = app.state::<consent::ConsentController>();
                    match controller.revoke().await {
                        Ok(()) => {
                            controller.force_hide_overlay(&app);
                            show_notification(
                                &app,
                                "Sonar",
                                "Fernzugriff gesperrt - u1 hat jetzt keinen Zugriff.",
                            );
                        }
                        Err(error) => show_notification(
                            &app,
                            "Sonar",
                            &format!("Fernzugriff konnte nicht gesperrt werden: {error}"),
                        ),
                    }
                });
            }
            MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app)?;
    Ok(())
}

fn show_main_window<R>(app: &AppHandle<R>)
where
    R: Runtime,
{
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

async fn request_help() -> Result<HelpRequestResult, String> {
    if let Ok(client) = bridge_client::BridgeClient::new() {
        if client.post_help_request().await.is_ok() {
            return Ok(HelpRequestResult {
                delivered: true,
                via: "bridge".to_string(),
                message: "Anfrage an die Bridge gesendet.".to_string(),
            });
        }
    }

    write_help_marker().map(|path| HelpRequestResult {
        delivered: false,
        via: "marker".to_string(),
        message: format!(
            "Anfrage lokal notiert; die Bridge verarbeitet {}.",
            path.display()
        ),
    })
}

fn write_help_marker() -> Result<PathBuf, String> {
    let dir = bridge_config_dir()?;
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create {}: {error}", dir.display()))?;
    let path = dir.join("forge-help-request.json");
    let ts_unix_ms = unix_timestamp_ms();
    let body = serde_json::json!({
        "ts": rfc3339_from_unix_ms(ts_unix_ms),
        "ts_unix_ms": ts_unix_ms,
        "kind": "forge.help_request"
    });
    fs::write(&path, body.to_string())
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    Ok(path)
}

fn bridge_config_dir() -> Result<PathBuf, String> {
    if cfg!(windows) {
        let home = env::var_os("USERPROFILE")
            .filter(|value| !value.is_empty())
            .or_else(|| env::var_os("HOME").filter(|value| !value.is_empty()))
            .ok_or_else(|| "USERPROFILE is unavailable".to_string())?;
        return Ok(PathBuf::from(home).join(".config/subunit-bridge"));
    }

    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(config_home).join("subunit-bridge"));
    }

    let home = env::var_os("HOME").ok_or_else(|| "HOME is unavailable".to_string())?;
    Ok(PathBuf::from(home).join(".config/subunit-bridge"))
}

fn unix_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn rfc3339_from_unix_ms(timestamp_ms: u128) -> String {
    let seconds = (timestamp_ms / 1000) as i64;
    let millis = (timestamp_ms % 1000) as u32;
    let days = seconds.div_euclid(86_400);
    let second_of_day = seconds.rem_euclid(86_400);
    let hour = second_of_day / 3_600;
    let minute = (second_of_day % 3_600) / 60;
    let second = second_of_day % 60;
    let (year, month, day) = civil_from_days(days);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_phase = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_phase + 2) / 5 + 1;
    let month = month_phase + if month_phase < 10 { 3 } else { -9 };

    if month <= 2 {
        year += 1;
    }

    (year, month, day)
}

fn notify_help_result<R>(app: &AppHandle<R>, result: Result<HelpRequestResult, String>)
where
    R: Runtime,
{
    match result {
        Ok(help) => show_notification(app, "Sonar", &help.message),
        Err(error) => show_notification(
            app,
            "Sonar",
            &format!("Anfrage konnte nicht gesendet werden: {error}"),
        ),
    }
}

fn show_notification<R>(app: &AppHandle<R>, title: &str, body: &str)
where
    R: Runtime,
{
    if let Err(error) = app.notification().builder().title(title).body(body).show() {
        eprintln!("failed to show notification: {error}");
    }
}
