//! Humane-automation-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), this domain has **no service**:
//! every command delegates to the [`crate::automation::runtime::AppRuntimeService`]
//! held in [`crate::app::AppState`] (`state.runtime_service`) — it owns spec
//! install/import/get, user-config + permission + enable toggles, uninstall,
//! escalation resolution/listing, and the per-spec memory read/compact, so there
//! is **no inline `state.db` SQL to lift** and the JUDGMENT RULE resolves to a
//! thin move.
//!
//! Relocated verbatim from the legacy `tauri_commands.rs` god file (the
//! `// ─── Humane Automation Commands (Phase 1 spec § 7.3)` section): the 11
//! `#[tauri::command]`s. The Marketplace section that follows in the god file is a
//! separate (deferred) domain and was left behind.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;

#[tauri::command]
pub async fn install_humane_spec(
    state: State<'_, AppState>,
    yaml: String,
    source_ref: Option<String>,
) -> Result<crate::automation::manager::HumaneSpecRow, Error> {
    state.runtime_service.install_humane_spec(&yaml, source_ref).await
        .map_err(|e| Error::Internal(e.to_string()))
}

#[tauri::command]
pub async fn import_humane_spec_file(
    state: State<'_, AppState>,
    path: String,
) -> Result<crate::automation::manager::HumaneSpecRow, Error> {
    state.runtime_service.import_humane_spec_file(&path).await
        .map_err(|e| Error::Internal(e.to_string()))
}

#[tauri::command]
pub async fn get_automation_spec(
    state: State<'_, AppState>,
    spec_id: String,
) -> Result<crate::automation::manager::HumaneSpecRow, Error> {
    state.runtime_service.get_spec(&spec_id)
        .map_err(|e| Error::Internal(e.to_string()))
}

#[tauri::command]
pub async fn update_user_config(
    state: State<'_, AppState>,
    spec_id: String,
    values: serde_json::Value,
) -> Result<(), Error> {
    state.runtime_service.update_user_config(&spec_id, &values)
        .map_err(|e| Error::Internal(e.to_string()))
}

#[tauri::command]
pub async fn set_automation_permission(
    state: State<'_, AppState>,
    spec_id: String,
    permission: String,
    granted: bool,
) -> Result<(), Error> {
    state.runtime_service.set_permission(&spec_id, &permission, granted).await
        .map_err(|e| Error::Internal(e.to_string()))
}

#[tauri::command]
pub async fn set_automation_enabled(
    state: State<'_, AppState>,
    spec_id: String,
    enabled: bool,
) -> Result<(), Error> {
    state.runtime_service.set_enabled(&spec_id, enabled).await
        .map_err(|e| Error::Internal(e.to_string()))
}

#[tauri::command]
pub async fn uninstall_automation(
    state: State<'_, AppState>,
    spec_id: String,
) -> Result<(), Error> {
    state.runtime_service.uninstall(&spec_id).await
        .map_err(|e| Error::Internal(e.to_string()))
}

#[tauri::command]
pub async fn resolve_escalation(
    state: State<'_, AppState>,
    escalation_id: String,
    choice: String,
    note: Option<String>,
) -> Result<(), Error> {
    state.runtime_service
        .resolve_escalation(&escalation_id, &choice, note.as_deref())
        .await
        .map_err(|e| Error::Internal(e.to_string()))
}

#[tauri::command]
pub async fn list_pending_escalations(
    state: State<'_, AppState>,
    spec_id: Option<String>,
) -> Result<Vec<crate::automation::runtime::EscalationRow>, Error> {
    state.runtime_service
        .list_pending_escalations(spec_id.as_deref())
        .map_err(|e| Error::Internal(e.to_string()))
}

#[tauri::command]
pub async fn read_automation_memory(
    state: State<'_, AppState>,
    spec_id: String,
) -> Result<String, Error> {
    state.runtime_service.read_memory(&spec_id).await
        .map_err(|e| Error::Internal(e.to_string()))
}

#[tauri::command]
pub async fn compact_automation_memory(
    state: State<'_, AppState>,
    spec_id: String,
) -> Result<String, Error> {
    state.runtime_service.compact_memory(&spec_id).await
        .map_err(|e| Error::Internal(e.to_string()))
}
