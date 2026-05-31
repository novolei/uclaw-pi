//! LLM-config-domain Tauri commands.
//!
//! No DB and no service: these operate directly on the in-memory
//! `state.llm_config` (`Arc<RwLock<LlmConfig>>`) and persist via
//! [`crate::config::llm::LlmConfig::save`] — the config object IS the logic
//! holder, so there is nothing to lift into a `services/` trait. Relocated
//! verbatim from `tauri_commands.rs` per the code-organization ADR (2026-05-31).

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{LlmConfigInput, LlmConfigResponse};

/// Read the current LLM config (api key elided to a `has_api_key` bool).
#[tauri::command]
pub async fn get_llm_config(state: State<'_, AppState>) -> Result<LlmConfigResponse, Error> {
    let config = state.llm_config.read().await;
    Ok(LlmConfigResponse {
        provider: config.provider.clone(),
        model: config.model.clone(),
        has_api_key: !config.api_key.is_empty(),
        base_url: config.base_url.clone(),
        max_tokens: config.max_tokens,
        temperature: config.temperature,
    })
}

/// Update + persist the LLM config. An empty `api_key` leaves the stored key
/// untouched (so the UI can save other fields without re-entering it).
#[tauri::command]
pub async fn update_llm_config(
    state: State<'_, AppState>,
    input: LlmConfigInput,
) -> Result<LlmConfigResponse, Error> {
    let mut config = state.llm_config.write().await;
    config.provider = input.provider;
    config.model = input.model;
    if !input.api_key.is_empty() {
        config.api_key = input.api_key;
    }
    config.base_url = input.base_url;
    config.max_tokens = input.max_tokens;
    config.temperature = input.temperature;

    config.save(&state.llm_config_path)?;

    Ok(LlmConfigResponse {
        provider: config.provider.clone(),
        model: config.model.clone(),
        has_api_key: !config.api_key.is_empty(),
        base_url: config.base_url.clone(),
        max_tokens: config.max_tokens,
        temperature: config.temperature,
    })
}
