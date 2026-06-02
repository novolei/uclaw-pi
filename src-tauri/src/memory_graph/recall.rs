use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use super::models::*;
use super::search::MemorySearchResult;
use super::store::MemoryGraphStore;
use crate::memu::client::MemUClient;

// ─── Time Range ───────────────────────────────────────────────────────────

/// 时间范围筛选参数，用于时间基础搜索
#[derive(Debug, Clone)]
pub struct TimeRange {
    /// 起始时间（ISO 8601 字符串），None 表示无下限
    pub start: Option<String>,
    /// 结束时间（ISO 8601 字符串），None 表示无上限
    pub end: Option<String>,
    /// 是否偏好近期记忆（启用时间衰减分）
    pub prefer_recent: bool,
}

impl TimeRange {
    /// 最近 N 天
    pub fn last_n_days(days: u32) -> Self {
        let end = chrono::Utc::now();
        let start = end - chrono::Duration::days(days as i64);
        Self {
            start: Some(start.to_rfc3339()),
            end: Some(end.to_rfc3339()),
            prefer_recent: true,
        }
    }

    /// 无时间限制（全量）
    pub fn all() -> Self {
        Self {
            start: None,
            end: None,
            prefer_recent: false,
        }
    }
}

/// 计算时间衰减分数：基于高斯衰减函数
/// `exp(-(age_days / half_life_days)²)`
/// 其中 age_days 是记忆创建至今的天数，half_life_days 是半衰期（默认 7 天）
pub fn time_decay_score(created_at: &str, half_life_days: f64) -> f32 {
    let created = match chrono::DateTime::parse_from_rfc3339(created_at)
        .or_else(|_| chrono::DateTime::parse_from_rfc3339(&format!("{}Z", created_at)))
    {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(_) => return 0.0,
    };
    let now = chrono::Utc::now();
    let age_hours = (now - created).num_hours() as f64;
    let age_days = age_hours / 24.0;
    if age_days < 0.0 {
        return 1.0; // 未来时间，给予满分
    }
    (-(age_days / half_life_days).powi(2)).exp() as f32
}

// ─── Recall Configuration ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MemoryRecallConfig {
    pub boot_limit: usize,
    pub trigger_limit: usize,
    pub seed_limit: usize,
    pub expansion_limit: usize,
    pub recent_limit: usize,
    pub fusion_strategy: FusionStrategy,
    pub rrf_k: u32,
    pub fts_weight: f32,
    pub vector_weight: f32,
    /// How many top-N learned skills to auto-mount in the boot layer
    /// regardless of query relevance. Set to 0 to disable.
    /// Skills are ranked by usage_count DESC then updated_at DESC.
    pub boot_learned_skills_limit: usize,
    /// Maximum total tokens the memory context block may occupy.
    /// format_recall_for_prompt will progressively truncate content
    /// (shorten snippets, then drop lower-priority sections) to stay
    /// under this budget.  Set to 0 to disable the limit entirely.
    ///
    /// Default: 5000 — balances informativeness against leaving
    /// enough room for the conversation itself (typical agent-loop
    /// context windows are 128k–200k; 5k memory is ~2.5–4%).
    pub token_budget: usize,
    /// How many top seeds to feed into L4 graph BFS propagation.
    /// Default: 5
    pub layer_expanded_seed_take: usize,
    /// Maximum BFS hop depth for L4 graph propagation.
    /// Default: 2
    pub layer_expanded_max_depth: usize,
    /// Half-life (in days) for the Gaussian time-decay function.
    /// Default: 7.0
    pub time_decay_half_life_days: f64,
    /// When memU vector search is unavailable, multiply seed_limit by
    /// this factor for the FTS fallback search to compensate.
    /// Default: 2.0
    pub fts_fallback_limit_multiplier: f32,
    /// How many UserProfile nodes to auto-inject in a dedicated recall
    /// channel (independent of boot_limit quota). Set to 0 to disable.
    /// Default: 5
    pub boot_user_profile_limit: usize,

    // ─── Memory OS Foundation Phase 5 — recall boost ───────────────────
    //
    // Spec §4.5: EntityPage hits get a multiplicative boost so the
    // pre-compiled compiled_truth surfaces preferentially over per-event
    // Episode fragments at the same rank. Backlink count adds a
    // logarithmic bump so well-connected entities rise above isolated
    // ones. Both default to neutral (×1.0 / +0.0) for backward-compat;
    // a memubot_config knob can dial them up after the user observes
    // recall behavior in their workspace.
    /// Multiplicative boost applied to RRF / Weighted score when the
    /// node's `kind == 'entity_page'`. Default 1.0 (no change).
    /// Spec recommends 1.2 for gradual rollout, 1.5 once stable.
    ///
    /// Note: MemoryRecallConfig itself has no serde derive — the wire
    /// boundary is `MemoryRecallConfigDto` below, which IS serde-aware.
    /// Default propagation goes through `Default::default()` on this
    /// struct + the DTO's `unwrap_or` fallback in the `From` impl.
    pub entity_page_boost: f32,
    /// Additive weight on `log10(1 + backlink_count)` where
    /// `backlink_count` is `COUNT(memory_edges WHERE child_node_id = node)`.
    /// Default 0.0 (no change). Spec recommends 0.3 once Phase 2
    /// auto-link has populated enough edges for the signal to mean
    /// something.
    pub backlink_boost_weight: f32,

    /// When set (non-empty), load_context ALSO recalls from this MemoryAdapter
    /// backend and appends a `<recalled_memories>` block. None/empty = off (no
    /// behavior change). Adapter-routing, not graph-recall tuning, but lives
    /// here because this config is already threaded to both recall sites.
    pub prompt_recall_backend: Option<String>,
    /// Max entries pulled for the adapter-recall supplement. Default 5.
    pub prompt_recall_limit: usize,
    /// When set, bound each L3 `memu.retrieve` (vector search) to this many
    /// milliseconds; on timeout, fall back to FTS-only (identical to the path
    /// taken when memU errors). `None` = unbounded (default — legacy behavior).
    /// The pi-engine recall sites set this so a slow/empty memU L3 (a Python
    /// subprocess that can stall seconds) can't block same-turn recall.
    pub memu_retrieve_timeout_ms: Option<u64>,
}

// Standalone default functions retained: used both by
// `MemoryRecallConfig::default()` below AND (transitively) by the DTO's
// `#[serde(default)]` Option<f32> fields, which deserialize to None when
// absent and then `From<Dto>` falls back to the Default impl's value.
fn default_entity_page_boost() -> f32 { 1.0 }
fn default_backlink_boost_weight() -> f32 { 0.0 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FusionStrategy {
    Rrf,
    Weighted,
}

impl Default for MemoryRecallConfig {
    fn default() -> Self {
        Self {
            boot_limit: 8,
            trigger_limit: 6,
            seed_limit: 8,
            expansion_limit: 6,
            recent_limit: 3,
            fusion_strategy: FusionStrategy::Rrf,
            rrf_k: 60,
            fts_weight: 0.5,
            vector_weight: 0.5,
            boot_learned_skills_limit: 5,
            token_budget: 5000,
            layer_expanded_seed_take: 5,
            layer_expanded_max_depth: 2,
            time_decay_half_life_days: 7.0,
            fts_fallback_limit_multiplier: 2.0,
            boot_user_profile_limit: 5,
            // Phase 5 boost — neutral defaults so the upgrade is a no-op
            // until the user opts in via memory_recall_config IPC.
            entity_page_boost: default_entity_page_boost(),
            backlink_boost_weight: default_backlink_boost_weight(),
            prompt_recall_backend: None,
            prompt_recall_limit: 5,
            memu_retrieve_timeout_ms: None,
        }
    }
}

// ─── Recall Config DTO (for IPC/settings serialization) ──────────────────

/// Serializable mirror of MemoryRecallConfig for frontend settings.
/// All fields are Optional so partial updates work naturally.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallConfigDto {
    #[serde(default)]
    pub boot_limit: Option<usize>,
    #[serde(default)]
    pub trigger_limit: Option<usize>,
    #[serde(default)]
    pub seed_limit: Option<usize>,
    #[serde(default)]
    pub expansion_limit: Option<usize>,
    #[serde(default)]
    pub recent_limit: Option<usize>,
    #[serde(default)]
    pub fusion_strategy: Option<FusionStrategy>,
    #[serde(default)]
    pub rrf_k: Option<u32>,
    #[serde(default)]
    pub fts_weight: Option<f32>,
    #[serde(default)]
    pub vector_weight: Option<f32>,
    #[serde(default)]
    pub boot_learned_skills_limit: Option<usize>,
    #[serde(default)]
    pub token_budget: Option<usize>,
    #[serde(default)]
    pub layer_expanded_seed_take: Option<usize>,
    #[serde(default)]
    pub layer_expanded_max_depth: Option<usize>,
    #[serde(default)]
    pub time_decay_half_life_days: Option<f64>,
    #[serde(default)]
    pub fts_fallback_limit_multiplier: Option<f32>,
    #[serde(default)]
    pub boot_user_profile_limit: Option<usize>,
    /// Memory OS Phase 5 — EntityPage recall multiplier (default 1.0).
    #[serde(default)]
    pub entity_page_boost: Option<f32>,
    /// Memory OS Phase 5 — backlink-count log-weight (default 0.0).
    #[serde(default)]
    pub backlink_boost_weight: Option<f32>,
    #[serde(default)]
    pub prompt_recall_backend: Option<String>,
    #[serde(default)]
    pub prompt_recall_limit: Option<usize>,
    #[serde(default)]
    pub memu_retrieve_timeout_ms: Option<u64>,
}

impl From<MemoryRecallConfigDto> for MemoryRecallConfig {
    fn from(dto: MemoryRecallConfigDto) -> Self {
        let default = MemoryRecallConfig::default();
        Self {
            boot_limit: dto.boot_limit.unwrap_or(default.boot_limit),
            trigger_limit: dto.trigger_limit.unwrap_or(default.trigger_limit),
            seed_limit: dto.seed_limit.unwrap_or(default.seed_limit),
            expansion_limit: dto.expansion_limit.unwrap_or(default.expansion_limit),
            recent_limit: dto.recent_limit.unwrap_or(default.recent_limit),
            fusion_strategy: dto.fusion_strategy.unwrap_or(default.fusion_strategy),
            rrf_k: dto.rrf_k.unwrap_or(default.rrf_k),
            fts_weight: dto.fts_weight.unwrap_or(default.fts_weight),
            vector_weight: dto.vector_weight.unwrap_or(default.vector_weight),
            boot_learned_skills_limit: dto.boot_learned_skills_limit.unwrap_or(default.boot_learned_skills_limit),
            token_budget: dto.token_budget.unwrap_or(default.token_budget),
            layer_expanded_seed_take: dto.layer_expanded_seed_take.unwrap_or(default.layer_expanded_seed_take),
            layer_expanded_max_depth: dto.layer_expanded_max_depth.unwrap_or(default.layer_expanded_max_depth),
            time_decay_half_life_days: dto.time_decay_half_life_days.unwrap_or(default.time_decay_half_life_days),
            fts_fallback_limit_multiplier: dto.fts_fallback_limit_multiplier.unwrap_or(default.fts_fallback_limit_multiplier),
            boot_user_profile_limit: dto.boot_user_profile_limit.unwrap_or(default.boot_user_profile_limit),
            entity_page_boost: dto.entity_page_boost.unwrap_or(default.entity_page_boost),
            backlink_boost_weight: dto.backlink_boost_weight.unwrap_or(default.backlink_boost_weight),
            prompt_recall_backend: dto.prompt_recall_backend.or(default.prompt_recall_backend),
            prompt_recall_limit: dto.prompt_recall_limit.unwrap_or(default.prompt_recall_limit),
            memu_retrieve_timeout_ms: dto
                .memu_retrieve_timeout_ms
                .or(default.memu_retrieve_timeout_ms),
        }
    }
}

impl From<MemoryRecallConfig> for MemoryRecallConfigDto {
    fn from(cfg: MemoryRecallConfig) -> Self {
        Self {
            boot_limit: Some(cfg.boot_limit),
            trigger_limit: Some(cfg.trigger_limit),
            seed_limit: Some(cfg.seed_limit),
            expansion_limit: Some(cfg.expansion_limit),
            recent_limit: Some(cfg.recent_limit),
            fusion_strategy: Some(cfg.fusion_strategy),
            rrf_k: Some(cfg.rrf_k),
            fts_weight: Some(cfg.fts_weight),
            vector_weight: Some(cfg.vector_weight),
            boot_learned_skills_limit: Some(cfg.boot_learned_skills_limit),
            token_budget: Some(cfg.token_budget),
            layer_expanded_seed_take: Some(cfg.layer_expanded_seed_take),
            layer_expanded_max_depth: Some(cfg.layer_expanded_max_depth),
            time_decay_half_life_days: Some(cfg.time_decay_half_life_days),
            fts_fallback_limit_multiplier: Some(cfg.fts_fallback_limit_multiplier),
            boot_user_profile_limit: Some(cfg.boot_user_profile_limit),
            entity_page_boost: Some(cfg.entity_page_boost),
            backlink_boost_weight: Some(cfg.backlink_boost_weight),
            prompt_recall_backend: cfg.prompt_recall_backend,
            prompt_recall_limit: Some(cfg.prompt_recall_limit),
            memu_retrieve_timeout_ms: cfg.memu_retrieve_timeout_ms,
        }
    }
}

// ─── Recall Candidate ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallCandidate {
    pub node_id: String,
    pub title: String,
    pub content: String,
    pub kind: MemoryNodeKind,
    pub source: String,
    pub reason: String,
    pub score: Option<f32>,
    pub fts_rank: Option<u32>,
    pub vector_rank: Option<u32>,
    pub matched_keywords: Vec<String>,
    pub metadata: Option<serde_json::Value>,
}

// ─── Timeline Entry ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryTimelineEntry {
    pub node_id: String,
    pub title: String,
    pub content_snippet: String,
    pub kind: MemoryNodeKind,
    pub updated_at: String,
    /// 时间衰减分数（0.0-1.0），基于高斯衰减函数
    pub time_score: Option<f32>,
}

// ─── Recall Plan ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallPlan {
    pub boot: Vec<MemoryRecallCandidate>,
    pub triggered: Vec<MemoryRecallCandidate>,
    pub relevant: Vec<MemoryRecallCandidate>,
    pub expanded: Vec<MemoryRecallCandidate>,
    pub recent: Vec<MemoryTimelineEntry>,
}

// ─── Recall Explanation (debug) ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallExplanation {
    pub query: String,
    pub boot: Vec<MemoryRecallCandidate>,
    pub triggered: Vec<MemoryRecallCandidate>,
    pub relevant: Vec<MemoryRecallCandidate>,
    pub expanded: Vec<MemoryRecallCandidate>,
    pub recent: Vec<MemoryTimelineEntry>,
    pub total_candidates: usize,
}

// ─── Recall Engine ────────────────────────────────────────────────────────

pub struct MemoryRecallEngine {
    store: Arc<MemoryGraphStore>,
    memu_client: Option<Arc<MemUClient>>,
    config: MemoryRecallConfig,
}

impl MemoryRecallEngine {
    pub fn new(
        store: Arc<MemoryGraphStore>,
        memu_client: Option<Arc<MemUClient>>,
        config: MemoryRecallConfig,
    ) -> Self {
        Self {
            store,
            memu_client,
            config,
        }
    }

    /// Return a reference to the engine's configuration.
    pub fn config(&self) -> &MemoryRecallConfig {
        &self.config
    }

    /// Increment usage_count on every learned-skill candidate that was
    /// rendered into the prompt. Call this AFTER format_recall_for_prompt
    /// has been invoked and the system prompt has been handed to the LLM —
    /// the count tracks "how often this skill influenced a turn", which
    /// is the signal `boot_learned_skills_limit` ranks by.
    ///
    /// Best-effort: errors are logged and swallowed. Failing to bump
    /// usage_count must never fail an agent turn.
    pub fn record_used_skills(&self, plan: &MemoryRecallPlan) {
        let ids = collect_emitted_skill_ids(plan);
        if ids.is_empty() {
            return;
        }
        let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        if let Err(e) = self.store.bump_skill_usage(&id_refs) {
            tracing::warn!(
                count = ids.len(),
                err = %e,
                "memory_graph: failed to bump skill usage_count (non-fatal)"
            );
        }
    }

    /// Build a complete 5-layer recall plan.
    pub async fn build_recall_plan(
        &self,
        space_id: &str,
        user_input: &str,
        is_group_chat: bool,
    ) -> anyhow::Result<MemoryRecallPlan> {
        self.build_recall_plan_with_time(space_id, user_input, is_group_chat, None).await
    }

    /// Build a complete 5-layer recall plan with optional time range filtering.
    ///
    /// Time range filtering is applied as a post-filter on L2 (triggered),
    /// L3 (relevant), and L4 (expanded) layers, in addition to the native
    /// time-based search in L5 (recent).
    pub async fn build_recall_plan_with_time(
        &self,
        space_id: &str,
        user_input: &str,
        is_group_chat: bool,
        time_range: Option<&TimeRange>,
    ) -> anyhow::Result<MemoryRecallPlan> {
        let mut seen = HashSet::<String>::new();

        // L1 — Boot (boot nodes are always relevant, skip time filter)
        let boot = self.layer_boot(space_id, is_group_chat, &mut seen)?;
        info!(count = boot.len(), "recall: L1 boot candidates");

        // L2 — Triggered
        let triggered = self.layer_triggered(space_id, user_input, &mut seen)?;
        info!(count = triggered.len(), "recall: L2 triggered candidates (pre-filter)");
        let triggered = self.filter_by_time_range(triggered, time_range)?;

        // L3 — Relevant (seed)
        let relevant = self.layer_relevant(space_id, user_input, &mut seen).await?;
        info!(count = relevant.len(), "recall: L3 relevant candidates (pre-filter)");
        let relevant = self.filter_by_time_range(relevant, time_range)?;

        // L4 — Expanded (graph walk)
        let expanded = self.layer_expanded(&relevant, &mut seen)?;
        info!(count = expanded.len(), "recall: L4 expanded candidates (pre-filter)");
        let expanded = self.filter_by_time_range(expanded, time_range)?;

        // L5 — Recent (with native time range filtering)
        let recent = self.layer_recent_enhanced(space_id, time_range)?;
        info!(count = recent.len(), "recall: L5 recent entries");

        Ok(MemoryRecallPlan {
            boot,
            triggered,
            relevant,
            expanded,
            recent,
        })
    }

    /// Build a recall plan with full debug explanation.
    pub async fn explain_recall(
        &self,
        space_id: &str,
        user_input: &str,
    ) -> anyhow::Result<MemoryRecallExplanation> {
        let plan = self.build_recall_plan(space_id, user_input, false).await?;
        let total = plan.boot.len()
            + plan.triggered.len()
            + plan.relevant.len()
            + plan.expanded.len()
            + plan.recent.len();

        Ok(MemoryRecallExplanation {
            query: user_input.to_string(),
            boot: plan.boot,
            triggered: plan.triggered,
            relevant: plan.relevant,
            expanded: plan.expanded,
            recent: plan.recent,
            total_candidates: total,
        })
    }

    /// Format a recall plan as injectable system-prompt text.
    ///
    /// `token_budget` is the approximate maximum number of tokens the
    /// output may occupy.  The formatter applies progressive truncation:
    ///  1. Shorten per-item content snippets (200 → 120 → 80 chars)
    ///  2. Drop lower-priority sections entirely (recent → expanded →
    ///     relevant → triggered → boot)
    ///  3. Within the Learned Skills section (highest priority), truncate
    ///     individual SOP fields
    /// Pass 0 to disable the limit (full output, backward-compatible).
    pub fn format_recall_for_prompt(plan: &MemoryRecallPlan, token_budget: usize) -> String {
        // Note: the emit-detection logic below is duplicated by
        // collect_emitted_skill_ids() at the bottom of this file. Keep
        // the two in sync — both must agree on what counts as "rendered
        // into the Learned Skills section" for usage_count to be honest.
        let mut out = String::from("<memory_context>\n");

        // Collect all learned skills from every layer for a dedicated section
        let all_layers: Vec<&MemoryRecallCandidate> = plan
            .boot
            .iter()
            .chain(plan.triggered.iter())
            .chain(plan.relevant.iter())
            .chain(plan.expanded.iter())
            .collect();

        let mut learned_skills: Vec<&MemoryRecallCandidate> = Vec::new();
        let mut skill_ids: HashSet<String> = HashSet::new();

        for c in &all_layers {
            if c.kind == MemoryNodeKind::Procedure {
                if let Some(ref meta) = c.metadata {
                    let enabled = meta.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                    let skill_type = meta.get("skill_type").and_then(|v| v.as_str()).unwrap_or("");
                    if enabled && skill_type == "learned" {
                        learned_skills.push(c);
                        skill_ids.insert(c.node_id.clone());
                    } else if !enabled {
                        // disabled skills are filtered out entirely
                        skill_ids.insert(c.node_id.clone());
                    }
                }
            }
        }

        // Helper closure: should this candidate be rendered in its normal section?
        let should_render_normal = |c: &MemoryRecallCandidate| -> bool {
            if skill_ids.contains(&c.node_id) {
                return false; // handled in learned skills section or disabled
            }
            if c.kind == MemoryNodeKind::Procedure {
                if let Some(ref meta) = c.metadata {
                    let enabled = meta.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                    if !enabled {
                        return false;
                    }
                }
            }
            true
        };

        // ── Token budget tracking ──
        let mut used = 0usize;
        let budget = token_budget;
        let budget_enabled = budget > 0;

        // ── Helper: estimate approximate token count ──
        fn estimate_tokens(s: &str) -> usize {
            fn estimate_ascii_tokens(char_count: usize) -> usize {
                ((char_count as f32 / 4.0) * 1.3).ceil() as usize
            }

            let mut tokens: usize = 0;
            let mut ascii_word_len: usize = 0;

            for c in s.chars() {
                if c >= '\u{4e00}' && c <= '\u{9fff}' || c >= '\u{3400}' && c <= '\u{4dbf}' {
                    // CJK 字符：平均 ~1.5 tokens/char，保守估计为 1
                    if ascii_word_len > 0 {
                        tokens += estimate_ascii_tokens(ascii_word_len);
                        ascii_word_len = 0;
                    }
                    tokens += 1;
                } else if c.is_alphanumeric() || c == '_' {
                    ascii_word_len += 1;
                } else {
                    // 分隔符/标点
                    if ascii_word_len > 0 {
                        tokens += estimate_ascii_tokens(ascii_word_len);
                        ascii_word_len = 0;
                    }
                    if !c.is_whitespace() {
                        tokens += 1; // 标点通常是独立 token
                    }
                }
            }
            if ascii_word_len > 0 {
                tokens += estimate_ascii_tokens(ascii_word_len);
            }
            tokens.max(1)
        }

        // ── Helper: render a snippet respecting a max char budget ──
        let budgeted_snippet = |content: &str, max_chars: usize| -> String {
            truncate_content(content, max_chars)
        };

        // ── Helper: format a section header ──
        let format_section_header = |header: &str| -> String { header.to_string() };


        // ── Learned Skills section (top priority) ──
        if !learned_skills.is_empty() {
            let header_lines = [
                "## Learned Skills (已学技能)\n",
                "以下是你已学会的技能 SOP。**强制规则**：\n",
                " 1. 任务开始前先判断本次请求是否匹配下列任一技能的「适用场景」。\n",
                " 2. 如匹配，**必须**在你的响应开头以引用块形式声明：\n",
                "   `> 应用技能：<技能名> — <一句话说明为何匹配>`\n",
                "   然后严格按该技能的「SOP 步骤」执行，不要绕过 / 简化 / 自创流程。\n",
                " 3. 如不匹配任何技能，无需声明，正常回复即可。\n",
                " 这一规则用于积累\"技能是否真正被使用\"的反馈数据，请配合执行。\n\n",
            ];
            let header: String = header_lines.iter().map(|s| *s).collect();
            out.push_str(&header);
            if budget_enabled {
                used += estimate_tokens(&header);
            }
        
            // Determine how much budget remains per skill (on average).
            let skills_count = learned_skills.len();
            let rem = if budget_enabled && used < budget {
                (budget - used) / skills_count.max(1)
            } else {
                usize::MAX
            };
        
            // Progressive truncation levels for SOP fields based on remaining
            // budget per skill:
            //   > 600 tokens/skill → full SOP
            //   400-600 → truncate steps to first 3, drop pitfalls
            //   200-400 → title + context only
            //   < 200   → title + context truncated to 80 chars
            for c in &learned_skills {
                let meta = c.metadata.as_ref().unwrap(); // safe: checked above
                let context = meta.get("context").and_then(|v| v.as_str()).unwrap_or("");
                let principles = meta.get("principles").and_then(|v| v.as_str()).unwrap_or("");
                let steps = meta.get("steps").and_then(|v| v.as_str()).unwrap_or("");
                let pitfalls = meta.get("pitfalls").and_then(|v| v.as_str()).unwrap_or("");
        
                out.push_str(&format!("### {}\n", c.title));
        
                let ctx_max = if rem >= 400 { 200 } else { 80 };
                if !context.is_empty() {
                    let ctx_text = budgeted_snippet(context, ctx_max);
                    out.push_str(&format!("**适用场景**: {}\n", ctx_text));
                }
        
                if rem >= 200 && !principles.is_empty() {
                    let p_text = budgeted_snippet(principles, 150);
                    out.push_str(&format!("**核心原则**: {}\n", p_text));
                }
        
                if rem >= 400 && !steps.is_empty() {
                    out.push_str("**SOP 步骤**:\n");
                    let step_lines: Vec<&str> = steps.lines()
                        .filter(|l| !l.trim().is_empty())
                        .collect();
                    let step_limit = if rem >= 600 { step_lines.len() } else { step_lines.len().min(3) };
                    for line in step_lines.iter().take(step_limit) {
                        let step_text = budgeted_snippet(line, 120);
                        out.push_str(&format!("{}\n", step_text));
                    }
                    if step_limit < step_lines.len() {
                        out.push_str(&format!("  _(... 共 {} 步，已省略)_\n", step_lines.len() - step_limit));
                    }
                }
        
                if rem >= 400 && !pitfalls.is_empty() {
                    let pf_text = budgeted_snippet(pitfalls, 150);
                    out.push_str(&format!("**注意事项**: {}\n", pf_text));
                }
        
                out.push('\n');
        
                if budget_enabled {
                    used = estimate_tokens(&out);
                }
            }
        }

        // ── Normal sections (filtering out learned/disabled skills) ──
        // Section priority: Boot > Triggered > Relevant > Expanded > Recent.
        // Use TokenBudgetAllocation for per-section budget management and
        // CrossLayerDedup for cross-layer deduplication with score merging.

        let boot_normal: Vec<&MemoryRecallCandidate> =
            plan.boot.iter().filter(|c| should_render_normal(*c)).collect();
        let triggered_normal: Vec<&MemoryRecallCandidate> =
            plan.triggered.iter().filter(|c| should_render_normal(*c)).collect();
        let relevant_normal: Vec<&MemoryRecallCandidate> =
            plan.relevant.iter().filter(|c| should_render_normal(*c)).collect();
        let expanded_normal: Vec<&MemoryRecallCandidate> =
            plan.expanded.iter().filter(|c| should_render_normal(*c)).collect();

        // Initialize per-section budget allocation from remaining tokens
        let remaining_budget = if budget_enabled { budget.saturating_sub(used) } else { 0 };
        let mut allocation = TokenBudgetAllocation::from_total(remaining_budget);

        // CrossLayerDedup: register all candidates for multi-source bonus tracking
        let mut dedup = CrossLayerDedup::new();
        for c in &boot_normal {
            dedup.should_include(&c.node_id, c.score.unwrap_or(1.0), "boot");
        }
        for c in &triggered_normal {
            dedup.should_include(&c.node_id, c.score.unwrap_or(0.8), "triggered");
        }
        for c in &relevant_normal {
            dedup.should_include(&c.node_id, c.score.unwrap_or(0.5), "relevant");
        }
        for c in &expanded_normal {
            dedup.should_include(&c.node_id, c.score.unwrap_or(0.3), "expanded");
        }

        // Helper: determine snippet max-chars from a given section budget
        let snippet_max_for_budget = |section_budget: usize| -> usize {
            if !budget_enabled { return 200; }
            if section_budget > 800 { 200 } else if section_budget > 400 { 120 } else { 60 }
        };

        // ── Boot ──
        if !boot_normal.is_empty() {
            let section_budget = allocation.budget_for("boot");
            let snippet_max = snippet_max_for_budget(section_budget);
            let mut section_out = format_section_header("## Boot Memories (启动记忆)\n");
            for c in &boot_normal {
                if budget_enabled && estimate_tokens(&section_out) >= section_budget {
                    break;
                }
                section_out.push_str(&format!(
                    "- [{}] {}: {}\n",
                    capitalize_kind(&c.kind),
                    c.title,
                    budgeted_snippet(&c.content, snippet_max),
                ));
            }
            section_out.push('\n');
            let section_tokens = estimate_tokens(&section_out);
            if budget_enabled {
                allocation.cascade_unused("boot", section_tokens);
            }
            out.push_str(&section_out);
        } else if budget_enabled {
            allocation.cascade_unused("boot", 0);
        }

        // ── Triggered ──
        if !triggered_normal.is_empty() {
            let section_budget = allocation.budget_for("triggered");
            let snippet_max = snippet_max_for_budget(section_budget);
            let mut section_out = format_section_header("## Triggered Memories (触发记忆)\n");
            for c in &triggered_normal {
                if budget_enabled && estimate_tokens(&section_out) >= section_budget {
                    break;
                }
                let kw_display = if c.matched_keywords.is_empty() {
                    String::new()
                } else {
                    format!("（触发词：{}）", c.matched_keywords.join(", "))
                };
                section_out.push_str(&format!(
                    "- [{}] {}: {}{}\n",
                    capitalize_kind(&c.kind),
                    c.title,
                    budgeted_snippet(&c.content, snippet_max),
                    kw_display,
                ));
            }
            section_out.push('\n');
            let section_tokens = estimate_tokens(&section_out);
            if budget_enabled {
                allocation.cascade_unused("triggered", section_tokens);
            }
            out.push_str(&section_out);
        } else if budget_enabled {
            allocation.cascade_unused("triggered", 0);
        }

        // ── Relevant ──
        if !relevant_normal.is_empty() {
            let section_budget = allocation.budget_for("relevant");
            let snippet_max = snippet_max_for_budget(section_budget);
            let mut section_out = format_section_header("## Relevant Memories (相关记忆)\n");
            for c in &relevant_normal {
                if budget_enabled && estimate_tokens(&section_out) >= section_budget {
                    break;
                }
                let bonus = dedup.multi_source_bonus(&c.node_id);
                let effective_score = c.score.unwrap_or(0.0) + bonus;
                let score_display = format!("（相关度：{:.2}）", effective_score);
                section_out.push_str(&format!(
                    "- [{}] {}: {}{}\n",
                    capitalize_kind(&c.kind),
                    c.title,
                    budgeted_snippet(&c.content, snippet_max),
                    score_display,
                ));
            }
            section_out.push('\n');
            let section_tokens = estimate_tokens(&section_out);
            if budget_enabled {
                allocation.cascade_unused("relevant", section_tokens);
            }
            out.push_str(&section_out);
        } else if budget_enabled {
            allocation.cascade_unused("relevant", 0);
        }

        // ── Expanded ──
        if !expanded_normal.is_empty() {
            let section_budget = allocation.budget_for("expanded");
            let snippet_max = snippet_max_for_budget(section_budget).min(120);
            let mut section_out = format_section_header("## Expanded Context (扩展上下文)\n");
            for c in &expanded_normal {
                if budget_enabled && estimate_tokens(&section_out) >= section_budget {
                    break;
                }
                section_out.push_str(&format!(
                    "- [{}] {}: {}（via {}）\n",
                    capitalize_kind(&c.kind),
                    c.title,
                    budgeted_snippet(&c.content, snippet_max),
                    c.reason,
                ));
            }
            section_out.push('\n');
            let section_tokens = estimate_tokens(&section_out);
            if budget_enabled {
                allocation.cascade_unused("expanded", section_tokens);
            }
            out.push_str(&section_out);
        } else if budget_enabled {
            allocation.cascade_unused("expanded", 0);
        }

        // ── Recent ── (lowest priority, receives cascaded surplus)
        if !plan.recent.is_empty() {
            let section_budget = allocation.budget_for("recent");
            if !budget_enabled || section_budget > 30 {
                let mut section_out = format_section_header("## Recent Activity (近期活动)\n");
                for e in &plan.recent {
                    if budget_enabled && estimate_tokens(&section_out) >= section_budget {
                        break;
                    }
                    let date = e.updated_at.get(..10).unwrap_or(&e.updated_at);
                    section_out.push_str(&format!(
                        "- [{}] {}: {}\n",
                        capitalize_kind(&e.kind),
                        date,
                        e.title,
                    ));
                }
                section_out.push('\n');
                out.push_str(&section_out);
            }
        }

        out.push_str("</memory_context>");
        out
    }

    // ── L1 Boot ──────────────────────────────────────────────────────────

    fn layer_boot(
        &self,
        space_id: &str,
        is_group_chat: bool,
        seen: &mut HashSet<String>,
    ) -> anyhow::Result<Vec<MemoryRecallCandidate>> {
        let details = self.store.list_boot_nodes(space_id, self.config.boot_limit)?;
        let mut candidates = Vec::new();

        for detail in details {
            let node = &detail.node;

            // In group chat, filter out Curated/UserProfile/Episode
            if is_group_chat {
                match node.kind {
                    MemoryNodeKind::Curated
                    | MemoryNodeKind::UserProfile
                    | MemoryNodeKind::Episode => continue,
                    _ => {}
                }
            }

            let content = match &detail.active_version {
                Some(v) => v.content.clone(),
                None => continue, // skip if no active version
            };

            seen.insert(node.id.clone());
            candidates.push(MemoryRecallCandidate {
                node_id: node.id.clone(),
                title: node.title.clone(),
                content,
                kind: node.kind,
                source: "boot".to_string(),
                reason: "Explicit boot membership".to_string(),
                score: None,
                fts_rank: None,
                vector_rank: None,
                matched_keywords: detail.keywords.clone(),
                metadata: node.metadata.clone(),
            });
        }

        // ── Auto-mount UserProfile nodes (dedicated channel) ─────
        // UserProfile nodes get their own quota so they never compete
        // with boot nodes for slots.
        if !is_group_chat && self.config.boot_user_profile_limit > 0 {
            let profiles = self
                .store
                .list_nodes_by_kind(space_id, MemoryNodeKind::UserProfile, self.config.boot_user_profile_limit)
                .unwrap_or_default();
            for node in profiles {
                if seen.contains(&node.id) {
                    continue;
                }
                let content = match self.store.get_active_version(&node.id) {
                    Ok(Some(v)) => v.content,
                    _ => continue,
                };
                seen.insert(node.id.clone());
                candidates.push(MemoryRecallCandidate {
                    node_id: node.id.clone(),
                    title: node.title.clone(),
                    content,
                    kind: node.kind,
                    source: "boot".to_string(),
                    reason: "UserProfile (dedicated channel)".to_string(),
                    score: None,
                    fts_rank: None,
                    vector_rank: None,
                    matched_keywords: Vec::new(),
                    metadata: node.metadata.clone(),
                });
            }
        }

        // ── Auto-mount top-N learned skills regardless of query ─────
        // Without this, learned skills only surface when L2 (keyword) or
        // L3 (FTS) lights them up — and FTS5 unicode61 needs literal
        // word overlap. A power user's high-usage skill should always be
        // in context. boot_learned_skills_limit=0 disables this layer.
        if self.config.boot_learned_skills_limit > 0 {
            let learned = self
                .store
                .list_top_learned_skills(space_id, self.config.boot_learned_skills_limit)
                .unwrap_or_default();
            for detail in learned {
                let node = &detail.node;
                if seen.contains(&node.id) {
                    continue;
                }
                let content = match &detail.active_version {
                    Some(v) => v.content.clone(),
                    None => continue,
                };
                seen.insert(node.id.clone());
                candidates.push(MemoryRecallCandidate {
                    node_id: node.id.clone(),
                    title: node.title.clone(),
                    content,
                    kind: node.kind,
                    source: "boot".to_string(),
                    reason: "Top learned skill (auto-mount)".to_string(),
                    score: None,
                    fts_rank: None,
                    vector_rank: None,
                    matched_keywords: detail.keywords.clone(),
                    metadata: node.metadata.clone(),
                });
            }
        }

        Ok(candidates)
    }

    // ── L2 Triggered ─────────────────────────────────────────────────────

    fn layer_triggered(
        &self,
        space_id: &str,
        user_input: &str,
        seen: &mut HashSet<String>,
    ) -> anyhow::Result<Vec<MemoryRecallCandidate>> {
        let tokens = tokenize_input(user_input);
        let token_refs: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();

        // Keyword match
        let keyword_nodes = self.store.keyword_search(space_id, &token_refs)?;

        // Trigger text match
        let trigger_edges = self.store.trigger_text_search(space_id, user_input)?;

        let mut candidates = Vec::new();
        let mut triggered_ids = HashSet::new();

        // Process keyword matches
        for node in keyword_nodes {
            if seen.contains(&node.id) || triggered_ids.contains(&node.id) {
                continue;
            }
            let content = match self.store.get_active_version(&node.id)? {
                Some(v) => v.content,
                None => continue,
            };
            let matched: Vec<String> = tokens
                .iter()
                .filter(|t| {
                    node.title.to_lowercase().contains(&t.to_lowercase())
                        || content.to_lowercase().contains(&t.to_lowercase())
                })
                .cloned()
                .collect();

            triggered_ids.insert(node.id.clone());
            candidates.push(MemoryRecallCandidate {
                node_id: node.id.clone(),
                title: node.title.clone(),
                content,
                kind: node.kind,
                source: "trigger".to_string(),
                reason: format!("Keyword matched: [{}]", matched.join(", ")),
                score: None,
                fts_rank: None,
                vector_rank: None,
                matched_keywords: matched,
                metadata: node.metadata.clone(),
            });
        }

        // Process trigger text matches
        for edge in trigger_edges {
            let node_id = &edge.child_node_id;
            if seen.contains(node_id) || triggered_ids.contains(node_id) {
                continue;
            }
            let node = match self.store.get_node(node_id)? {
                Some(n) => n,
                None => continue,
            };
            let content = match self.store.get_active_version(node_id)? {
                Some(v) => v.content,
                None => continue,
            };
            let trigger_text = edge.trigger_text.clone().unwrap_or_default();

            triggered_ids.insert(node_id.clone());
            candidates.push(MemoryRecallCandidate {
                node_id: node_id.clone(),
                title: node.title.clone(),
                content,
                kind: node.kind,
                source: "trigger".to_string(),
                reason: format!("Disclosure text matched: [{}]", trigger_text),
                score: None,
                fts_rank: None,
                vector_rank: None,
                matched_keywords: vec![trigger_text],
                metadata: node.metadata.clone(),
            });
        }

        // Sort by priority (we don't have direct priority, sort by update time desc)
        candidates.truncate(self.config.trigger_limit);

        for c in &candidates {
            seen.insert(c.node_id.clone());
        }

        Ok(candidates)
    }

    /// Memory OS Phase 5 — batch fetch `(kind, backlink_count)` for the
    /// given node ids. Single SQL round-trip via a `IN (?, ?, ...)`
    /// LEFT JOIN, returning the partial map (ids missing from the
    /// result map cleanly to "no boost" in `layer_relevant`).
    ///
    /// Backlink count counts edges where `child_node_id = id`. Edges
    /// added by Phase 2 auto-link, by Phase 4 explicit create_edge,
    /// and by older Procedure/Episode wiring all contribute.
    fn fetch_boost_signals(
        &self,
        ids: &[String],
    ) -> anyhow::Result<HashMap<String, (MemoryNodeKind, i64)>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock: {}", e))?;
        // ?1, ?2, ... placeholders for the IN-clause.
        let placeholders = (1..=ids.len())
            .map(|i| format!("?{}", i))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT n.id, n.kind, ( \
               SELECT COUNT(*) FROM memory_edges e WHERE e.child_node_id = n.id \
             ) AS backlinks \
             FROM memory_nodes n \
             WHERE n.id IN ({placeholders})"
        );
        // Phase 1 fix-up E0597 lifetime pattern: separate stmt + rows
        // bindings so MappedRows drops before stmt does.
        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            ids.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut out = HashMap::with_capacity(ids.len());
        for r in rows.flatten() {
            let (id, kind_str, backlinks) = r;
            out.insert(id, (MemoryNodeKind::from_str(&kind_str), backlinks));
        }
        Ok(out)
    }

    // ── L3 Relevant (seed) ───────────────────────────────────────────────

    async fn layer_relevant(
        &self,
        space_id: &str,
        user_input: &str,
        seen: &mut HashSet<String>,
    ) -> anyhow::Result<Vec<MemoryRecallCandidate>> {
        // 根据 memU 可用性调整 FTS 搜索范围
        let fts_limit = if self.memu_client.is_some() {
            self.config.seed_limit * 2
        } else {
            // memU 不可用时，扩大 FTS 搜索范围以补偿
            (self.config.seed_limit as f32 * self.config.fts_fallback_limit_multiplier) as usize
        };

        // FTS5 search — trigram tokenizer handles CJK natively (since V31 migration).
        // No need for enhanced n-gram query workaround; raw user input works well
        // for both CJK and Latin text with trigram tokenization.
        let fts_results = self
            .store
            .fts_search(space_id, user_input, fts_limit)
            .unwrap_or_default();

        // Vector search via memU (if available). memU's retrieve goes through a
        // Python subprocess and can stall for seconds; when the caller sets
        // `memu_retrieve_timeout_ms` (the pi-engine recall sites do), bound it
        // and fall back to FTS-only on timeout — identical to the memU-error
        // path — so a slow/empty L3 can't block same-turn recall.
        let vector_results = if let Some(ref memu) = self.memu_client {
            let queries = vec![serde_json::json!({
                "role": "user",
                "content": user_input,
            })];
            let fetch = async {
                match memu.retrieve(queries, None, None).await {
                    Ok(result) => result.items,
                    Err(e) => {
                        info!(error = %e, "recall: memU retrieve failed, falling back to FTS only");
                        Vec::new()
                    }
                }
            };
            match self.config.memu_retrieve_timeout_ms {
                Some(ms) => {
                    match tokio::time::timeout(std::time::Duration::from_millis(ms), fetch).await {
                        Ok(items) => items,
                        Err(_) => {
                            info!(
                                timeout_ms = ms,
                                "recall: memU retrieve timed out, falling back to FTS only"
                            );
                            Vec::new()
                        }
                    }
                }
                None => fetch.await,
            }
        } else {
            Vec::new()
        };

        // Build rank maps
        let mut fts_rank_map: HashMap<String, (u32, &MemorySearchResult)> = HashMap::new();
        for (rank, r) in fts_results.iter().enumerate() {
            fts_rank_map.insert(r.node_id.clone(), (rank as u32 + 1, r));
        }

        let mut vector_rank_map: HashMap<String, u32> = HashMap::new();
        for (rank, item) in vector_results.iter().enumerate() {
            if let Some(id) = item.get("node_id").and_then(|v| v.as_str()) {
                vector_rank_map.insert(id.to_string(), rank as u32 + 1);
            }
        }

        // Collect all candidate node_ids
        let mut all_ids: Vec<String> = Vec::new();
        for id in fts_rank_map.keys() {
            if !all_ids.contains(id) {
                all_ids.push(id.clone());
            }
        }
        for id in vector_rank_map.keys() {
            if !all_ids.contains(id) {
                all_ids.push(id.clone());
            }
        }

        // Memory OS Phase 5 — batch-fetch the (kind, backlink_count)
        // signals we need to apply the EntityPage boost + backlink
        // log-weight inside the fusion loop. One round-trip to SQLite
        // instead of N round-trips inside the loop.
        //
        // backlink_count is `COUNT(memory_edges WHERE child_node_id = node)`.
        // We deliberately don't filter by space_id here: cross-space
        // citations remain rare in practice and including them avoids
        // dropping the signal for nodes that get referenced by Shared
        // memory in other workspaces.
        let boost_signals: HashMap<String, (MemoryNodeKind, i64)> = if all_ids.is_empty()
            || (self.config.entity_page_boost == 1.0
                && self.config.backlink_boost_weight == 0.0)
        {
            // Both knobs neutral → no need to query.
            HashMap::new()
        } else {
            self.fetch_boost_signals(&all_ids).unwrap_or_default()
        };

        // Fuse scores
        let k = self.config.rrf_k;
        let mut scored: Vec<(String, f32, Option<u32>, Option<u32>)> = Vec::new();

        for id in &all_ids {
            if seen.contains(id) {
                continue;
            }
            let fts_r = fts_rank_map.get(id).map(|(r, _)| *r);
            let vec_r = vector_rank_map.get(id).copied();

            let base_score = match &self.config.fusion_strategy {
                FusionStrategy::Rrf => {
                    let mut s = 0.0f32;
                    if let Some(r) = fts_r {
                        s += 1.0 / (k as f32 + r as f32);
                    }
                    if let Some(r) = vec_r {
                        s += 1.0 / (k as f32 + r as f32);
                    }
                    s
                }
                FusionStrategy::Weighted => {
                    let fts_score = fts_r
                        .and_then(|_r| fts_rank_map.get(id).map(|(_, res)| res.score))
                        .unwrap_or(0.0);
                    let vec_score = vec_r.map(|r| 1.0 / r as f32).unwrap_or(0.0);
                    self.config.fts_weight * fts_score + self.config.vector_weight * vec_score
                }
            };

            // Apply Phase 5 boost layered on top of base score. When the
            // config knobs are at their defaults (1.0 / 0.0), boost ==
            // base_score and the upgrade is invisible. The HashMap miss
            // case (boost_signals empty OR id absent) also degrades to
            // no boost.
            let score = if let Some((kind, backlinks)) = boost_signals.get(id) {
                let mut s = base_score;
                if *kind == MemoryNodeKind::EntityPage && self.config.entity_page_boost != 1.0 {
                    s *= self.config.entity_page_boost;
                }
                if self.config.backlink_boost_weight > 0.0 {
                    s += ((*backlinks as f32) + 1.0).log10() * self.config.backlink_boost_weight;
                }
                s
            } else {
                base_score
            };

            scored.push((id.clone(), score, fts_r, vec_r));
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(self.config.seed_limit);

        let mut candidates = Vec::new();
        for (node_id, score, fts_r, vec_r) in scored {
            let (title, content, kind, node_metadata) = if let Some((_, fts_res)) = fts_rank_map.get(&node_id) {
                // Use FTS result data, but get full content from active version
                let full_content = self
                    .store
                    .get_active_version(&node_id)?
                    .map(|v| v.content)
                    .unwrap_or_else(|| fts_res.content_snippet.clone());
                let meta = self.store.get_node(&node_id)?.and_then(|n| n.metadata);
                (fts_res.title.clone(), full_content, fts_res.kind, meta)
            } else {
                // Vector-only result — fetch from store
                match self.store.get_node(&node_id)? {
                    Some(node) => {
                        let content = self
                            .store
                            .get_active_version(&node_id)?
                            .map(|v| v.content)
                            .unwrap_or_default();
                        let meta = node.metadata.clone();
                        (node.title, content, node.kind, meta)
                    }
                    None => continue,
                }
            };

            let fts_label = fts_r
                .map(|r| format!("fts #{}", r))
                .unwrap_or_else(|| "fts -".to_string());
            let vec_label = vec_r
                .map(|r| format!("vector #{}", r))
                .unwrap_or_else(|| "vector -".to_string());

            seen.insert(node_id.clone());
            candidates.push(MemoryRecallCandidate {
                node_id,
                title,
                content,
                kind,
                source: "search".to_string(),
                reason: format!("Hybrid recall hit ({}, {})", fts_label, vec_label),
                score: Some(score),
                fts_rank: fts_r,
                vector_rank: vec_r,
                matched_keywords: Vec::new(),
                metadata: node_metadata,
            });
        }

        Ok(candidates)
    }

    // ── L4 Expanded (graph walk with BFS propagation) ────────────────────

    /// 图传播扩展：从 L3 相关节点出发，通过 BFS 沿 memory_edges 进行多跳传播。
    ///
    /// 替代原先的单层 parent/child/route 展开，使用
    /// `MemoryGraphStore::graph_propagation_search` 的加权 BFS 算法：
    /// - 关系类型权重：parent_of/child_of=1.0, derived_from=0.8, related_to=0.7, etc.
    /// - 衰减因子 0.6，每跳得分递减
    /// - priority 字段作为附加加成
    fn layer_expanded(
        &self,
        seeds: &[MemoryRecallCandidate],
        seen: &mut HashSet<String>,
    ) -> anyhow::Result<Vec<MemoryRecallCandidate>> {
        if seeds.is_empty() {
            return Ok(Vec::new());
        }

        // Collect top-N seed node IDs for BFS propagation (P1: configurable)
        let seed_ids: Vec<String> = seeds
            .iter()
            .take(self.config.layer_expanded_seed_take)
            .map(|s| s.node_id.clone())
            .collect();

        // BFS graph propagation (P1: configurable max_depth)
        let propagation_results = self.store.graph_propagation_search(
            &seed_ids,
            self.config.layer_expanded_max_depth,
            self.config.expansion_limit * 3,
        )?;

        // P3: 用目标节点新近度重新评分
        let mut candidates = Vec::new();
        for gr in &propagation_results {
            if seen.contains(&gr.node_id) {
                continue;
            }
            if let Some(c) = self.make_expansion_candidate(
                &gr.node_id,
                "graph_propagation",
                &format!("graph bfs depth={} score={:.3}", gr.depth, gr.score),
            )? {
                let mut candidate = c;

                // 用 time_decay_score 对传播得分做新近度加权
                let recency = if let Ok(Some(node)) = self.store.get_node(&gr.node_id) {
                    time_decay_score(&node.updated_at, self.config.time_decay_half_life_days)
                } else {
                    0.5 // 默认中间值
                };
                // 综合得分 = 传播得分 * (0.5 + 0.5 * recency)
                let weighted_score = gr.score * (0.5 + 0.5 * recency);
                candidate.score = Some(weighted_score);

                seen.insert(candidate.node_id.clone());
                candidates.push(candidate);
            }
        }

        // 按加权得分重新排序
        candidates.sort_by(|a, b| {
            b.score
                .unwrap_or(0.0)
                .partial_cmp(&a.score.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(self.config.expansion_limit);
        Ok(candidates)
    }

    fn make_expansion_candidate(
        &self,
        node_id: &str,
        source: &str,
        reason: &str,
    ) -> anyhow::Result<Option<MemoryRecallCandidate>> {
        let node = match self.store.get_node(node_id)? {
            Some(n) => n,
            None => return Ok(None),
        };
        let content = match self.store.get_active_version(node_id)? {
            Some(v) => v.content,
            None => return Ok(None),
        };
        Ok(Some(MemoryRecallCandidate {
            node_id: node_id.to_string(),
            title: node.title.clone(),
            content,
            kind: node.kind,
            source: source.to_string(),
            reason: reason.to_string(),
            score: None,
            fts_rank: None,
            vector_rank: None,
            matched_keywords: Vec::new(),
            metadata: node.metadata.clone(),
        }))
    }

    /// Post-filter candidates by time range.
    ///
    /// If time_range is None, returns candidates unchanged.
    /// Otherwise, queries each candidate's node created_at and removes
    /// those outside the specified range. Boot nodes are never filtered
    /// (they should be handled at the caller level).
    fn filter_by_time_range(
        &self,
        candidates: Vec<MemoryRecallCandidate>,
        time_range: Option<&TimeRange>,
    ) -> anyhow::Result<Vec<MemoryRecallCandidate>> {
        let tr = match time_range {
            Some(t) if t.start.is_some() || t.end.is_some() => t,
            _ => return Ok(candidates),
        };

        let start_dt = tr.start.as_ref().and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .or_else(|_| chrono::DateTime::parse_from_rfc3339(&format!("{}Z", s)))
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        });
        let end_dt = tr.end.as_ref().and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .or_else(|_| chrono::DateTime::parse_from_rfc3339(&format!("{}Z", s)))
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        });

        if start_dt.is_none() && end_dt.is_none() {
            return Ok(candidates);
        }

        let filtered: Vec<MemoryRecallCandidate> = candidates
            .into_iter()
            .filter(|c| {
                // Query node creation time
                let node = match self.store.get_node(&c.node_id) {
                    Ok(Some(n)) => n,
                    _ => return true, // keep if we can't determine time
                };
                let created = match chrono::DateTime::parse_from_rfc3339(&node.created_at)
                    .or_else(|_| chrono::DateTime::parse_from_rfc3339(&format!("{}Z", node.created_at)))
                {
                    Ok(dt) => dt.with_timezone(&chrono::Utc),
                    Err(_) => return true, // keep if unparseable
                };
                if let Some(start) = start_dt {
                    if created < start {
                        return false;
                    }
                }
                if let Some(end) = end_dt {
                    if created > end {
                        return false;
                    }
                }
                true
            })
            .collect();

        Ok(filtered)
    }

    // ── L5 Recent ────────────────────────────────────────────────────────

    fn layer_recent(
        &self,
        space_id: &str,
    ) -> anyhow::Result<Vec<MemoryTimelineEntry>> {
        self.layer_recent_enhanced(space_id, None)
    }

    /// L5 — Recent with optional time range filtering and decay scoring.
    fn layer_recent_enhanced(
        &self,
        space_id: &str,
        time_range: Option<&TimeRange>,
    ) -> anyhow::Result<Vec<MemoryTimelineEntry>> {
        let nodes = if let Some(tr) = time_range {
            self.store.search_by_time_range(
                space_id,
                tr.start.as_deref(),
                tr.end.as_deref(),
                self.config.recent_limit,
            )?
        } else {
            self.store.list_recent_nodes(space_id, self.config.recent_limit)?
        };

        let apply_decay = time_range.map(|tr| tr.prefer_recent).unwrap_or(false);

        let mut entries = Vec::new();
        for node in nodes {
            let snippet = self
                .store
                .get_active_version(&node.id)?
                .map(|v| truncate_content(&v.content, 120))
                .unwrap_or_default();

            let time_score = if apply_decay {
                Some(time_decay_score(&node.created_at, self.config.time_decay_half_life_days))
            } else {
                None
            };

            entries.push(MemoryTimelineEntry {
                node_id: node.id,
                title: node.title,
                content_snippet: snippet,
                kind: node.kind,
                updated_at: node.updated_at,
                time_score,
            });
        }

        Ok(entries)
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────

/// 构建增强的 FTS5 查询字符串：3-gram + 停用词过滤
///
/// 历史遗留函数 — 在 memory_fts 使用 unicode61 tokenizer 时作为 CJK 搜索的
/// 变通方案。自 V31 迁移将 tokenizer 切换为 trigram 后不再需要，保留作为参考。
#[allow(dead_code)]
fn build_enhanced_fts_query(input: &str) -> String {
    // 中文停用词
    const CJK_STOP_WORDS: &[char] = &[
        '的', '了', '是', '在', '我', '有', '和', '就', '不', '人', '都', '一', '个', '上',
        '也', '很', '到', '说', '要', '去', '你', '会', '着', '没', '那', '这', '他', '她',
    ];

    let mut english_words = Vec::new();
    let mut cjk_chars = Vec::new();
    let mut current_ascii = String::new();

    for c in input.chars() {
        if c >= '\u{4e00}' && c <= '\u{9fff}' || c >= '\u{3400}' && c <= '\u{4dbf}' {
            // CJK 字符
            if !current_ascii.is_empty() {
                english_words.push(std::mem::take(&mut current_ascii));
            }
            if !CJK_STOP_WORDS.contains(&c) {
                cjk_chars.push(c);
            }
        } else if c.is_alphanumeric() {
            current_ascii.push(c);
        } else {
            if !current_ascii.is_empty() {
                english_words.push(std::mem::take(&mut current_ascii));
            }
        }
    }
    if !current_ascii.is_empty() {
        english_words.push(current_ascii);
    }

    // CJK: 3-gram 滑动窗口
    let cjk_tokens: Vec<String> = if cjk_chars.len() >= 3 {
        cjk_chars
            .windows(3)
            .map(|w| w.iter().collect::<String>())
            .collect()
    } else if cjk_chars.len() >= 2 {
        cjk_chars
            .windows(2)
            .map(|w| w.iter().collect::<String>())
            .collect()
    } else {
        cjk_chars.iter().map(|c| c.to_string()).collect()
    };

    // 组合英文词 + CJK n-grams
    let mut parts = Vec::new();
    if !english_words.is_empty() {
        parts.push(english_words.join(" "));
    }
    for token in &cjk_tokens {
        parts.push(format!("\"{}\"", token));
    }

    if parts.is_empty() {
        input.to_string()
    } else {
        parts.join(" OR ")
    }
}

/// 跨层记忆去重与分数合并
struct CrossLayerDedup {
    seen: HashMap<String, (f32, Vec<&'static str>)>, // node_id -> (best_score, source_layers)
}

impl CrossLayerDedup {
    fn new() -> Self {
        Self {
            seen: HashMap::new(),
        }
    }

    /// 返回 true 如果该节点应该被包含（首次出现或分数更高）
    fn should_include(&mut self, node_id: &str, score: f32, layer: &'static str) -> bool {
        match self.seen.entry(node_id.to_string()) {
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert((score, vec![layer]));
                true
            }
            std::collections::hash_map::Entry::Occupied(mut e) => {
                let (best, layers) = e.get_mut();
                layers.push(layer);
                if score > *best {
                    *best = score;
                    true // 更高分数，允许替换
                } else {
                    false // 已有更好的版本
                }
            }
        }
    }

    /// 获取多源加成分数
    fn multi_source_bonus(&self, node_id: &str) -> f32 {
        self.seen
            .get(node_id)
            .map(|(_, layers)| {
                if layers.len() > 1 {
                    0.1 * (layers.len() - 1) as f32
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0)
    }
}

/// 令牌预算智能分配
struct TokenBudgetAllocation {
    boot: usize,
    triggered: usize,
    relevant: usize,
    expanded: usize,
    recent: usize,
}

impl TokenBudgetAllocation {
    fn from_total(total: usize) -> Self {
        Self {
            boot: (total as f32 * 0.30) as usize,
            triggered: (total as f32 * 0.20) as usize,
            relevant: (total as f32 * 0.25) as usize,
            expanded: (total as f32 * 0.15) as usize,
            recent: (total as f32 * 0.10) as usize,
        }
    }

    /// 流转未用配额到下一层
    fn cascade_unused(&mut self, layer: &str, used: usize) {
        let surplus = match layer {
            "boot" => self.boot.saturating_sub(used),
            "triggered" => self.triggered.saturating_sub(used),
            "relevant" => self.relevant.saturating_sub(used),
            "expanded" => self.expanded.saturating_sub(used),
            _ => return,
        };
        if surplus == 0 {
            return;
        }
        match layer {
            "boot" => {
                let per = surplus / 2;
                self.triggered += per;
                self.relevant += surplus - per;
            }
            "triggered" => {
                let per = surplus / 2;
                self.relevant += per;
                self.expanded += surplus - per;
            }
            "relevant" => {
                let per = surplus / 2;
                self.expanded += per;
                self.recent += surplus - per;
            }
            "expanded" => {
                self.recent += surplus;
            }
            _ => {}
        }
    }

    /// 获取指定层的当前预算
    fn budget_for(&self, layer: &str) -> usize {
        match layer {
            "boot" => self.boot,
            "triggered" => self.triggered,
            "relevant" => self.relevant,
            "expanded" => self.expanded,
            "recent" => self.recent,
            _ => 0,
        }
    }
}

/// Simple tokenizer that handles both CJK and ASCII text.
/// - CJK characters are emitted individually.
/// - ASCII/Latin runs are split on whitespace and common punctuation.
fn tokenize_input(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut ascii_buf = String::new();

    for ch in input.chars() {
        if is_cjk(ch) {
            // Flush any pending ASCII buffer
            if !ascii_buf.is_empty() {
                flush_ascii(&mut ascii_buf, &mut tokens);
            }
            tokens.push(ch.to_string());
        } else if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            ascii_buf.push(ch);
        } else {
            // Delimiter (space, punctuation, etc.)
            if !ascii_buf.is_empty() {
                flush_ascii(&mut ascii_buf, &mut tokens);
            }
        }
    }
    if !ascii_buf.is_empty() {
        flush_ascii(&mut ascii_buf, &mut tokens);
    }

    // Deduplicate while preserving order
    let mut seen = HashSet::new();
    tokens.retain(|t| {
        let lower = t.to_lowercase();
        if lower.len() < 2 && !is_cjk(lower.chars().next().unwrap_or(' ')) {
            return false; // skip single ASCII chars
        }
        seen.insert(lower)
    });

    tokens
}

fn flush_ascii(buf: &mut String, tokens: &mut Vec<String>) {
    let word = std::mem::take(buf);
    if !word.is_empty() {
        tokens.push(word);
    }
}

fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}' // CJK Unified Ideographs Extension A
        | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
        | '\u{3000}'..='\u{303F}' // CJK Symbols and Punctuation
    )
}

fn truncate_content(content: &str, max_len: usize) -> String {
    if content.chars().count() <= max_len {
        content.to_string()
    } else {
        let truncated: String = content.chars().take(max_len).collect();
        format!("{}...", truncated)
    }
}

fn capitalize_kind(kind: &MemoryNodeKind) -> &'static str {
    match kind {
        MemoryNodeKind::Boot => "Boot",
        MemoryNodeKind::Identity => "Identity",
        MemoryNodeKind::Value => "Value",
        MemoryNodeKind::UserProfile => "UserProfile",
        MemoryNodeKind::Directive => "Directive",
        MemoryNodeKind::Curated => "Curated",
        MemoryNodeKind::Episode => "Episode",
        MemoryNodeKind::Procedure => "Procedure",
        MemoryNodeKind::Reference => "Reference",
        MemoryNodeKind::EntityPage => "EntityPage",
    }
}

/// Collect node IDs of every learned skill that `format_recall_for_prompt`
/// would render into the "Learned Skills" section. Must mirror the same
/// filter (Procedure + skill_type=='learned' + enabled).
///
/// Used by `MemoryRecallEngine::record_used_skills` to bump usage_count.
fn collect_emitted_skill_ids(plan: &MemoryRecallPlan) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let layers: Vec<&MemoryRecallCandidate> = plan
        .boot
        .iter()
        .chain(plan.triggered.iter())
        .chain(plan.relevant.iter())
        .chain(plan.expanded.iter())
        .collect();
    for c in layers {
        if c.kind != MemoryNodeKind::Procedure {
            continue;
        }
        let Some(ref meta) = c.metadata else { continue };
        let enabled = meta.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
        let skill_type = meta.get("skill_type").and_then(|v| v.as_str()).unwrap_or("");
        if !(enabled && skill_type == "learned") {
            continue;
        }
        if seen.insert(c.node_id.clone()) {
            ids.push(c.node_id.clone());
        }
    }
    ids
}

#[cfg(test)]
mod recall_helpers_tests {
    use super::*;

    fn skill_candidate(node_id: &str, enabled: bool, learned: bool) -> MemoryRecallCandidate {
        MemoryRecallCandidate {
            node_id: node_id.into(),
            title: "t".into(),
            content: "c".into(),
            kind: MemoryNodeKind::Procedure,
            source: "boot".into(),
            reason: "r".into(),
            score: None,
            fts_rank: None,
            vector_rank: None,
            matched_keywords: vec![],
            metadata: Some(serde_json::json!({
                "enabled": enabled,
                "skill_type": if learned { "learned" } else { "static" },
            })),
        }
    }

    fn empty_plan() -> MemoryRecallPlan {
        MemoryRecallPlan {
            boot: vec![],
            triggered: vec![],
            relevant: vec![],
            expanded: vec![],
            recent: vec![],
        }
    }

    #[test]
    fn collect_emitted_skips_disabled_and_non_learned() {
        let mut plan = empty_plan();
        plan.boot.push(skill_candidate("a", true, true));     // included
        plan.boot.push(skill_candidate("b", false, true));    // disabled, skip
        plan.boot.push(skill_candidate("c", true, false));    // static, skip
        plan.triggered.push(skill_candidate("a", true, true)); // dup of a, skip
        plan.relevant.push(skill_candidate("d", true, true)); // included
        let ids = collect_emitted_skill_ids(&plan);
        assert_eq!(ids, vec!["a".to_string(), "d".to_string()]);
    }

    #[test]
    fn collect_emitted_returns_empty_when_no_learned() {
        let mut plan = empty_plan();
        plan.boot.push(skill_candidate("a", true, false));
        let ids = collect_emitted_skill_ids(&plan);
        assert!(ids.is_empty());
    }
}

// ─── Phase 5 boost tests ────────────────────────────────────────────────

#[cfg(test)]
mod phase5_boost_tests {
    use super::*;
    use crate::memory_graph::store::MemoryGraphStore;
    use rusqlite::{params, Connection};
    use std::sync::Mutex;

    fn fresh_store() -> Arc<MemoryGraphStore> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        conn.execute_batch(crate::db::migrations::V35_MEMORY_OS_PHASE_1).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        Arc::new(MemoryGraphStore::new(Arc::new(Mutex::new(conn))))
    }

    fn engine_with_config(
        store: Arc<MemoryGraphStore>,
        entity_page_boost: f32,
        backlink_boost_weight: f32,
    ) -> MemoryRecallEngine {
        let mut cfg = MemoryRecallConfig::default();
        cfg.entity_page_boost = entity_page_boost;
        cfg.backlink_boost_weight = backlink_boost_weight;
        MemoryRecallEngine::new(store, None, cfg)
    }

    fn insert_node(store: &MemoryGraphStore, id: &str, kind: &str, title: &str) {
        let conn = store.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO memory_nodes \
             (id, space_id, kind, title, created_at, updated_at) \
             VALUES (?1, 'default', ?2, ?3, ?4, ?4)",
            params![id, kind, title, now],
        )
        .unwrap();
    }

    fn insert_edge(store: &MemoryGraphStore, edge_id: &str, parent: &str, child: &str) {
        let conn = store.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO memory_edges \
             (id, space_id, parent_node_id, child_node_id, relation_kind, visibility, priority, created_at, updated_at) \
             VALUES (?1, 'default', ?2, ?3, 'relates_to', 'private', 0, ?4, ?4)",
            params![edge_id, parent, child, now],
        )
        .unwrap();
    }

    #[test]
    fn fetch_boost_signals_returns_kind_and_backlinks() {
        let store = fresh_store();
        insert_node(&store, "page", "entity_page", "Page");
        insert_node(&store, "ref1", "episode", "Ref 1");
        insert_node(&store, "ref2", "episode", "Ref 2");
        // Two episodes both reference page → page has backlink_count=2.
        insert_edge(&store, "e1", "ref1", "page");
        insert_edge(&store, "e2", "ref2", "page");

        let engine = engine_with_config(store, 1.5, 0.3);
        let ids = vec!["page".to_string(), "ref1".to_string()];
        let signals = engine.fetch_boost_signals(&ids).unwrap();
        assert_eq!(signals.len(), 2);
        let (page_kind, page_backlinks) = signals.get("page").unwrap();
        assert_eq!(*page_kind, MemoryNodeKind::EntityPage);
        assert_eq!(*page_backlinks, 2);
        let (ref_kind, ref_backlinks) = signals.get("ref1").unwrap();
        assert_eq!(*ref_kind, MemoryNodeKind::Episode);
        assert_eq!(*ref_backlinks, 0);
    }

    #[test]
    fn fetch_boost_signals_empty_input_returns_empty_map() {
        let store = fresh_store();
        let engine = engine_with_config(store, 1.5, 0.3);
        let signals = engine.fetch_boost_signals(&[]).unwrap();
        assert!(signals.is_empty());
    }

    #[test]
    fn fetch_boost_signals_missing_ids_excluded_from_map() {
        let store = fresh_store();
        insert_node(&store, "real", "entity_page", "Real");
        let engine = engine_with_config(store, 1.5, 0.3);
        let signals = engine
            .fetch_boost_signals(&["real".into(), "ghost".into()])
            .unwrap();
        assert_eq!(signals.len(), 1);
        assert!(signals.contains_key("real"));
        assert!(!signals.contains_key("ghost"));
    }

    #[test]
    fn boost_config_defaults_are_neutral() {
        // Default config should leave existing recall behaviour unchanged
        // (the upgrade is a no-op until the user dials up the knobs).
        let cfg = MemoryRecallConfig::default();
        assert_eq!(cfg.entity_page_boost, 1.0);
        assert_eq!(cfg.backlink_boost_weight, 0.0);
    }

    #[test]
    fn dto_round_trip_preserves_phase5_knobs() {
        let mut cfg = MemoryRecallConfig::default();
        cfg.entity_page_boost = 1.5;
        cfg.backlink_boost_weight = 0.3;
        let dto: MemoryRecallConfigDto = cfg.clone().into();
        assert_eq!(dto.entity_page_boost, Some(1.5));
        assert_eq!(dto.backlink_boost_weight, Some(0.3));
        let restored: MemoryRecallConfig = dto.into();
        assert_eq!(restored.entity_page_boost, 1.5);
        assert_eq!(restored.backlink_boost_weight, 0.3);
    }

    #[test]
    fn dto_round_trip_preserves_memu_retrieve_timeout() {
        let mut cfg = MemoryRecallConfig::default();
        // Default is unbounded (legacy behavior — no per-retrieve timeout).
        assert_eq!(cfg.memu_retrieve_timeout_ms, None);
        cfg.memu_retrieve_timeout_ms = Some(300);
        let dto: MemoryRecallConfigDto = cfg.clone().into();
        assert_eq!(dto.memu_retrieve_timeout_ms, Some(300));
        let restored: MemoryRecallConfig = dto.into();
        assert_eq!(restored.memu_retrieve_timeout_ms, Some(300));
    }

    #[test]
    fn dto_without_memu_timeout_defaults_to_none() {
        // Forward-compat: a settings blob that predates the field stays
        // unbounded (legacy callers are untouched).
        let json = r#"{"bootLimit": 8}"#;
        let dto: MemoryRecallConfigDto = serde_json::from_str(json).unwrap();
        assert!(dto.memu_retrieve_timeout_ms.is_none());
        let cfg: MemoryRecallConfig = dto.into();
        assert_eq!(cfg.memu_retrieve_timeout_ms, None);
    }

    #[test]
    fn dto_partial_update_keeps_defaults_for_unspecified_knobs() {
        // Forward-compat: a config DTO that doesn't mention the Phase 5
        // knobs must keep the neutral defaults (not flip them to None or
        // some sentinel).
        let json = r#"{"bootLimit": 8}"#;
        let dto: MemoryRecallConfigDto = serde_json::from_str(json).unwrap();
        assert!(dto.entity_page_boost.is_none());
        let cfg: MemoryRecallConfig = dto.into();
        assert_eq!(cfg.entity_page_boost, 1.0);
        assert_eq!(cfg.backlink_boost_weight, 0.0);
    }
}
