//! Provider module type definitions.
//!
//! Defines core types for the provider registry, model metadata,
//! configuration storage, and connection testing results.
//!
//! Ported from if2Ai's provider/types.rs and config/types.rs.

use serde::{Deserialize, Serialize};

use crate::local_llm::download::quant::Quant;

// ── Provider Category ───────────────────────────────────────────────────────

/// Provider category for grouping in the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCategory {
    /// Domestic Chinese providers (Moonshot, Zhipu, DashScope, etc.)
    Domestic,
    /// International providers (OpenAI, Anthropic, Google, etc.)
    International,
    /// Local providers (Ollama)
    Local,
    /// Custom provider (user-defined)
    Custom,
}

impl ProviderCategory {
    /// Human-readable label for the category.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Domestic => "国内",
            Self::International => "国际",
            Self::Local => "本地",
            Self::Custom => "自定义",
        }
    }
}

// ── Service Category (UI grouping) ──────────────────────────────────────────

/// Settings tab grouping for provider list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceCategory {
    /// OAuth providers (ChatGPT Plus/Pro via Codex)
    OAuth,
    /// Coding Plan subscription SKUs
    CodingPlan,
    /// Standard API key providers
    Api,
}

impl ServiceCategory {
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::OAuth => "OAUTH",
            Self::CodingPlan => "CODING PLAN",
            Self::Api => "API",
        }
    }

    #[must_use]
    pub fn display_order(&self) -> u8 {
        match self {
            Self::OAuth => 0,
            Self::CodingPlan => 1,
            Self::Api => 2,
        }
    }
}

// ── API Type ────────────────────────────────────────────────────────────────

/// Wire protocol type for provider API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ApiType {
    /// OpenAI-compatible chat completions API (most providers)
    #[serde(rename = "openai-completions")]
    OpenAiCompletions,
    /// Anthropic Messages API (Anthropic, MiniMax, Kimi Coding)
    #[serde(rename = "anthropic-messages")]
    AnthropicMessages,
    /// OpenAI Responses API
    #[serde(rename = "openai-responses")]
    OpenAiResponses,
    /// ChatGPT Codex Responses API (Plus/Pro)
    #[serde(rename = "openai-codex-responses")]
    OpenAiCodexResponses,
    /// In-process local inference via mistralrs (S1: MiniCPM5-1B-GGUF).
    #[serde(rename = "local-mistralrs")]
    LocalMistralRs,
}

// ── Auth Type ───────────────────────────────────────────────────────────────

/// Authentication scheme for provider API access.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    /// Bearer token / API key authentication
    ApiKey,
    /// OAuth flow (e.g., ChatGPT Codex)
    OAuth,
    /// No authentication required (e.g., local Ollama)
    None,
}

// ── Provider Status ─────────────────────────────────────────────────────────

/// Provider availability status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProviderStatus {
    /// Provider is available and ready to use.
    Available,
    /// Provider requires an API key to be configured.
    ApiKeyRequired,
    /// Provider is currently unavailable.
    Unavailable { reason: String },
}

// ── Model Modality ──────────────────────────────────────────────────────────

/// Model input modality.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelModality {
    /// Text-only model.
    Text,
    /// Vision-capable model (text + image).
    Vision,
    /// Multimodal model (text + image + audio + video).
    Multimodal,
}

impl ModelModality {
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Text => "文本",
            Self::Vision => "视觉",
            Self::Multimodal => "多模态",
        }
    }
}

// ── Known Provider (built-in registry entry) ────────────────────────────────

/// Built-in provider definition.
///
/// Each provider has a stable id, display name, default connection settings,
/// and metadata for the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownProvider {
    /// Stable provider identifier, e.g. "openai", "ollama", "anthropic"
    pub id: String,
    /// Display name shown in UI, e.g. "OpenAI", "Ollama (本地)"
    pub display_name: String,
    /// Authentication type required
    pub auth_type: AuthType,
    /// Default API base URL (empty string if not applicable)
    pub default_base_url: String,
    /// Default API protocol
    pub default_api: ApiType,
    /// Settings UI grouping
    pub service_category: ServiceCategory,
    /// Geographical category for onboarding
    pub geo_category: ProviderCategory,
    /// Whether this provider supports model listing
    pub supports_models: bool,
}

impl KnownProvider {
    #[must_use]
    pub fn needs_api_key(&self) -> bool {
        self.auth_type == AuthType::ApiKey
    }

    #[must_use]
    pub fn is_oauth(&self) -> bool {
        self.auth_type == AuthType::OAuth
    }
}

// ── Model ───────────────────────────────────────────────────────────────────

/// Model metadata.
///
/// Represents a single LLM model with its capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// Model identifier, e.g. "claude-sonnet-4-6", "gpt-4o"
    pub id: String,
    /// Display name, e.g. "Claude Sonnet 4.6"
    pub name: String,
    /// Context window size in tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    /// Maximum output tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    /// Input modality
    pub modality: ModelModality,
    /// Whether model supports reasoning/thinking
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reasoning: bool,
    /// Whether assistant tool_call history must carry reasoning_content
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reasoning_required_in_tool_calls: bool,
    /// Whether model accepts top-level reasoning_effort
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub supports_reasoning_effort: bool,
}

// ── Provider Config (saved user configuration) ──────────────────────────────

/// User-provided provider configuration — saved to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider identifier, e.g. "openai", "ollama"
    pub provider_id: String,
    /// Display name shown in UI
    pub display_name: String,
    /// API key (None for local providers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Base URL for the provider's API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// API protocol type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<ApiType>,
}

impl ProviderConfig {
    /// Check if configuration has the minimum required fields.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        match self.provider_id.as_str() {
            "ollama" => self.base_url.as_ref().is_some_and(|u| !u.is_empty()),
            _ => {
                self.base_url.as_ref().is_some_and(|u| !u.is_empty())
                    && self.api_key.as_ref().is_some_and(|k| !k.is_empty())
            }
        }
    }
}

// ── Model Selection ─────────────────────────────────────────────────────────

/// Records the user's active model choice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSelection {
    /// Provider ID that owns this model
    pub provider_id: String,
    /// Model identifier
    pub model_id: String,
}

// ── Model Role Config ───────────────────────────────────────────────────────

/// Per-role model assignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRoleConfig {
    /// Role: "chat", "utility", "summarizer", "compiler"
    pub role: String,
    /// Model reference in "provider_id/model_id" format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_ref: Option<String>,
}

/// Available model role names.
pub const MODEL_ROLES: &[&str] = &["chat", "utility", "utility_large", "summarizer", "compiler"];

/// Human-readable label for a model role.
#[must_use]
pub fn model_role_label(role: &str) -> &'static str {
    match role {
        "chat" => "主对话模型",
        "utility" => "轻工具模型（摘要/翻译）",
        "utility_large" => "重工具模型（复杂推理）",
        "summarizer" => "摘要模型（记忆编译）",
        "compiler" => "编译模型（快速响应）",
        _ => "未知角色",
    }
}

// ── Test Result ──────────────────────────────────────────────────────────────

/// Connection test outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Whether the test succeeded
    pub success: bool,
    /// Human-readable message
    pub message: String,
    /// Round-trip latency in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// Additional details (error codes, suggestions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

// ── All Provider Configs (persisted to disk) ────────────────────────────────

/// Complete provider configuration store — persisted to providers.json.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfigs {
    /// Configured providers with their connection details
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    /// Currently active model selection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_model: Option<ModelSelection>,
    /// All selected models across providers
    #[serde(default)]
    pub selected_models: Vec<ModelSelection>,
    /// Per-role model assignments
    #[serde(default)]
    pub role_models: Vec<ModelRoleConfig>,
    /// Active local-model GGUF quant the in-process engine should load.
    /// `None` ⇒ the engine falls back to [`Quant::default`] (Q4_K_M).
    #[serde(default)]
    pub active_local_quant: Option<Quant>,
}

impl ProviderConfigs {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Upsert a provider config (update if exists, append if new).
    ///
    /// BYOK note: the stored API key is never sent back to the frontend
    /// (`get_provider_config` returns only `has_api_key` + a masked tail), so a
    /// save that omits the key (`api_key = None`) means "leave the key
    /// unchanged" — not "clear it". Preserve the existing key in that case to
    /// avoid silently wiping it when the user edits another field without
    /// retyping the key. Removing a provider entirely goes through
    /// [`Self::remove_provider`].
    pub fn upsert_provider(&mut self, mut config: ProviderConfig) {
        if let Some(existing) = self
            .providers
            .iter_mut()
            .find(|p| p.provider_id == config.provider_id)
        {
            if config.api_key.is_none() {
                config.api_key = existing.api_key.clone();
            }
            *existing = config;
        } else {
            self.providers.push(config);
        }
    }

    /// Remove a provider config by id.
    pub fn remove_provider(&mut self, provider_id: &str) {
        self.providers.retain(|p| p.provider_id != provider_id);
        self.selected_models
            .retain(|m| m.provider_id != provider_id);
        if self
            .active_model
            .as_ref()
            .is_some_and(|m| m.provider_id == provider_id)
        {
            self.active_model = None;
        }
    }

    /// Find a provider config by id.
    #[must_use]
    pub fn find_provider(&self, provider_id: &str) -> Option<&ProviderConfig> {
        self.providers.iter().find(|p| p.provider_id == provider_id)
    }

    /// List all configured provider IDs.
    #[must_use]
    pub fn configured_ids(&self) -> Vec<String> {
        self.providers.iter().map(|p| p.provider_id.clone()).collect()
    }

    /// Get models for a specific provider.
    #[must_use]
    pub fn models_for_provider(&self, provider_id: &str) -> Vec<String> {
        self.selected_models
            .iter()
            .filter(|m| m.provider_id == provider_id)
            .map(|m| m.model_id.clone())
            .collect()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_category_labels() {
        assert_eq!(ProviderCategory::Domestic.label(), "国内");
        assert_eq!(ProviderCategory::International.label(), "国际");
        assert_eq!(ProviderCategory::Local.label(), "本地");
        assert_eq!(ProviderCategory::Custom.label(), "自定义");
    }

    #[test]
    fn test_model_modality_labels() {
        assert_eq!(ModelModality::Text.label(), "文本");
        assert_eq!(ModelModality::Vision.label(), "视觉");
        assert_eq!(ModelModality::Multimodal.label(), "多模态");
    }

    #[test]
    fn test_provider_config_is_complete() {
        let complete = ProviderConfig {
            provider_id: "openai".into(),
            display_name: "OpenAI".into(),
            api_key: Some("sk-test".into()),
            base_url: Some("https://api.openai.com/v1".into()),
            api: Some(ApiType::OpenAiCompletions),
        };
        assert!(complete.is_complete());

        let missing_key = ProviderConfig {
            provider_id: "openai".into(),
            display_name: "OpenAI".into(),
            api_key: None,
            base_url: Some("https://api.openai.com/v1".into()),
            api: Some(ApiType::OpenAiCompletions),
        };
        assert!(!missing_key.is_complete());
    }

    #[test]
    fn test_provider_configs_upsert() {
        let mut configs = ProviderConfigs::new();
        let c1 = ProviderConfig {
            provider_id: "openai".into(),
            display_name: "OpenAI".into(),
            api_key: Some("key1".into()),
            base_url: Some("url1".into()),
            api: None,
        };
        configs.upsert_provider(c1);
        assert_eq!(configs.providers.len(), 1);

        // Update same provider
        let c2 = ProviderConfig {
            provider_id: "openai".into(),
            display_name: "OpenAI Updated".into(),
            api_key: Some("key2".into()),
            base_url: Some("url2".into()),
            api: None,
        };
        configs.upsert_provider(c2);
        assert_eq!(configs.providers.len(), 1);
        assert_eq!(configs.providers[0].display_name, "OpenAI Updated");
    }

    #[test]
    fn test_upsert_preserves_existing_key_when_none() {
        // BYOK: the frontend never receives the stored key, so a save that
        // omits api_key (None) must leave the existing key intact — not wipe it.
        let mut configs = ProviderConfigs::new();
        configs.upsert_provider(ProviderConfig {
            provider_id: "openai".into(),
            display_name: "OpenAI".into(),
            api_key: Some("sk-secret".into()),
            base_url: Some("url1".into()),
            api: None,
        });

        // Re-save without the key (e.g. user edits base_url but doesn't retype key).
        configs.upsert_provider(ProviderConfig {
            provider_id: "openai".into(),
            display_name: "OpenAI".into(),
            api_key: None,
            base_url: Some("url2".into()),
            api: None,
        });

        assert_eq!(configs.providers.len(), 1);
        assert_eq!(
            configs.providers[0].api_key.as_deref(),
            Some("sk-secret"),
            "existing key must be preserved when re-saved with api_key=None"
        );
        assert_eq!(
            configs.providers[0].base_url.as_deref(),
            Some("url2"),
            "other fields still update normally"
        );
    }

    #[test]
    fn test_upsert_replaces_key_when_new_key_provided() {
        // A real new key still replaces the old one.
        let mut configs = ProviderConfigs::new();
        configs.upsert_provider(ProviderConfig {
            provider_id: "openai".into(),
            display_name: "OpenAI".into(),
            api_key: Some("old-key".into()),
            base_url: None,
            api: None,
        });
        configs.upsert_provider(ProviderConfig {
            provider_id: "openai".into(),
            display_name: "OpenAI".into(),
            api_key: Some("new-key".into()),
            base_url: None,
            api: None,
        });
        assert_eq!(configs.providers[0].api_key.as_deref(), Some("new-key"));
    }

    #[test]
    fn test_provider_configs_remove() {
        let mut configs = ProviderConfigs::new();
        configs.upsert_provider(ProviderConfig {
            provider_id: "openai".into(),
            display_name: "OpenAI".into(),
            api_key: Some("key".into()),
            base_url: Some("url".into()),
            api: None,
        });
        configs.selected_models.push(ModelSelection {
            provider_id: "openai".into(),
            model_id: "gpt-4".into(),
        });
        configs.remove_provider("openai");
        assert!(configs.providers.is_empty());
        assert!(configs.selected_models.is_empty());
    }
}
