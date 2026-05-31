//! Background-task-domain Tauri commands.
//!
//! No DB and no new service: this delegates to the in-memory
//! [`crate::background::BackgroundTaskManager`] held in `state.background_tasks`
//! (the manager IS the service). Relocated verbatim from `tauri_commands.rs`
//! per the code-organization ADR (2026-05-31).

use tauri::State;

use crate::app::AppState;
use crate::error::Error;

/// Snapshot of the current background tasks (owned clones for IPC).
#[tauri::command]
pub async fn get_background_tasks(
    state: State<'_, AppState>,
) -> Result<Vec<crate::background::BackgroundTask>, Error> {
    let mgr = state.background_tasks.lock().await;
    Ok(mgr.list().into_iter().cloned().collect())
}
