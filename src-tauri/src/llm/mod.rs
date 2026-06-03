pub mod provider;
pub mod providers;
pub mod stream_error;
// M1-T7 — eager prewarm of HTTP/2+TLS to the active LLM provider.
pub mod prewarm;

pub use provider::{CompletionConfig, LlmProvider};
pub use providers::anthropic::AnthropicProvider;
pub use providers::openai::OpenAIProvider;
pub use stream_error::{classify_stream_error, StreamErrorKind};

use crate::config::llm::LlmConfig;
use crate::providers::types::ApiType;
use std::sync::Arc;

/// 决定某 provider 用哪种 wire API。config_api(ProviderConfig.api 覆盖)优先;
/// 否则 "anthropic" id → AnthropicMessages,其余 → OpenAiCompletions(同历史行为)。
pub(crate) fn resolve_api(provider_id: &str, config_api: Option<ApiType>) -> ApiType {
    config_api.unwrap_or_else(|| {
        if provider_id == "anthropic" { ApiType::AnthropicMessages } else { ApiType::OpenAiCompletions }
    })
}

/// Create an LLM provider from config.
///
/// Routes via `resolve_api`: AnthropicMessages → AnthropicProvider;
/// all other wire types → OpenAIProvider (covers OpenAI-compatible endpoints
/// for DeepSeek, Moonshot, Zhipu, Ollama, etc.).
pub fn create_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>, crate::error::Error> {
    match resolve_api(&config.provider, config.api.clone()) {
        ApiType::AnthropicMessages => Ok(Arc::new(AnthropicProvider::new(
            config.api_key.clone(),
            config.base_url.clone(),
        ))),
        ApiType::LocalMistralRs => Ok(Arc::new(
            crate::local_llm::provider::LocalMistralRsProvider::from_global(config.model.clone())?,
        )),
        _ => Ok(Arc::new(OpenAIProvider::new(
            config.api_key.clone(),
            config.base_url.clone(),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_api;
    use crate::providers::types::ApiType;

    #[test]
    fn explicit_anthropic_messages_routes_anthropic() {
        assert_eq!(resolve_api("kimi-coding", Some(ApiType::AnthropicMessages)), ApiType::AnthropicMessages);
    }
    #[test]
    fn none_with_anthropic_id_backcompat() {
        assert_eq!(resolve_api("anthropic", None), ApiType::AnthropicMessages);
    }
    #[test]
    fn none_with_other_id_is_openai() {
        assert_eq!(resolve_api("deepseek", None), ApiType::OpenAiCompletions);
    }
    #[test]
    fn explicit_override_wins() {
        assert_eq!(resolve_api("anthropic", Some(ApiType::OpenAiCompletions)), ApiType::OpenAiCompletions);
    }
}

/// Build an `LlmConfig` from the active provider service model.
/// Returns `None` if no active model is configured.
pub fn llm_config_from_provider(
    provider_id: &str,
    model: &str,
    api_key: &str,
    base_url: &str,
    max_tokens: u32,
    temperature: f32,
    api: Option<ApiType>,
) -> LlmConfig {
    LlmConfig {
        provider: provider_id.to_string(),
        model: model.to_string(),
        api_key: api_key.to_string(),
        base_url: if base_url.is_empty() { None } else { Some(base_url.to_string()) },
        max_tokens: Some(max_tokens),
        temperature: Some(temperature),
        api,
    }
}
