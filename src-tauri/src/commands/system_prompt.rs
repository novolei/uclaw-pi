//! System-prompt-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), all 7 commands have genuine
//! inline SQL (over `system_prompts`, `system_prompt_versions`, and the
//! `settings` keys `default_prompt_id` / `append_datetime_username`), so the
//! JUDGMENT RULE resolves to a real service: every body just locks `state.db`,
//! calls [`crate::services::system_prompt_service::DbSystemPrompt`], and maps
//! the result.
//!
//! The cross-cutting `invalidate_prompt_cache` is intentionally **not** here:
//! it owns a module-private cache shared with the agent prompt-build path
//! (`resolve_user_system_prompt` et al.) and stays in `tauri_commands.rs`; the
//! service calls back into it after each mutation. `resolve_user_system_prompt`
//! / `get_system_prompt` / `substitute_template_vars` are not used by any of
//! these commands (their callers are the agent loop) and were left untouched.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{
    SystemPromptConfigDto, SystemPromptCreateInput, SystemPromptDto, SystemPromptUpdateInput,
    SystemPromptVersionDto,
};
use crate::services::system_prompt_service::{DbSystemPrompt, SystemPromptService};

/// Load all system prompts and the global default prompt ID.
#[tauri::command]
pub async fn get_system_prompt_config(
    state: State<'_, AppState>,
) -> Result<SystemPromptConfigDto, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    DbSystemPrompt.config(&conn)
}

/// Create a new user-defined system prompt.
#[tauri::command]
pub async fn create_system_prompt(
    state: State<'_, AppState>,
    input: SystemPromptCreateInput,
) -> Result<SystemPromptDto, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    DbSystemPrompt.create(&conn, input)
}

/// Delete a user-defined system prompt (built-in prompts are protected).
#[tauri::command]
pub async fn delete_system_prompt(state: State<'_, AppState>, id: String) -> Result<(), Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    DbSystemPrompt.delete(&conn, &id)
}

/// Update a system prompt's name and/or content. Built-in prompts are read-only.
#[tauri::command]
pub async fn update_system_prompt(
    state: State<'_, AppState>,
    id: String,
    input: SystemPromptUpdateInput,
) -> Result<SystemPromptDto, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    DbSystemPrompt.update(&conn, &id, input)
}

/// Set the global default system prompt ID.
#[tauri::command]
pub async fn set_default_prompt(state: State<'_, AppState>, id: String) -> Result<(), Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    DbSystemPrompt.set_default(&conn, &id)
}

/// Retrieve version history for a system prompt (newest first).
#[tauri::command]
pub async fn get_system_prompt_versions(
    state: State<'_, AppState>,
    prompt_id: String,
) -> Result<Vec<SystemPromptVersionDto>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    DbSystemPrompt.versions(&conn, &prompt_id)
}

/// Update the "append date/time and username" preference.
#[tauri::command]
pub async fn update_append_setting(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    DbSystemPrompt.set_append_setting(&conn, enabled)
}
