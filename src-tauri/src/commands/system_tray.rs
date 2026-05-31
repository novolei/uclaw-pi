//! System-tray / badge-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), this domain has **no service**:
//! the single command takes no [`crate::app::AppState`] and touches no `state.db`
//! SQL — it only emits a `badge:updated` event over the [`tauri::AppHandle`] for
//! the UI to render. There is nothing to lift, so the JUDGMENT RULE resolves to a
//! thin move.
//!
//! Relocated verbatim from the legacy `tauri_commands.rs` god file (the
//! `// ─── System Tray / Badge Commands (Phase 3)` section): the 1
//! `#[tauri::command]`.

use tauri::Emitter;

use crate::error::Error;

/// Update the app's badge count. Emits a `badge:updated` event to the frontend
/// (the UI owns the actual badge display).
#[tauri::command]
pub async fn update_badge_count(
    app_handle: tauri::AppHandle,
    count: u32,
) -> Result<bool, Error> {
    // Emit badge update event to frontend (UI handles display)
    let _ = app_handle.emit("badge:updated", serde_json::json!({ "count": count }));
    Ok(true)
}
