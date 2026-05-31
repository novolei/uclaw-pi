//! Settings-domain Tauri commands — thin wrappers over
//! [`crate::services::settings_service`]. No SQL or business logic here (it lives
//! in the service); these just lock the DB, call the service, and map errors.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::services::settings_service::{DbSettings, SettingsService};

/// Whether the optional local HTTP API server is enabled (persisted; the startup
/// gate in `main.rs` reads the same flag). Changing it applies after restart.
#[tauri::command]
pub async fn get_http_api_enabled(state: State<'_, AppState>) -> Result<bool, Error> {
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    Ok(DbSettings.http_api_enabled(&conn))
}

/// Enable/disable the optional local HTTP API server. Persisted; applies on the
/// next app restart (the server thread is spawned once, at startup).
#[tauri::command]
pub async fn set_http_api_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), Error> {
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    DbSettings
        .set_http_api_enabled(&conn, enabled)
        .map_err(Error::Database)
}
