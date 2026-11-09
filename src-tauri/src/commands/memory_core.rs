//! MemoryCore (hybrid retrieval engine) Tauri commands — thin wrappers over
//! [`crate::memory_core::service::ServiceHandle`]. Live status is also pushed
//! via the `memory-core:status` event (payload: `MemoryCoreStatus`).

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::memory_core::status::MemoryCoreStatus;

#[tauri::command]
pub async fn memory_core_get_status(state: State<'_, AppState>) -> Result<MemoryCoreStatus, Error> {
    Ok(state.memory_core_service.status())
}

/// Persist the models toggle, then apply it at runtime (on: download + load;
/// off: stop retrying, unload, keyword mode).
#[tauri::command]
pub async fn memory_core_set_models_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<MemoryCoreStatus, Error> {
    state
        .memory_core_service
        .set_models_enabled(enabled)
        .await
        .map_err(|e| Error::Internal(format!("memory_core_set_models_enabled: {e:#}")))
}

/// Retry what failed: a failed model download (phase `failed`), or a failed
/// model load (`loadError`) — re-runs the load when the files are on disk,
/// otherwise re-downloads. No-op when nothing failed.
#[tauri::command]
pub async fn memory_core_retry_download(state: State<'_, AppState>) -> Result<MemoryCoreStatus, Error> {
    Ok(state.memory_core_service.retry_download().await)
}
