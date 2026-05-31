//! Automation-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31) this is a **mixed** domain:
//!
//! - `list_automations` / `trigger_automation_manual` / `stop_automation_runs` /
//!   `get_automation_activity` delegate to the async
//!   [`crate::automation::runtime::AppRuntimeService`] held in
//!   `state.runtime_service` — that service *is* the logic holder, so the
//!   JUDGMENT RULE resolves to a thin move.
//! - `get_or_create_spec_home_thread` carries inline `state.db` SQL (find-or-
//!   create the per-spec `agent_sessions` home thread) → that SQL is lifted into
//!   [`crate::services::automation_service::DbAutomation`]. The command here just
//!   locks `state.db` and delegates.
//!
//! Relocated from the legacy `tauri_commands.rs` god file.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::services::automation_service::{AutomationService, DbAutomation};

// list_automations — returns Vec<HumaneSpecRow> (V20 schema)
#[tauri::command]
pub async fn list_automations(
    state: State<'_, AppState>,
) -> Result<Vec<crate::automation::manager::HumaneSpecRow>, Error> {
    state.runtime_service.list_specs()
        .map_err(|e| Error::Internal(e.to_string()))
}

// trigger_automation_manual — delegates to AppRuntimeService
#[tauri::command]
pub async fn trigger_automation_manual(
    state: State<'_, AppState>,
    spec_id: String,
) -> Result<(), Error> {
    state.runtime_service.trigger_manual(&spec_id).await
        .map_err(|e| Error::Internal(e.to_string()))
}

#[tauri::command]
pub async fn stop_automation_runs(
    state: State<'_, AppState>,
    spec_id: String,
) -> Result<usize, Error> {
    state.runtime_service.stop_active_runs(&spec_id).await
        .map_err(|e| Error::Internal(e.to_string()))
}

// get_automation_activity — queries V20 schema via AppRuntimeService
#[tauri::command]
pub async fn get_automation_activity(
    state: State<'_, AppState>,
    spec_id: String,
    limit: Option<usize>,
) -> Result<Vec<crate::automation::activity::AutomationActivity>, Error> {
    state.runtime_service.get_activity(&spec_id, limit.unwrap_or(20))
        .map_err(|e| Error::Internal(e.to_string()))
}

#[tauri::command]
pub async fn get_or_create_spec_home_thread(
    state: State<'_, AppState>,
    spec_id: String,
) -> Result<serde_json::Value, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    DbAutomation
        .get_or_create_home_thread(&conn, &spec_id)
        .map_err(Error::Internal)
}
