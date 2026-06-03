//! Provider readiness adapter for uClaw's existing provider registry/configs.
//!
//! This module derives reports from local configuration only. It does not
//! perform network probes, mutate credentials, or alter runtime provider
//! selection.

use uclaw_provider_core::{
    ProviderApiFamily, ProviderAuthRequirement, ProviderCapabilityFlags, ProviderCredentialStatus,
    ProviderProbeStatus, ProviderReadinessInput, ProviderReadinessReport, ProviderRuntimeHints,
    ProviderStreamingStatus,
};

use super::types::{ApiType, AuthType, KnownProvider, ModelSelection, ProviderConfig};

pub fn api_family_from_api_type(api_type: &ApiType) -> ProviderApiFamily {
    match api_type {
        ApiType::OpenAiCompletions => ProviderApiFamily::OpenAiCompletions,
        ApiType::AnthropicMessages => ProviderApiFamily::AnthropicMessages,
        ApiType::OpenAiResponses => ProviderApiFamily::OpenAiResponses,
        ApiType::OpenAiCodexResponses => ProviderApiFamily::OpenAiCodexResponses,
        // In-process local inference — no remote wire protocol.
        ApiType::LocalMistralRs => ProviderApiFamily::Unknown,
    }
}

pub fn auth_requirement_from_auth_type(auth_type: &AuthType) -> ProviderAuthRequirement {
    match auth_type {
        AuthType::ApiKey => ProviderAuthRequirement::ApiKey,
        AuthType::OAuth => ProviderAuthRequirement::OAuth,
        AuthType::None => ProviderAuthRequirement::None,
    }
}

pub fn credential_status_for(
    known: &KnownProvider,
    config: Option<&ProviderConfig>,
) -> ProviderCredentialStatus {
    match known.auth_type {
        AuthType::None => ProviderCredentialStatus::NotRequired,
        AuthType::ApiKey => config
            .and_then(|c| c.api_key.as_deref())
            .filter(|key| !key.trim().is_empty())
            .map(|_| ProviderCredentialStatus::Present)
            .unwrap_or(ProviderCredentialStatus::Missing),
        AuthType::OAuth => config
            .and_then(|c| c.api_key.as_deref())
            .filter(|key| !key.trim().is_empty())
            .map(|_| ProviderCredentialStatus::Present)
            .unwrap_or(ProviderCredentialStatus::Missing),
    }
}

pub fn capability_flags_for(known: &KnownProvider) -> ProviderCapabilityFlags {
    let api_family = api_family_from_api_type(&known.default_api);
    let id = known.id.as_str();

    ProviderCapabilityFlags {
        supports_model_listing: known.supports_models,
        supports_streaming: true,
        supports_image_input: matches!(id, "openai" | "gemini" | "openrouter"),
        supports_reasoning_effort: matches!(
            api_family,
            ProviderApiFamily::OpenAiCompletions
                | ProviderApiFamily::OpenAiResponses
                | ProviderApiFamily::OpenAiCodexResponses
        ),
        supports_prompt_cache: matches!(api_family, ProviderApiFamily::AnthropicMessages),
        supports_split_prompt: matches!(api_family, ProviderApiFamily::AnthropicMessages),
    }
}

pub fn runtime_hints_for(known: &KnownProvider) -> ProviderRuntimeHints {
    ProviderRuntimeHints {
        api_family: api_family_from_api_type(&known.default_api),
        auth_requirement: auth_requirement_from_auth_type(&known.auth_type),
        streaming_status: ProviderStreamingStatus::Assumed,
        capabilities: capability_flags_for(known),
    }
}

pub fn requires_base_url(known: &KnownProvider) -> bool {
    !matches!(known.default_api, ApiType::OpenAiCodexResponses)
}

pub fn assess_provider_readiness(
    provider_id: &str,
    known: Option<&KnownProvider>,
    config: Option<&ProviderConfig>,
    selected_models: &[ModelSelection],
    active_model: Option<&ModelSelection>,
) -> ProviderReadinessReport {
    let Some(known) = known else {
        return ProviderReadinessReport::new(ProviderReadinessInput {
            provider_id: provider_id.to_string(),
            display_name: provider_id.to_string(),
            api_family: ProviderApiFamily::Unknown,
            auth_requirement: ProviderAuthRequirement::None,
            credential_status: ProviderCredentialStatus::NotRequired,
            configured: false,
            requires_base_url: false,
            has_base_url: false,
            supports_models: false,
            selected_models: Vec::new(),
            active_model: None,
            probe_status: ProviderProbeStatus::NotRun,
            capabilities: ProviderCapabilityFlags::default(),
        });
    };

    let provider_models: Vec<String> = selected_models
        .iter()
        .filter(|model| model.provider_id == known.id)
        .map(|model| model.model_id.clone())
        .collect();
    let active_model = active_model
        .filter(|model| model.provider_id == known.id)
        .map(|model| model.model_id.clone());
    let base_url = config
        .and_then(|c| c.base_url.as_deref())
        .unwrap_or(known.default_base_url.as_str());

    ProviderReadinessReport::new(ProviderReadinessInput {
        provider_id: known.id.clone(),
        display_name: known.display_name.clone(),
        api_family: api_family_from_api_type(&known.default_api),
        auth_requirement: auth_requirement_from_auth_type(&known.auth_type),
        credential_status: credential_status_for(known, config),
        configured: config.is_some(),
        requires_base_url: requires_base_url(known),
        has_base_url: !base_url.trim().is_empty(),
        supports_models: known.supports_models,
        selected_models: provider_models,
        active_model,
        probe_status: ProviderProbeStatus::NotRun,
        capabilities: capability_flags_for(known),
    })
}

#[cfg(test)]
#[path = "readiness_tests.rs"]
mod tests;
