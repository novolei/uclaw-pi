//! Provider-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), this domain has **no service**:
//! every config/model command delegates to the in-memory
//! [`crate::providers::ProviderService`] held in `state.provider_service` (an
//! `Arc`). That manager *is* the logic holder — config CRUD, model selection,
//! role-model assignment, connection testing all live there, none of it inline
//! SQL — so the JUDGMENT RULE resolves to a thin move. `list_providers` is fully
//! static (the built-in [`crate::providers::registry`]).
//!
//! `parse_api_type` was Provider-only and lives here as a module-private helper.
//! `mask_key` is shared (a unit-test module in `tauri_commands.rs` exercises it)
//! and stays `pub(crate)` there, imported here.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{
    ListModelsInput, ModelInfo, ModelSelectionInfo, ProviderConfigInput, ProviderConfigResponse,
    ProviderInfo, TestConnectionInput, TestResultInfo,
};
use crate::tauri_commands::mask_key;

/// Parse a UI API-type string into the typed [`crate::providers::types::ApiType`].
/// Provider-only; the inverse `{:?}` formatting is inlined at the read sites.
fn parse_api_type(s: &str) -> Option<crate::providers::types::ApiType> {
    match s {
        "OpenAiCompletions" | "openai_completions" | "openai-completions" => {
            Some(crate::providers::types::ApiType::OpenAiCompletions)
        }
        "AnthropicMessages" | "anthropic_messages" | "anthropic-messages" => {
            Some(crate::providers::types::ApiType::AnthropicMessages)
        }
        "OpenAiResponses" | "openai_responses" | "openai-responses" => {
            Some(crate::providers::types::ApiType::OpenAiResponses)
        }
        "OpenAiCodexResponses" | "openai_codex_responses" | "openai-codex-responses" => {
            Some(crate::providers::types::ApiType::OpenAiCodexResponses)
        }
        _ => None,
    }
}

/// List all built-in providers.
#[tauri::command]
pub fn list_providers() -> Vec<ProviderInfo> {
    crate::providers::registry::all()
        .iter()
        .map(|p| ProviderInfo {
            id: p.id.to_string(),
            display_name: p.display_name.to_string(),
            auth_type: format!("{:?}", p.auth_type).to_lowercase(),
            default_base_url: p.default_base_url.to_string(),
            default_api: format!("{:?}", p.default_api),
            service_category: format!("{:?}", p.service_category),
            geo_category: format!("{:?}", p.geo_category),
            supports_models: p.supports_models,
        })
        .collect()
}

/// List all configured provider IDs.
#[tauri::command]
pub async fn list_configured_providers(state: State<'_, AppState>) -> Result<Vec<String>, Error> {
    Ok(state.provider_service.list_configured_ids().await)
}

/// Get saved provider config.
#[tauri::command]
pub async fn get_provider_config(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<Option<ProviderConfigResponse>, Error> {
    let config = state.provider_service.get_provider_config(&provider_id).await;
    Ok(config.map(|c| {
        let api_key = c.api_key.filter(|k| !k.is_empty());
        ProviderConfigResponse {
            provider_id: c.provider_id,
            display_name: c.display_name,
            has_api_key: api_key.is_some(),
            masked_key: api_key.as_deref().map(mask_key),
            base_url: c.base_url,
            api: c.api.map(|a| format!("{:?}", a)),
        }
    }))
}

/// Save a provider configuration.
#[tauri::command]
pub async fn configure_provider(
    state: State<'_, AppState>,
    input: ProviderConfigInput,
) -> Result<(), Error> {
    let config = crate::providers::types::ProviderConfig {
        provider_id: input.provider_id,
        display_name: input.display_name,
        api_key: input.api_key.filter(|k| !k.is_empty()),
        base_url: input.base_url.filter(|u| !u.is_empty()),
        api: input.api.and_then(|a| parse_api_type(&a)),
    };
    state.provider_service.configure_provider(config).await
}

/// Save a provider configuration with model selections.
#[tauri::command]
pub async fn configure_provider_with_models(
    state: State<'_, AppState>,
    provider_config: ProviderConfigInput,
    model_ids: Vec<String>,
) -> Result<(), Error> {
    let config = crate::providers::types::ProviderConfig {
        provider_id: provider_config.provider_id,
        display_name: provider_config.display_name,
        api_key: provider_config.api_key.filter(|k| !k.is_empty()),
        base_url: provider_config.base_url.filter(|u| !u.is_empty()),
        api: provider_config.api.and_then(|a| parse_api_type(&a)),
    };
    state
        .provider_service
        .configure_provider_with_models(config, &model_ids)
        .await
}

/// Remove a provider configuration.
#[tauri::command]
pub async fn remove_provider_config(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<(), Error> {
    state.provider_service.remove_provider(&provider_id).await
}

/// Test provider connection.
#[tauri::command]
pub async fn test_provider_connection(
    state: State<'_, AppState>,
    input: TestConnectionInput,
) -> Result<TestResultInfo, Error> {
    let result = state
        .provider_service
        .test_connection(
            &input.provider_id,
            &input.base_url,
            input.api_key.as_deref(),
        )
        .await;
    Ok(TestResultInfo {
        success: result.success,
        message: result.message,
        latency_ms: result.latency_ms,
        details: result.details,
    })
}

/// List available models from a provider.
#[tauri::command]
pub async fn list_provider_models(
    state: State<'_, AppState>,
    input: ListModelsInput,
) -> Result<Vec<ModelInfo>, Error> {
    let models = state
        .provider_service
        .list_models(&input.provider_id, &input.base_url, input.api_key.as_deref())
        .await
        .map_err(|e| Error::Internal(format!("Failed to list models: {e}")))?;

    Ok(models
        .into_iter()
        .map(|m| ModelInfo {
            id: m.id,
            name: m.name,
            context_window: m.context_window,
            max_tokens: m.max_tokens,
            modality: format!("{:?}", m.modality),
            reasoning: m.reasoning,
            supports_reasoning_effort: m.supports_reasoning_effort,
        })
        .collect())
}

/// Get configured models for a specific provider.
#[tauri::command]
pub async fn get_configured_models(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<Vec<String>, Error> {
    Ok(state.provider_service.get_configured_models(&provider_id).await)
}

/// Get all configured models grouped by provider.
#[tauri::command]
pub async fn get_all_configured_models(
    state: State<'_, AppState>,
) -> Result<Vec<(String, Vec<String>)>, Error> {
    Ok(state.provider_service.get_all_configured_models().await)
}

/// Get the current active model.
#[tauri::command]
pub async fn get_active_model(
    state: State<'_, AppState>,
) -> Result<Option<ModelSelectionInfo>, Error> {
    let selection = state.provider_service.get_active_model().await;
    Ok(selection.map(|s| ModelSelectionInfo {
        provider_id: s.provider_id,
        model_id: s.model_id,
    }))
}

/// Set the active model.
#[tauri::command]
pub async fn set_active_model(
    state: State<'_, AppState>,
    provider_id: String,
    model_id: String,
) -> Result<(), Error> {
    state
        .provider_service
        .select_model(&provider_id, &model_id)
        .await
}

/// Get all per-role model assignments.
#[tauri::command]
pub async fn get_role_models(
    state: State<'_, AppState>,
) -> Result<Vec<crate::providers::types::ModelRoleConfig>, Error> {
    Ok(state.provider_service.get_role_models().await)
}

/// Set (or clear) the model assigned to a specific role.
/// Pass `model_ref` as `None` to clear the assignment.
#[tauri::command]
pub async fn set_role_model(
    state: State<'_, AppState>,
    role: String,
    model_ref: Option<String>,
) -> Result<(), Error> {
    state
        .provider_service
        .set_role_model(&role, model_ref)
        .await
}
