//! Built-in provider registry.
//!
//! Contains 25+ pre-configured provider entries with default connection
//! settings. User configuration overrides these defaults.
//!
//! Providers are grouped into 4 service categories for the UI:
//! - OAuth (1): ChatGPT Plus/Pro via Codex
//! - Coding Plan (3): Kimi, DashScope, VolcEngine coding plans
//! - API (21+): Standard API key providers (domestic + international)

use super::types::{ApiType, AuthType, KnownProvider, ProviderCategory, ServiceCategory};

/// All built-in providers, grouped by service category then alphabetically.
///
/// IDs are stable wire strings — they appear in user config and IPC.
/// **Do not rename** existing entries; add new ones at the end.
pub fn builtin_providers() -> Vec<KnownProvider> {
    vec![
    // ── OAuth ──────────────────────────────────────────────────────
    KnownProvider {
        id: "openai-codex-oauth".into(),
        display_name: "ChatGPT Plus/Pro (Codex)".into(),
        auth_type: AuthType::OAuth,
        default_base_url: "".into(),
        default_api: ApiType::OpenAiCodexResponses,
        service_category: ServiceCategory::OAuth,
        geo_category: ProviderCategory::International,
        supports_models: false,
    },
    // ── Coding Plan SKUs ─────────────────────────────────────────
    KnownProvider {
        id: "kimi-coding".into(),
        display_name: "Kimi Coding Plan".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.kimi.com/coding/".into(),
        default_api: ApiType::AnthropicMessages,
        service_category: ServiceCategory::CodingPlan,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "dashscope-coding".into(),
        display_name: "百炼 Coding Plan".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://coding.dashscope.aliyuncs.com/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::CodingPlan,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "volcengine-coding".into(),
        display_name: "火山引擎 Coding Plan".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://ark.cn-beijing.volces.com/api/coding/v3".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::CodingPlan,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    // ── API (International) ───────────────────────────────────────
    KnownProvider {
        id: "openai".into(),
        display_name: "OpenAI".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.openai.com/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::International,
        supports_models: true,
    },
    KnownProvider {
        id: "anthropic".into(),
        display_name: "Anthropic".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.anthropic.com".into(),
        default_api: ApiType::AnthropicMessages,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::International,
        supports_models: true,
    },
    KnownProvider {
        id: "gemini".into(),
        display_name: "Google Gemini".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::International,
        supports_models: true,
    },
    KnownProvider {
        id: "openrouter".into(),
        display_name: "OpenRouter".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://openrouter.ai/api/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::International,
        supports_models: true,
    },
    KnownProvider {
        id: "groq".into(),
        display_name: "Groq".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.groq.com/openai/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::International,
        supports_models: true,
    },
    KnownProvider {
        id: "together".into(),
        display_name: "Together AI".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.together.xyz/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::International,
        supports_models: true,
    },
    KnownProvider {
        id: "fireworks".into(),
        display_name: "Fireworks AI".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.fireworks.ai/inference/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::International,
        supports_models: true,
    },
    KnownProvider {
        id: "mistral".into(),
        display_name: "Mistral AI".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.mistral.ai/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::International,
        supports_models: true,
    },
    KnownProvider {
        id: "perplexity".into(),
        display_name: "Perplexity".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.perplexity.ai".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::International,
        supports_models: true,
    },
    KnownProvider {
        id: "xai".into(),
        display_name: "xAI (Grok)".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.x.ai/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::International,
        supports_models: true,
    },
    // ── API (Domestic) ────────────────────────────────────────────
    KnownProvider {
        id: "deepseek".into(),
        display_name: "DeepSeek".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.deepseek.com/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "moonshot".into(),
        display_name: "Moonshot (Kimi)".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.moonshot.cn/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "dashscope".into(),
        display_name: "阿里云百炼 (DashScope)".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "siliconflow".into(),
        display_name: "SiliconFlow (硅基流动)".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.siliconflow.cn/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "zhipu".into(),
        display_name: "智谱 AI (GLM)".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://open.bigmodel.cn/api/paas/v4".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "minimax".into(),
        display_name: "MiniMax (海螺)".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.minimaxi.com/anthropic".into(),
        default_api: ApiType::AnthropicMessages,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "baichuan".into(),
        display_name: "百川智能".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.baichuan-ai.com/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "stepfun".into(),
        display_name: "阶跃星辰 (StepFun)".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.stepfun.com/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "volcengine".into(),
        display_name: "火山引擎 (豆包)".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://ark.cn-beijing.volces.com/api/v3".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "hunyuan".into(),
        display_name: "腾讯混元".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.hunyuan.cloud.tencent.com/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "baidu-cloud".into(),
        display_name: "百度智能云 (文心)".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://qianfan.baidubce.com/v2".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "modelscope".into(),
        display_name: "魔搭 (ModelScope)".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api-inference.modelscope.cn/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "infini".into(),
        display_name: "无问芯穹 (Infini)".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://cloud.infini-ai.com/maas/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    KnownProvider {
        id: "mimo".into(),
        display_name: "Xiaomi (MiMo)".into(),
        auth_type: AuthType::ApiKey,
        default_base_url: "https://api.xiaomimimo.com/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
    // ── API (Local) ───────────────────────────────────────────────
    KnownProvider {
        id: "ollama".into(),
        display_name: "Ollama (本地)".into(),
        auth_type: AuthType::None,
        default_base_url: "http://localhost:11434/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Local,
        supports_models: true,
    },
    KnownProvider {
        id: "local-minicpm".into(),
        display_name: "MiniCPM (本地)".into(),
        auth_type: AuthType::None,
        default_base_url: "".into(),
        default_api: ApiType::LocalMistralRs,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Local,
        supports_models: false,
    },
    ]
}

/// Look up a built-in provider by stable id.
#[must_use]
pub fn find(provider_id: &str) -> Option<KnownProvider> {
    builtin_providers()
        .into_iter()
        .find(|p| p.id == provider_id)
}

/// List all built-in providers.
#[must_use]
pub fn all() -> Vec<KnownProvider> {
    builtin_providers()
}

/// List providers by service category.
#[must_use]
pub fn by_category(category: &ServiceCategory) -> Vec<KnownProvider> {
    builtin_providers()
        .into_iter()
        .filter(|p| p.service_category == *category)
        .collect()
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ids_are_unique() {
        let mut ids: Vec<_> = builtin_providers().iter().map(|p| p.id.clone()).collect();
        ids.sort();
        let len = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), len, "Duplicate provider ID detected");
    }

    #[test]
    fn test_coding_plan_ids_end_with_coding() {
        for p in by_category(&ServiceCategory::CodingPlan) {
            assert!(
                p.id.ends_with("-coding"),
                "Coding plan provider '{}' should end with -coding",
                p.id
            );
        }
    }

    #[test]
    fn test_oauth_providers_have_oauth_auth() {
        for p in by_category(&ServiceCategory::OAuth) {
            assert!(
                p.auth_type == AuthType::OAuth,
                "OAuth provider '{}' should have OAuth auth type",
                p.id
            );
        }
    }

    #[test]
    fn test_find_well_known_providers() {
        assert!(find("openai").is_some());
        assert!(find("anthropic").is_some());
        assert!(find("deepseek").is_some());
        assert!(find("ollama").is_some());
        assert!(find("nonexistent").is_none());
    }

    #[test]
    fn test_api_category_has_most_providers() {
        let api_count = by_category(&ServiceCategory::Api).len();
        assert!(
            api_count > 15,
            "Expected more than 15 API providers, got {api_count}"
        );
    }

    #[test]
    fn test_all_providers_exceeds_25() {
        let providers = builtin_providers();
        assert!(
            providers.len() >= 25,
            "Expected at least 25 providers, got {}",
            providers.len()
        );
    }
}
