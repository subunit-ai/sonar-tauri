//! Tauri-Commands für den Trace-Space — dünne Hülle um die in-process Trace-Engine
//! (Crate `trace-engine`, eigenes Repo). Erfassung ist opt-in (start/stop), alles lokal.

use tauri::State;
use trace_engine::{ActivityEvent, AppUsage, TraceEngine, TraceStatus};

#[tauri::command]
pub fn trace_status(engine: State<'_, TraceEngine>) -> Result<TraceStatus, String> {
    engine.status()
}

#[tauri::command]
pub fn trace_start(engine: State<'_, TraceEngine>) -> Result<(), String> {
    engine.start()
}

#[tauri::command]
pub fn trace_stop(engine: State<'_, TraceEngine>) -> Result<(), String> {
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
