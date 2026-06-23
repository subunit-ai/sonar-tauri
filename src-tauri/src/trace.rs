//! Tauri-Commands für den Trace-Space — dünne Hülle um die in-process Trace-Engine
//! (Crate `trace-engine`, eigenes Repo). Erfassung ist opt-in (start/stop), alles lokal.

use crate::bridge_client::BridgeClient;
use crate::config::AppState;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use trace_engine::{ActivityEvent, AppUsage, TraceEngine, TraceStatus};

#[tauri::command]
pub fn trace_status(engine: State<'_, TraceEngine>) -> Result<TraceStatus, String> {
    engine.status()
}

#[tauri::command]
pub fn trace_start(engine: State<'_, TraceEngine>, state: State<'_, AppState>) -> Result<(), String> {
    // Flag persistieren → der Auto-Arm beim nächsten Start (lib.rs setup) erfasst wieder,
    // selbst wenn die App geschlossen oder das Gerät neu gestartet wurde.
    {
        let mut cfg = state.config.lock();
        cfg.trace_capture_enabled = true;
        if let Err(error) = cfg.save() {
            eprintln!("[trace] Capture-Flag speichern fehlgeschlagen: {error}");
        }
    }
    engine.start()
}

#[tauri::command]
pub fn trace_stop(engine: State<'_, TraceEngine>, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut cfg = state.config.lock();
        cfg.trace_capture_enabled = false;
        if let Err(error) = cfg.save() {
            eprintln!("[trace] Capture-Flag speichern fehlgeschlagen: {error}");
        }
    }
    engine.stop()
}

#[tauri::command]
pub fn trace_recent_events(
    engine: State<'_, TraceEngine>,
    limit: u32,
) -> Result<Vec<ActivityEvent>, String> {
    // Limit klemmen — kein unbegrenztes Vec aus dem Renderer. (Codex-P2.5)
    engine.recent_events(limit.clamp(1, 1000))
}

#[tauri::command]
pub fn trace_app_usage(
    engine: State<'_, TraceEngine>,
    since: i64,
) -> Result<Vec<AppUsage>, String> {
    engine.app_usage(since)
}

/// Baut den nächsten Upload-Batch aus noch nicht synchronisierten UI-Events und übergibt
/// ihn der Bridge-Outbox (Kind `activity.batch`). Gibt die Anzahl übergebener Events zurück
/// (0 = nichts offen). Events werden ERST nach erfolgreichem Enqueue als synchronisiert
/// markiert → bei Bridge-Fehler bleiben sie offen und werden später erneut versucht.
#[tauri::command]
pub async fn trace_sync_batch(engine: State<'_, TraceEngine>) -> Result<u32, String> {
    // Klein batchen: ein activity.batch-Outbox-Eintrag muss unter dem Server-Body-Limit
    // bleiben (der Outbox-Worker bündelt mehrere). Rest holt der nächste Sync-Zyklus.
    let batch = match engine.pending_batch(100)? {
        Some(batch) => batch,
        None => return Ok(0),
    };
    let count = batch.events.len() as u32;
    let ids: Vec<i64> = batch.events.iter().map(|e| e.source_local_id).collect();

    let client = BridgeClient::new()?;
    client
        .post_authed_json("/activity", &batch)
        .await
        .map_err(|e| e.to_string())?;

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    engine.mark_ui_events_synced(&ids, now_ms)?;
    Ok(count)
}
