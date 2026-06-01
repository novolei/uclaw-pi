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

/// Whether the agent routes through the experimental PiEngine backend (vs the
/// legacy loop). Reflects the live runtime value (Settings override > env var).
#[tauri::command]
pub async fn get_pi_engine_enabled(_state: State<'_, AppState>) -> Result<bool, Error> {
    Ok(crate::engine_sink::pi_engine_enabled())
}

/// Switch the agent engine backend. Persisted AND applied at runtime — takes
/// effect on the next message (a conversation boundary), not mid-stream. pi is
/// experimental (R4: MCP/browser/skill tools not yet wired); legacy is the
/// safe default.
#[tauri::command]
pub async fn set_pi_engine_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), Error> {
    {
        let conn = state
            .db
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
        DbSettings
            .set_pi_engine_enabled(&conn, enabled)
            .map_err(Error::Database)?;
    }
    crate::engine_sink::set_pi_engine_override(Some(enabled));
    Ok(())
}

/// Whether a skills.sh API key is stored (status only — the key itself is never
/// returned to the frontend). The Settings card uses this to show 已设置/未设置.
#[tauri::command]
pub async fn get_skills_sh_api_key_set(state: State<'_, AppState>) -> Result<bool, Error> {
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    Ok(DbSettings.skills_sh_api_key_set(&conn))
}

/// Store (or clear, with an empty string) the skills.sh API key the marketplace
/// client authenticates with. Persisted in the `settings` table; applies on the
/// next marketplace search/install (the key is read per request).
#[tauri::command]
pub async fn set_skills_sh_api_key(state: State<'_, AppState>, key: String) -> Result<(), Error> {
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    DbSettings
        .set_skills_sh_api_key(&conn, &key)
        .map_err(Error::Database)
}
