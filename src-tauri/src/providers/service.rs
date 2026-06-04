//! ProviderService — listing, model discovery, and connection testing.
//!
//! Provides the service layer for the provider settings page:
//! - `list_providers()`: Returns all built-in providers
//! - `list_models()`: Fetches available models from provider API
//!   - Ollama: GET {base_url}/api/tags
//!   - Anthropic: Hardcoded registry
//!   - OpenAI-compatible: GET {base_url}/models
//! - `test_connection()`: Validates API connectivity with latency measurement
//! - `configure_provider()`: Saves provider config to disk
//! - `remove_provider()`: Removes a provider configuration

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use crate::error::Error;

use super::registry;
use super::readiness;
use super::store::{load_provider_configs, save_provider_configs};
use super::types::{
    KnownProvider, Model, ModelModality, ProviderConfig, ProviderConfigs, TestResult,
};

/// Service for provider management operations.
pub struct ProviderService {
    configs: Arc<RwLock<ProviderConfigs>>,
    configs_path: std::path::PathBuf,
}

impl ProviderService {
    /// Create a new ProviderService backed by the given configs path.
    pub fn new(data_dir: &std::path::Path) -> Result<Self, Error> {
        let configs_path = super::store::default_providers_path(data_dir);
        let configs = load_provider_configs(&configs_path)
            .map_err(|e| Error::Internal(format!("Failed to load provider configs: {e}")))?;
        Ok(Self {
            configs: Arc::new(RwLock::new(configs)),
            configs_path,
        })
    }

    // ── Provider listing ────────────────────────────────────────────────────

    /// List all built-in providers.
    #[must_use]
    pub fn list_builtin_providers() -> Vec<KnownProvider> {
        registry::all()
    }

    /// List all configured provider IDs.
    pub async fn list_configured_ids(&self) -> Vec<String> {
        self.configs.read().await.configured_ids()
    }

    /// Get a provider config (built-in info + saved settings).
    pub async fn get_provider_config(&self, provider_id: &str) -> Option<ProviderConfig> {
        self.configs
            .read()
            .await
            .find_provider(provider_id)
            .cloned()
    }

    /// Get all configured models grouped by provider.
    pub async fn get_all_configured_models(&self) -> Vec<(String, Vec<String>)> {
        let configs = self.configs.read().await;
        let mut groups: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for m in &configs.selected_models {
            groups
                .entry(m.provider_id.clone())
                .or_default()
                .push(m.model_id.clone());
        }
        groups.into_iter().collect()
    }

    /// Get configured model IDs for a specific provider.
    pub async fn get_configured_models(&self, provider_id: &str) -> Vec<String> {
        self.configs.read().await.models_for_provider(provider_id)
    }

    /// Get the current active model.
    pub async fn get_active_model(&self) -> Option<super::types::ModelSelection> {
        self.configs.read().await.active_model.clone()
    }

    /// Derive a provider readiness report from the current local config.
    ///
    /// This is a metadata-only helper: it does not make HTTP calls, mutate
    /// credentials, or change runtime provider selection.
    pub async fn provider_readiness(
        &self,
        provider_id: &str,
    ) -> uclaw_provider_core::ProviderReadinessReport {
        let configs = self.configs.read().await;
        let known = registry::find(provider_id);
        readiness::assess_provider_readiness(
            provider_id,
            known.as_ref(),
            configs.find_provider(provider_id),
            &configs.selected_models,
            configs.active_model.as_ref(),
        )
    }

    /// Derive readiness reports for every built-in provider.
    pub async fn all_provider_readiness(
        &self,
    ) -> Vec<uclaw_provider_core::ProviderReadinessReport> {
        let configs = self.configs.read().await;
        registry::all()
            .into_iter()
            .map(|known| {
                readiness::assess_provider_readiness(
                    &known.id,
                    Some(&known),
                    configs.find_provider(&known.id),
                    &configs.selected_models,
                    configs.active_model.as_ref(),
                )
            })
            .collect()
    }

    /// Resolve the active model into full LLM connection parameters.
    /// Returns `(provider_id, model, api_key, base_url, api_override)`.
    /// Used by the chat system to create the LLM provider for sending messages.
    pub async fn get_active_llm_config(
        &self,
    ) -> Option<(String, String, String, String, Option<crate::providers::types::ApiType>)> {
        let configs = self.configs.read().await;
        let active = configs.active_model.as_ref()?;
        let provider = configs.find_provider(&active.provider_id)?;
        Some((
            active.provider_id.clone(),
            active.model_id.clone(),
            provider.api_key.clone().unwrap_or_default(),
            provider.base_url.clone().unwrap_or_default(),
            provider.api.clone(),
        ))
    }

    /// Resolve a specific provider+model into LLM connection parameters.
    /// Returns None if the provider is not configured.
    /// Returns `(provider_id, model, api_key, base_url, api_override)`.
    pub async fn get_provider_llm_config(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Option<(String, String, String, String, Option<crate::providers::types::ApiType>)> {
        let configs = self.configs.read().await;
        let provider = configs.find_provider(provider_id)?;
        Some((
            provider_id.to_string(),
            model_id.to_string(),
            provider.api_key.clone().unwrap_or_default(),
            provider.base_url.clone().unwrap_or_default(),
            provider.api.clone(),
        ))
    }

    /// Resolve the LLM config for a model role with a graceful fallback chain.
    /// Priority: role_models[role] → role_models["chat"] → active_model.
    /// Returns `(provider_id, model, api_key, base_url, api_override)`.
    pub async fn get_role_llm_config(
        &self,
        role: &str,
    ) -> Option<(String, String, String, String, Option<crate::providers::types::ApiType>)> {
        let configs = self.configs.read().await;

        // 1) exact role assignment, then 2) "chat" role fallback.
        for candidate in [role, "chat"] {
            let Some(role_cfg) = configs.role_models.iter().find(|r| r.role == candidate) else {
                continue;
            };
            let Some(model_ref) = &role_cfg.model_ref else {
                continue;
            };
            let parts: Vec<&str> = model_ref.splitn(2, '/').collect();
            if parts.len() != 2 {
                continue;
            }
            let (pid, mid) = (parts[0], parts[1]);
            if let Some(provider) = configs.find_provider(pid) {
                return Some((
                    pid.to_string(),
                    mid.to_string(),
                    provider.api_key.clone().unwrap_or_default(),
                    provider.base_url.clone().unwrap_or_default(),
                    provider.api.clone(),
                ));
            }
        }

        // 3) active_model fallback.
        let active = configs.active_model.as_ref()?;
        let provider = configs.find_provider(&active.provider_id)?;
        Some((
            active.provider_id.clone(),
            active.model_id.clone(),
            provider.api_key.clone().unwrap_or_default(),
            provider.base_url.clone().unwrap_or_default(),
            provider.api.clone(),
        ))
    }

    /// Resolve the chat-role model → active_model fallback chain.
    /// Thin wrapper over [`Self::get_role_llm_config`] with role `"chat"`.
    /// Returns `(provider_id, model, api_key, base_url, api_override)`.
    pub async fn get_chat_llm_config(
        &self,
    ) -> Option<(String, String, String, String, Option<crate::providers::types::ApiType>)> {
        self.get_role_llm_config("chat").await
    }

    /// Resolve the LLM config for a structured one-shot **quality** utility task
    /// (session title / summary). Same tuple shape as [`Self::get_role_llm_config`].
    ///
    /// Quality-first with a local-only fallback: start from the `utility` role,
    /// but if it resolves to the **local in-process model** (LocalMistralRs) —
    /// whose 1B structured-JSON output is fragile (loose titles, emoji always 💬)
    /// — prefer a non-local (cloud) model from the active selection when one is
    /// configured. Falls back to the local utility model when no cloud is
    /// available (offline / local-only setups), so privacy/cost-conscious users
    /// still work without network. Title generation routes through this instead
    /// of `get_role_llm_config("utility")`.
    pub async fn get_utility_quality_llm_config(
        &self,
    ) -> Option<(String, String, String, String, Option<crate::providers::types::ApiType>)> {
        let is_local = |cfg: &Option<(String, String, String, String, Option<crate::providers::types::ApiType>)>| {
            cfg.as_ref().is_some_and(|c| {
                c.0 == "local-minicpm"
                    || c.4 == Some(crate::providers::types::ApiType::LocalMistralRs)
            })
        };
        let utility = self.get_role_llm_config("utility").await;
        if !is_local(&utility) {
            // utility already resolves to a capable cloud model (or its chat/active
            // fallback) — use it as-is.
            return utility;
        }
        // utility is the local 1B model — prefer the active cloud model for quality.
        let active = self.get_active_llm_config().await;
        if active.is_some() && !is_local(&active) {
            return active;
        }
        // No cloud configured (offline / local-only) — keep the local model.
        utility
    }

    /// Resolve the ingestion-role model. Thin wrapper over
    /// [`Self::get_role_llm_config`] with role `"ingestion"`; drops the
    /// `api_override` field for callers that don't need it.
    /// NOTE: unlike the pre-S0 version, ingestion now inherits the `chat`
    /// role assignment before falling back to `active_model` (the generic
    /// resolver's `role → chat → active` chain). This is intentional and
    /// strictly more permissive — it never changes a configured-ingestion
    /// or fully-unconfigured outcome.
    pub async fn get_ingestion_llm_config(&self) -> Option<(String, String, String, String)> {
        self.get_role_llm_config("ingestion")
            .await
            .map(|(pid, mid, key, url, _api)| (pid, mid, key, url))
    }

    // ── Provider configuration ──────────────────────────────────────────────

    /// Save a provider configuration.
    pub async fn configure_provider(&self, config: ProviderConfig) -> Result<(), Error> {
        let mut configs = self.configs.write().await;
        configs.upsert_provider(config);
        save_provider_configs(&configs, &self.configs_path)
            .map_err(|e| Error::Internal(format!("Failed to save provider configs: {e}")))
    }

    /// Configure a provider with multiple model selections.
    /// The first model becomes the default (active_model).
    pub async fn configure_provider_with_models(
        &self,
        provider_config: ProviderConfig,
        model_ids: &[String],
    ) -> Result<(), Error> {
        let mut configs = self.configs.write().await;

        configs.upsert_provider(provider_config.clone());

        // Remove existing models for this provider, then add new ones
        configs
            .selected_models
            .retain(|m| m.provider_id != provider_config.provider_id);

        let mut seen = std::collections::HashSet::new();
        for model_id in model_ids {
            let key = format!("{}::{}", provider_config.provider_id, model_id);
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);
            configs.selected_models.push(super::types::ModelSelection {
                provider_id: provider_config.provider_id.clone(),
                model_id: model_id.clone(),
            });
        }

        // First model becomes the default
        if let Some(first) = model_ids.first() {
            configs.active_model = Some(super::types::ModelSelection {
                provider_id: provider_config.provider_id.clone(),
                model_id: first.clone(),
            });
        }

        save_provider_configs(&configs, &self.configs_path)
            .map_err(|e| Error::Internal(format!("Failed to save provider configs: {e}")))
    }

    /// Remove a provider configuration.
    pub async fn remove_provider(&self, provider_id: &str) -> Result<(), Error> {
        let mut configs = self.configs.write().await;
        configs.remove_provider(provider_id);
        save_provider_configs(&configs, &self.configs_path)
            .map_err(|e| Error::Internal(format!("Failed to save after removal: {e}")))
    }

    /// Get all per-role model assignments.
    pub async fn get_role_models(&self) -> Vec<super::types::ModelRoleConfig> {
        self.configs.read().await.role_models.clone()
    }

    /// Set (or clear) the model assigned to a specific role.
    pub async fn set_role_model(&self, role: &str, model_ref: Option<String>) -> Result<(), Error> {
        let mut configs = self.configs.write().await;
        if let Some(entry) = configs.role_models.iter_mut().find(|r| r.role == role) {
            entry.model_ref = model_ref;
        } else {
            configs.role_models.push(super::types::ModelRoleConfig {
                role: role.to_string(),
                model_ref,
            });
        }
        save_provider_configs(&configs, &self.configs_path)
            .map_err(|e| Error::Internal(format!("Failed to save role model: {e}")))
    }

    /// Select the active model.
    pub async fn select_model(&self, provider_id: &str, model_id: &str) -> Result<(), Error> {
        let mut configs = self.configs.write().await;
        configs.active_model = Some(super::types::ModelSelection {
            provider_id: provider_id.to_string(),
            model_id: model_id.to_string(),
        });
        save_provider_configs(&configs, &self.configs_path)
            .map_err(|e| Error::Internal(format!("Failed to save model selection: {e}")))
    }

    /// Get the active local-model GGUF quant (None ⇒ engine uses the default).
    pub async fn get_active_local_quant(
        &self,
    ) -> Option<crate::local_llm::download::quant::Quant> {
        self.configs.read().await.active_local_quant
    }

    /// Persist the active local-model GGUF quant so the UI + engine agree.
    pub async fn set_active_local_quant(
        &self,
        quant: crate::local_llm::download::quant::Quant,
    ) -> Result<(), Error> {
        let mut configs = self.configs.write().await;
        configs.active_local_quant = Some(quant);
        save_provider_configs(&configs, &self.configs_path)
            .map_err(|e| Error::Internal(format!("Failed to save active local quant: {e}")))
    }

    // ── Model listing ───────────────────────────────────────────────────────

    /// List available models for a given provider.
    ///
    /// Uses three different protocols:
    /// - **Ollama**: GET {base_url}/api/tags
    /// - **Anthropic**: Returns known Claude models from built-in registry
    /// - **OpenAI-compatible**: GET {base_url}/models
    pub async fn list_models(
        &self,
        provider_id: &str,
        base_url: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<Model>, String> {
        match provider_id {
            "local-minicpm" => Ok(list_local_minicpm_models()),
            "ollama" => list_ollama_models(base_url).await,
            "anthropic" => Ok(list_anthropic_models()),
            _ => list_openai_compat_models(base_url, api_key).await,
        }
    }

    // ── Connection testing ──────────────────────────────────────────────────

    /// Test connection to a provider.
    pub async fn test_connection(
        &self,
        provider_id: &str,
        base_url: &str,
        api_key: Option<&str>,
    ) -> TestResult {
        let start = Instant::now();
        match test_provider_endpoint(provider_id, base_url, api_key).await {
            Ok(message) => TestResult {
                success: true,
                message,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                details: None,
            },
            Err(error) => TestResult {
                success: false,
                message: error,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                details: None,
            },
        }
    }
}

// ── Model listing implementations ───────────────────────────────────────────

/// Fetch models from a local Ollama instance via `/api/tags`.
async fn list_ollama_models(base_url: &str) -> Result<Vec<Model>, String> {
    // Ollama's /api/tags is on the native API root, not under /v1
    let base = base_url.trim_end_matches('/').trim_end_matches("/v1");
    let url = format!("{base}/api/tags");

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to connect to Ollama: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Ollama returned {}", response.status()));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Ollama response: {e}"))?;

    let models = body
        .get("models")
        .and_then(|m| m.as_array())
        .ok_or_else(|| "Ollama response missing 'models' field".to_string())?;

    Ok(models
        .iter()
        .filter_map(|m| {
            let name = m.get("name").and_then(|v| v.as_str())?;
            Some(Model {
                id: name.to_string(),
                name: name.to_string(),
                context_window: None,
                max_tokens: None,
                modality: ModelModality::Text,
                reasoning: false,
                reasoning_required_in_tool_calls: false,
                supports_reasoning_effort: false,
            })
        })
        .collect())
}

/// Return the static MiniCPM model list (no network call).
fn list_local_minicpm_models() -> Vec<Model> {
    vec![Model {
        id: "minicpm5-1b".to_string(),
        name: "MiniCPM5 1B (本地)".to_string(),
        context_window: Some(32_768),
        max_tokens: Some(4_096),
        modality: ModelModality::Text,
        reasoning: false,
        reasoning_required_in_tool_calls: false,
        supports_reasoning_effort: false,
    }]
}

/// Return known Anthropic/Claude models from built-in registry.
fn list_anthropic_models() -> Vec<Model> {
    [
        (
            "claude-opus-4-6",
            "Claude Opus 4.6",
            200_000u64,
            32_000u64,
        ),
        (
            "claude-sonnet-4-6",
            "Claude Sonnet 4.6",
            200_000,
            64_000,
        ),
        (
            "claude-sonnet-4-5-20250514",
            "Claude Sonnet 4.5",
            200_000,
            64_000,
        ),
        (
            "claude-haiku-4-5-20251213",
            "Claude Haiku 4.5",
            200_000,
            8_000,
        ),
    ]
    .into_iter()
    .map(|(id, name, ctx, max)| Model {
        id: id.to_string(),
        name: name.to_string(),
        context_window: Some(ctx),
        max_tokens: Some(max),
        modality: ModelModality::Text,
        reasoning: false,
        reasoning_required_in_tool_calls: false,
        supports_reasoning_effort: false,
    })
    .collect()
}

/// Fetch models from an OpenAI-compatible provider via `/models`.
async fn list_openai_compat_models(
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<Model>, String> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let mut request = client.get(&url);

    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to connect to provider: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Provider returned {}", response.status()));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse provider response: {e}"))?;

    let models = body
        .get("data")
        .and_then(|m| m.as_array())
        .ok_or_else(|| "Provider response missing 'data' field".to_string())?;

    Ok(models
        .iter()
        .filter_map(|m| {
            let id = m.get("id").and_then(|v| v.as_str())?;
            Some(Model {
                id: id.to_string(),
                name: id.to_string(),
                context_window: None,
                max_tokens: None,
                modality: ModelModality::Text,
                reasoning: false,
                reasoning_required_in_tool_calls: false,
                supports_reasoning_effort: false,
            })
        })
        .collect())
}

// ── Connection testing ──────────────────────────────────────────────────────

/// Test connectivity to a provider endpoint.
async fn test_provider_endpoint(
    provider_id: &str,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<String, String> {
    match provider_id {
        "ollama" => {
            let url = format!("{}/api/tags", base_url.trim_end_matches('/').trim_end_matches("/v1"));
            let response = reqwest::get(&url)
                .await
                .map_err(|e| format!("Failed to reach Ollama: {e}"))?;
            if response.status().is_success() {
                Ok("Ollama connection successful".to_string())
            } else {
                Err(format!("Ollama returned HTTP {}", response.status()))
            }
        }
        _ => {
            let url = format!("{}/models", base_url.trim_end_matches('/'));
            let client = reqwest::Client::new();
            let mut request = client.get(&url);
            if let Some(key) = api_key {
                request = request.bearer_auth(key);
            }
            let response = request
                .send()
                .await
                .map_err(|e| format!("Connection failed: {e}"))?;
            let status = response.status();
            if status.is_success() {
                Ok(format!("Connection successful (HTTP {})", status.as_u16()))
            } else if status.as_u16() == 401 || status.as_u16() == 403 {
                Err("Authentication failed — check your API key".to_string())
            } else if status.as_u16() == 404 {
                Ok(format!(
                    "Endpoint exists but returned 404 (HTTP {})",
                    status.as_u16()
                ))
            } else {
                Err(format!("Server returned HTTP {}", status.as_u16()))
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_builtin_providers_returns_all() {
        let providers = ProviderService::list_builtin_providers();
        assert!(!providers.is_empty());
        assert!(providers.iter().any(|p| p.id == "openai"));
        assert!(providers.iter().any(|p| p.id == "ollama"));
    }

    #[test]
    fn test_list_anthropic_models_returns_models() {
        let models = list_anthropic_models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id.contains("sonnet")));
        assert!(models.iter().any(|m| m.id.contains("opus")));
    }

    #[test]
    fn test_anthropic_models_have_context_windows() {
        let models = list_anthropic_models();
        for model in &models {
            assert!(model.context_window.is_some(), "{} missing context window", model.id);
            assert!(model.max_tokens.is_some(), "{} missing max tokens", model.id);
        }
    }

    // ProviderConfig + ProviderConfigs are already in scope via `use super::*`.
    use super::super::types::{ApiType, ModelRoleConfig, ModelSelection};

    /// Build a ProviderService directly from in-memory configs (no disk I/O).
    fn svc(configs: ProviderConfigs) -> ProviderService {
        ProviderService {
            configs: std::sync::Arc::new(tokio::sync::RwLock::new(configs)),
            configs_path: std::path::PathBuf::from("/tmp/uclaw-test-providers.json"),
        }
    }

    fn provider(id: &str) -> ProviderConfig {
        ProviderConfig {
            provider_id: id.to_string(),
            display_name: id.to_string(),
            api_key: Some(format!("key-{id}")),
            base_url: Some(format!("https://{id}.example/v1")),
            api: Some(ApiType::OpenAiCompletions),
        }
    }

    #[tokio::test]
    async fn active_local_quant_round_trips() {
        use crate::local_llm::download::quant::Quant;
        // Unique temp path so the disk write doesn't collide with `svc`'s shared
        // fixed path / other tests.
        let path = std::env::temp_dir().join(format!(
            "uclaw-test-quant-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let s = ProviderService {
            configs: std::sync::Arc::new(tokio::sync::RwLock::new(ProviderConfigs::new())),
            configs_path: path.clone(),
        };

        // Unset by default.
        assert_eq!(s.get_active_local_quant().await, None);

        // Set + read back.
        s.set_active_local_quant(Quant::Q8_0).await.unwrap();
        assert_eq!(s.get_active_local_quant().await, Some(Quant::Q8_0));

        // Overwrite.
        s.set_active_local_quant(Quant::F16).await.unwrap();
        assert_eq!(s.get_active_local_quant().await, Some(Quant::F16));

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn role_config_uses_exact_role_assignment() {
        let configs = ProviderConfigs {
            providers: vec![provider("local"), provider("deepseek")],
            active_model: Some(ModelSelection {
                provider_id: "deepseek".into(),
                model_id: "deepseek-v4".into(),
            }),
            selected_models: vec![],
            role_models: vec![
                ModelRoleConfig { role: "chat".into(), model_ref: Some("deepseek/deepseek-v4".into()) },
                ModelRoleConfig { role: "summarizer".into(), model_ref: Some("local/minicpm5-1b".into()) },
            ],
            active_local_quant: None,
        };
        let s = svc(configs);
        let (pid, mid, _key, _url, _api) = s.get_role_llm_config("summarizer").await.unwrap();
        assert_eq!(pid, "local");
        assert_eq!(mid, "minicpm5-1b");
    }

    #[tokio::test]
    async fn role_config_falls_back_to_chat_when_role_unset() {
        let configs = ProviderConfigs {
            providers: vec![provider("deepseek")],
            active_model: None,
            selected_models: vec![],
            role_models: vec![ModelRoleConfig {
                role: "chat".into(),
                model_ref: Some("deepseek/deepseek-v4".into()),
            }],
            active_local_quant: None,
        };
        let s = svc(configs);
        let (pid, mid, _, _, _) = s.get_role_llm_config("summarizer").await.unwrap();
        assert_eq!(pid, "deepseek");
        assert_eq!(mid, "deepseek-v4");
    }

    #[tokio::test]
    async fn role_config_falls_back_to_active_when_chat_unset() {
        let configs = ProviderConfigs {
            providers: vec![provider("deepseek")],
            active_model: Some(ModelSelection {
                provider_id: "deepseek".into(),
                model_id: "deepseek-v4".into(),
            }),
            selected_models: vec![],
            role_models: vec![],
            active_local_quant: None,
        };
        let s = svc(configs);
        let (pid, mid, _, _, _) = s.get_role_llm_config("summarizer").await.unwrap();
        assert_eq!(pid, "deepseek");
        assert_eq!(mid, "deepseek-v4");
    }

    /// A local in-process MiniCPM provider (id + LocalMistralRs api), matching
    /// the production registry entry that `get_utility_quality_llm_config`
    /// detects as "local".
    fn local_provider() -> ProviderConfig {
        let mut p = provider("local-minicpm");
        p.api = Some(ApiType::LocalMistralRs);
        p
    }

    #[tokio::test]
    async fn utility_quality_prefers_cloud_when_utility_is_local() {
        // utility = local 1B, active = cloud → title routes to the cloud model.
        let configs = ProviderConfigs {
            providers: vec![local_provider(), provider("deepseek")],
            active_model: Some(ModelSelection {
                provider_id: "deepseek".into(),
                model_id: "deepseek-v4".into(),
            }),
            selected_models: vec![],
            role_models: vec![ModelRoleConfig {
                role: "utility".into(),
                model_ref: Some("local-minicpm/minicpm5-1b".into()),
            }],
            active_local_quant: None,
        };
        let s = svc(configs);
        let (pid, mid, _, _, _) = s.get_utility_quality_llm_config().await.unwrap();
        assert_eq!(pid, "deepseek", "local utility should prefer the cloud active model");
        assert_eq!(mid, "deepseek-v4");
    }

    #[tokio::test]
    async fn utility_quality_keeps_local_when_no_cloud_available() {
        // utility = local AND active = local (offline / local-only) → keep local.
        let configs = ProviderConfigs {
            providers: vec![local_provider()],
            active_model: Some(ModelSelection {
                provider_id: "local-minicpm".into(),
                model_id: "minicpm5-1b".into(),
            }),
            selected_models: vec![],
            role_models: vec![ModelRoleConfig {
                role: "utility".into(),
                model_ref: Some("local-minicpm/minicpm5-1b".into()),
            }],
            active_local_quant: None,
        };
        let s = svc(configs);
        let (pid, _, _, _, _) = s.get_utility_quality_llm_config().await.unwrap();
        assert_eq!(pid, "local-minicpm", "no cloud → keep the local utility model");
    }

    #[tokio::test]
    async fn utility_quality_uses_utility_as_is_when_already_cloud() {
        // utility already cloud → use it verbatim, don't swap to the active model.
        let configs = ProviderConfigs {
            providers: vec![provider("deepseek"), provider("moonshot")],
            active_model: Some(ModelSelection {
                provider_id: "moonshot".into(),
                model_id: "k2".into(),
            }),
            selected_models: vec![],
            role_models: vec![ModelRoleConfig {
                role: "utility".into(),
                model_ref: Some("deepseek/deepseek-v4".into()),
            }],
            active_local_quant: None,
        };
        let s = svc(configs);
        let (pid, mid, _, _, _) = s.get_utility_quality_llm_config().await.unwrap();
        assert_eq!(pid, "deepseek", "cloud utility used as-is");
        assert_eq!(mid, "deepseek-v4");
    }

    #[tokio::test]
    async fn role_config_none_when_nothing_configured() {
        let s = svc(ProviderConfigs::default());
        assert!(s.get_role_llm_config("summarizer").await.is_none());
    }

    #[tokio::test]
    async fn role_config_falls_through_unresolvable_assignment_to_chat() {
        // The role is assigned, but neither candidate resolves cleanly:
        // a dead provider reference AND a malformed (slash-less) ref must
        // both fall THROUGH to the valid chat assignment — not return None.
        let configs = ProviderConfigs {
            providers: vec![provider("deepseek")],
            active_model: None,
            selected_models: vec![],
            role_models: vec![
                // points at a provider that isn't configured → find_provider miss
                ModelRoleConfig { role: "summarizer".into(), model_ref: Some("ghost/missing".into()) },
                // malformed: no '/' separator → split miss
                ModelRoleConfig { role: "utility".into(), model_ref: Some("no-slash".into()) },
                ModelRoleConfig { role: "chat".into(), model_ref: Some("deepseek/deepseek-v4".into()) },
            ],
            active_local_quant: None,
        };
        let s = svc(configs);
        let (pid, mid, _, _, _) = s.get_role_llm_config("summarizer").await.unwrap();
        assert_eq!(pid, "deepseek", "dead-provider role must fall through to chat");
        assert_eq!(mid, "deepseek-v4");
        let (pid2, _, _, _, _) = s.get_role_llm_config("utility").await.unwrap();
        assert_eq!(pid2, "deepseek", "malformed model_ref must fall through to chat");
    }
}
