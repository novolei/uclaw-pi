// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local LLM-domain Tauri commands (S1) — thin wrappers over
//! [`crate::local_llm`]: presence check + ModelScope download.
//!
//! Per the code-organization ADR (2026-05-31), command bodies are thin: parse
//! input → call the local-llm helpers → map result/error → emit event. The
//! engine handle + idle-unload reaper are wired in `main.rs`/`app.rs`, not here.

use tauri::{AppHandle, Emitter, State};

use crate::app::AppState;
use crate::error::Error;

/// True iff the local MiniCPM GGUF is downloaded.
#[tauri::command]
pub async fn is_local_model_present(state: State<'_, AppState>) -> Result<bool, Error> {
    Ok(crate::local_llm::paths::is_model_present(
        &state.data_dir,
        crate::local_llm::download::Quant::default(),
    ))
}

/// Download the MiniCPM Q4_K_M GGUF from ModelScope, emitting progress on
/// `local-model:download-progress` (`{ downloaded, total }`). Returns the
/// absolute path to the downloaded GGUF on success.
#[tauri::command]
pub async fn download_local_model(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, Error> {
    let data_dir = state.data_dir.clone();
    let path = crate::local_llm::download::download_from_modelscope(&data_dir, |downloaded, total| {
        let _ = app.emit(
            "local-model:download-progress",
            serde_json::json!({ "downloaded": downloaded, "total": total }),
        );
    })
    .await
    .map_err(|e| Error::Internal(format!("download failed: {e}")))?;
    Ok(path.to_string_lossy().to_string())
}
