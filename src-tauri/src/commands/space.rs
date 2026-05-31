//! Space-domain Tauri commands — thin wrappers over
//! [`crate::services::space_service`]. No SQL or business logic here (it lives in
//! the service); these just lock the DB, call the service, and map errors.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{CreateSpaceInput, SpaceResponse};
use crate::services::space_service::{DbSpace, SpaceService};

/// Create a new workspace (sorts last).
#[tauri::command]
pub async fn create_space(
    state: State<'_, AppState>,
    input: CreateSpaceInput,
) -> Result<SpaceResponse, Error> {
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    DbSpace.create(&conn, input)
}

/// List workspaces, backfilling any NULL `path` (compute dir → mkdir → persist).
#[tauri::command]
pub async fn list_spaces(state: State<'_, AppState>) -> Result<Vec<SpaceResponse>, Error> {
    let workspace_root = state.workspace_root.clone();
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    DbSpace.list(&conn, &workspace_root)
}

/// Delete a workspace by id. Returns whether a row was removed.
#[tauri::command]
pub async fn delete_space(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    DbSpace.delete(&conn, &id)
}
