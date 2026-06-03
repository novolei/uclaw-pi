//! Shared LLM adapter for Memory OS scenarios — Phase 6a foundation
//! commit before the real `WikiSynthesizer` (Phase 6b) and real
//! `LintAnalyzer` (Phase 6c) get wired up.
//!
//! ## Why this exists
//!
//! Phase 3 (`wiki_synth.rs`) and Phase 5 (`memory_lint.rs`) each defined
//! an LLM-shaped seam (`WikiSynthesizer`, `LintAnalyzer`) but only
//! shipped deterministic `Stub*` implementations because plumbing a
//! real provider/credential/cost path is its own chunk of work and
//! doesn't bisect cleanly into either phase's scope.
//!
//! Phase 6 needs real LLM calls in three places:
//!
//! 1. Wiki overview synthesis (Phase 6b — `RealWikiSynthesizer`)
//! 2. Lint check analyzer    (Phase 6c — `RealLintAnalyzer`)
//! 3. EntityPage compiled_truth synthesis (Phase 6.2)
//!
//! All three want the same shape: take a system prompt + user prompt,
//! run a completion against the user's configured active model, capture
//! tokens spent under a cost tag, return the text. So we factor that
//! into one [`MemoryOsLlm`] trait + one [`MemoryOsLlmClient`] impl.
//!
//! ## Cost tagging
//!
//! Each call passes a `cost_tag` like `"memory_wiki"` /
//! `"memory_lint"` / `"memory_entity_synth"`. The recorded
//! `cost_records.model` row is `format!("{tag}:{actual_model}")`, e.g.
//! `"memory_wiki:claude-sonnet-4-20250514"`. This preserves the
//! `LIKE 'memory_lint%'` cost-guard pattern Phase 5 already relies on
//! (`memory_lint_run_now` sums today's spend before each LLM call).
//!
//! ## Mock for tests
//!
//! `#[cfg(test)] MockMemoryOsLlm` returns a canned text + canned token
//! counts. Phase 6b/6c/6.2 unit tests build the real synth/analyzer
//! around this mock so the test suite never needs a live API key.

use async_trait::async_trait;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

use crate::agent::types::{calculate_cost, ChatMessage, RespondOutput};
use crate::llm::{create_provider, llm_config_from_provider, CompletionConfig};
use crate::providers::service::ProviderService;

// ─── Trait + DTOs ──────────────────────────────────────────────────────

/// Pluggable text-completion façade for Memory OS LLM scenarios.
///
/// Real impl in this module ([`MemoryOsLlmClient`]) routes through
/// `ProviderService`. Tests use [`MockMemoryOsLlm`] (gated on `cfg(test)`).
#[async_trait]
pub trait MemoryOsLlm: Send + Sync {
    /// Run one chat-style completion (single system msg + single user msg).
    ///
    /// `cost_tag` is the per-feature prefix written into
    /// `cost_records.model`; pass `"memory_wiki"` / `"memory_lint"` /
    /// `"memory_entity_synth"`. The cost row's `model` column ends up
    /// as `"{cost_tag}:{actual_model_id}"` so daily-spend queries
    /// (`WHERE model LIKE 'memory_lint%'`) keep working.
    async fn complete_text(
        &self,
        cost_tag: &str,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: u32,
    ) -> Result<MemoryOsLlmOutput, MemoryOsLlmError>;

    /// Short descriptor for telemetry / UI badges. Real client returns
    /// the resolved provider+model (after the first call) or
    /// `"memory_os_llm:unconfigured"` before; tests return
    /// `"mock:memory_os_llm"`.
    fn descriptor(&self) -> String;
}

#[derive(Debug, Clone)]
pub struct MemoryOsLlmOutput {
    pub text: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Resolved upstream model id (e.g. `claude-sonnet-4-20250514`),
    /// NOT the cost-tagged combined string. Useful for badges that
    /// want to display the underlying model separately.
    pub model: String,
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryOsLlmError {
    #[error("no active LLM provider configured — set one in Settings before enabling Memory OS LLM features")]
    NoProvider,
    #[error("LLM returned an empty text completion")]
    EmptyText,
    #[error("LLM call failed: {0}")]
    Llm(String),
}

// ─── Real client ───────────────────────────────────────────────────────

/// Production implementation. Holds an `Arc<ProviderService>` and an
/// `Arc<Mutex<Connection>>` for cost persistence. Both are cheap clones
/// of `AppState` fields, so the client itself is `Arc<Self>`-shareable.
pub struct MemoryOsLlmClient {
    provider_service: Arc<ProviderService>,
    db: Arc<Mutex<Connection>>,
}

impl MemoryOsLlmClient {
    pub fn new(provider_service: Arc<ProviderService>, db: Arc<Mutex<Connection>>) -> Self {
        Self {
            provider_service,
            db,
        }
    }

    /// Best-effort cost record insert. Logs and swallows failures so a
    /// flaky DB write never breaks the caller's LLM pipeline.
    fn record_cost(&self, cost_tag: &str, model: &str, input_tokens: u32, output_tokens: u32) {
        let combined_model = format!("{}:{}", cost_tag, model);
        let cost = calculate_cost(model, input_tokens, output_tokens);
        let now = chrono::Utc::now().timestamp_millis();
        let id = uuid::Uuid::new_v4().to_string();
        let conn = match self.db.lock() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("memory_os_llm: DB lock failed: {}", e);
                return;
            }
        };
        // V13 cost_records.session_id is NOT NULL — use a stable
        // sentinel for non-agent-session writes so daily-spend rollups
        // can still attribute them.
        if let Err(e) = conn.execute(
            "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)
             VALUES (?1, 'memory_os', ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                combined_model,
                input_tokens as i64,
                output_tokens as i64,
                cost,
                now
            ],
        ) {
            tracing::warn!("memory_os_llm: INSERT cost record failed: {}", e);
        }
    }
}

/// Map a Memory-OS `cost_tag` to the model role its completion should use.
/// Unmapped tags fall back to `"chat"` (fail-safe, never fail-dead).
/// Roles resolve through [`ProviderService::get_role_llm_config`]'s
/// `role → chat → active` fallback chain.
fn role_for_cost_tag(tag: &str) -> &'static str {
    match tag {
        "memory_consolidation" | "memory_wiki" => "summarizer",
        "memory_reflection" | "memory_daydream" => "compiler",
        "memory_promotion" | "memory_entity_synth" | "memory_lint" => "utility",
        _ => "chat",
    }
}

/// Whether a completion's token spend should be booked to `cost_records`.
/// Local in-process inference (`ApiType::LocalMistralRs`) is free, so it is
/// never costed; every hosted provider (api `None` / OpenAI / Anthropic / …)
/// records as before. (Guarding here rather than in the global
/// `calculate_cost`, which intentionally keeps a non-zero default for unknown
/// hosted model ids.)
fn should_record_cost(api: &Option<crate::providers::types::ApiType>) -> bool {
    !matches!(api, Some(crate::providers::types::ApiType::LocalMistralRs))
}

#[async_trait]
impl MemoryOsLlm for MemoryOsLlmClient {
    async fn complete_text(
        &self,
        cost_tag: &str,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: u32,
    ) -> Result<MemoryOsLlmOutput, MemoryOsLlmError> {
        let role = role_for_cost_tag(cost_tag);
        let (provider_id, model, api_key, base_url, api) = self
            .provider_service
            .get_role_llm_config(role)
            .await
            .ok_or(MemoryOsLlmError::NoProvider)?;

        // Per-scenario routing observability (S0): proves which role a given
        // Memory-OS pass resolved to, and which model it landed on.
        tracing::info!(
            cost_tag,
            role,
            resolved_model = %model,
            "memory_os_llm: routed completion to role"
        );

        let cfg = llm_config_from_provider(
            &provider_id,
            &model,
            &api_key,
            &base_url,
            max_tokens,
            0.3, // memory-os synthesis prefers determinism over flair
            api.clone(), // effective api — carries ApiType::LocalMistralRs so a role
                         // assigned to local-minicpm reaches the local provider (S1)
        );
        let provider =
            create_provider(&cfg).map_err(|e| MemoryOsLlmError::Llm(e.to_string()))?;

        let messages = vec![
            ChatMessage::system(system_prompt),
            ChatMessage::user(user_prompt),
        ];
        let completion = CompletionConfig {
            model: model.clone(),
            max_tokens,
            temperature: 0.3,
            thinking_enabled: false,
        };

        let resp = provider
            .complete(messages, vec![], &completion)
            .await
            .map_err(|e| MemoryOsLlmError::Llm(e.to_string()))?;

        let (text, usage) = match resp {
            RespondOutput::Text { text, metadata, .. } => {
                (text, metadata.usage.unwrap_or_default())
            }
            RespondOutput::ToolCalls {
                text, metadata, ..
            } => (text.unwrap_or_default(), metadata.usage.unwrap_or_default()),
        };

        if text.trim().is_empty() {
            return Err(MemoryOsLlmError::EmptyText);
        }

        if should_record_cost(&api) {
            self.record_cost(cost_tag, &model, usage.input_tokens, usage.output_tokens);
        }

        Ok(MemoryOsLlmOutput {
            text,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            model,
        })
    }

    fn descriptor(&self) -> String {
        // Can't `.await` here because trait is sync — return the
        // generic descriptor; consumers that want the resolved model
        // can read it off the most-recent `MemoryOsLlmOutput.model`.
        "memory_os_llm:provider_service".to_string()
    }
}

// ─── Test mock ─────────────────────────────────────────────────────────

#[cfg(test)]
pub struct MockMemoryOsLlm {
    pub canned_text: String,
    pub canned_input_tokens: u32,
    pub canned_output_tokens: u32,
    pub canned_model: String,
}

#[cfg(test)]
impl Default for MockMemoryOsLlm {
    fn default() -> Self {
        Self {
            canned_text: "[mock memory_os_llm output]".to_string(),
            canned_input_tokens: 100,
            canned_output_tokens: 50,
            canned_model: "mock:test-model".to_string(),
        }
    }
}

#[cfg(test)]
#[async_trait]
impl MemoryOsLlm for MockMemoryOsLlm {
    async fn complete_text(
        &self,
        _cost_tag: &str,
        _system_prompt: &str,
        _user_prompt: &str,
        _max_tokens: u32,
    ) -> Result<MemoryOsLlmOutput, MemoryOsLlmError> {
        Ok(MemoryOsLlmOutput {
            text: self.canned_text.clone(),
            input_tokens: self.canned_input_tokens,
            output_tokens: self.canned_output_tokens,
            model: self.canned_model.clone(),
        })
    }

    fn descriptor(&self) -> String {
        "mock:memory_os_llm".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_returns_canned_output() {
        let mock = MockMemoryOsLlm::default();
        let out = mock
            .complete_text("memory_wiki", "sys", "usr", 1024)
            .await
            .unwrap();
        assert!(out.text.contains("[mock"));
        assert_eq!(out.input_tokens, 100);
        assert_eq!(out.output_tokens, 50);
        assert_eq!(out.model, "mock:test-model");
    }

    #[tokio::test]
    async fn mock_descriptor_marks_it_as_mock() {
        let mock = MockMemoryOsLlm::default();
        assert_eq!(mock.descriptor(), "mock:memory_os_llm");
    }

    #[tokio::test]
    async fn mock_with_custom_text() {
        let mock = MockMemoryOsLlm {
            canned_text: "Alice works at Acme.".to_string(),
            canned_input_tokens: 250,
            canned_output_tokens: 120,
            canned_model: "mock:sonnet".to_string(),
        };
        let out = mock
            .complete_text("memory_entity_synth", "system", "user", 2000)
            .await
            .unwrap();
        assert_eq!(out.text, "Alice works at Acme.");
        assert_eq!(out.input_tokens, 250);
        assert_eq!(out.output_tokens, 120);
        assert_eq!(out.model, "mock:sonnet");
    }

    /// Smoke test the trait-object pattern Phase 6b/6c/6.2 will use
    /// (`Arc<dyn MemoryOsLlm>` field on a scenario struct).
    #[tokio::test]
    async fn trait_object_dispatches_correctly() {
        let llm: Arc<dyn MemoryOsLlm> = Arc::new(MockMemoryOsLlm::default());
        let out = llm.complete_text("memory_lint", "sys", "usr", 512).await.unwrap();
        assert!(!out.text.is_empty());
        assert_eq!(llm.descriptor(), "mock:memory_os_llm");
    }

    #[test]
    fn cost_tags_map_to_expected_roles() {
        assert_eq!(role_for_cost_tag("memory_consolidation"), "summarizer");
        assert_eq!(role_for_cost_tag("memory_wiki"), "summarizer");
        assert_eq!(role_for_cost_tag("memory_reflection"), "compiler");
        assert_eq!(role_for_cost_tag("memory_daydream"), "compiler");
        assert_eq!(role_for_cost_tag("memory_promotion"), "utility");
        assert_eq!(role_for_cost_tag("memory_entity_synth"), "utility");
        assert_eq!(role_for_cost_tag("memory_lint"), "utility");
        // Unknown tags default to the safe "chat" role.
        assert_eq!(role_for_cost_tag("something_new"), "chat");
    }

    #[test]
    fn effective_api_is_carried_into_llm_config() {
        // The config built for a resolved local-minicpm role must keep the
        // LocalMistralRs api so create_provider routes to the local provider
        // (rather than falling back to the OpenAI wire protocol).
        let cfg = crate::llm::llm_config_from_provider(
            "local-minicpm", "minicpm5-1b", "", "", 256, 0.3,
            Some(crate::providers::types::ApiType::LocalMistralRs),
        );
        assert_eq!(cfg.api, Some(crate::providers::types::ApiType::LocalMistralRs));
        assert_eq!(cfg.provider, "local-minicpm");
    }

    #[test]
    fn local_inference_skips_cost_recording() {
        use crate::providers::types::ApiType;
        // Local in-process inference is free → never recorded.
        assert!(!should_record_cost(&Some(ApiType::LocalMistralRs)));
        // Hosted providers (and the unset/default case) always record.
        assert!(should_record_cost(&None));
        assert!(should_record_cost(&Some(ApiType::OpenAiCompletions)));
        assert!(should_record_cost(&Some(ApiType::AnthropicMessages)));
    }
}
