//! ProactiveService — 24/7 主动代理服务
//!
//! 实现 memUBot 的主动轮询能力：
//! - 定时轮询（默认 30 秒间隔）
//! - 上下文滑动窗口监控（订阅 InfraService 消息总线）
//! - 自主 agent loop 执行（检测到新上下文时触发）
//! - 用户确认机制（破坏性操作前等待用户输入）
//!
//! ## 架构
//! ```text
//! InfraService ──subscribe──▶ context_listener (tokio task)
//!                                │
//!                                ▼
//!                         context_messages (VecDeque, 滑动窗口)
//!                                │
//!                                ▼
//! tick_loop (tokio task) ──检测 has_new_context──▶ tick_inner()
//!                                                     │
//!                                                     ▼
//!                                               agent loop (placeholder)
//!                                                     │
//!                                                     ▼
//!                                           ProactiveStorage (持久化)
//! ```

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use std::path::PathBuf;

use rusqlite::Connection;

use async_trait::async_trait;

/// Extract the content of the first `<summary>…</summary>` XML tag from a string.
/// Falls back to the first 200 characters if no tag is found.
fn extract_summary_text(response: &str) -> String {
    if let Some(start) = response.find("<summary>") {
        let after = &response[start + "<summary>".len()..];
        if let Some(end) = after.find("</summary>") {
            return after[..end].trim().to_string();
        }
    }
    // No <summary> tag — return first 200 chars stripped of XML tags
    let stripped: String = {
        let mut out = String::new();
        let mut in_tag = false;
        for ch in response.chars().take(400) {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                c if !in_tag => out.push(c),
                _ => {}
            }
        }
        out.trim().chars().take(200).collect()
    };
    stripped
}
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use crate::agent::types::ChatMessage;
use crate::infra::{InfraEventType, InfraService};
use crate::llm::provider::CompletionConfig;
use tauri::Emitter;
use crate::memu::client::MemUClient;
use crate::memubot_config::ProactiveConfig;
use crate::memory_graph::store::MemoryGraphStore;
use crate::providers::service::ProviderService;
use crate::services::{ManagedService, ServiceHealth, ServiceStatus};

use super::conversation_bridge::ConversationBridge;
use super::execution_log::ExecutionLogCollector;
use super::hybrid_search::{HybridSearchEngine, HybridSearchRequest};
use super::multimodal::MultimodalQueue;

/// Build a <system_info> block with current date/time for authoritative time context
/// in proactive scenario LLM calls. Mirrors `ChatDelegate::build_system_time_block()`.
fn build_proactive_time_block() -> String {
    use chrono::{Datelike, Local, Timelike};
    let now = Local::now();
    let weekday = match now.weekday() {
        chrono::Weekday::Mon => "周一",
        chrono::Weekday::Tue => "周二",
        chrono::Weekday::Wed => "周三",
        chrono::Weekday::Thu => "周四",
        chrono::Weekday::Fri => "周五",
        chrono::Weekday::Sat => "周六",
        chrono::Weekday::Sun => "周日",
    };
    let time = format!(
        "{}年{}月{}日 {} {:02}:{:02}",
        now.year(),
        now.month(),
        now.day(),
        weekday,
        now.hour(),
        now.minute(),
    );
    format!(
        "<system_info>\n当前时间: {}\n注意: 以上时间由系统提供，你不需要使用工具（如 bash date）获取时间，直接使用此信息回答即可。\n</system_info>",
        time
    )
}

use super::scenarios::{ScenarioContext, ScenarioManager, SessionContextWindow};
use super::storage::ProactiveStorage;
use super::failure_memory::FailureMemoryManager;
use super::personality_model::PersonalityModel;
use super::preference_extractor::PreferenceExtractor;
use super::proactive_recall::ProactiveRecallService;
use super::task_memory::{TaskMemoryManager, TaskRecord, TaskStatus, TaskType};
use super::tool_memory::{ToolUsageMemoryManager, ToolUsageRecord};
use super::types::*;
use crate::agent::gep::types::{Capsule, BlastRadius, CapsuleOutcome, EnvFingerprint, EvolutionEvent, Gene, GeneCandidate, LearningCard, LearningCardType, OutcomeStatus, StrategyHint};
use crate::agent::gep::repository::GeneRepository;
use crate::agent::gep::lifecycle::GeneLifecycleManager;
use crate::agent::gep::distillation;
use crate::memubot_config::GeneEvolutionConfig;

/// 上下文滑动窗口最大容量（每个 session 保留最近 N 条消息）
const CONTEXT_WINDOW_SIZE: usize = 20;

/// 最多保留的 session 窗口数（超出时淘汰最久未活跃的）
const MAX_SESSION_WINDOWS: usize = 10;

/// Memory OS Phase 5 helper — start of today (UTC) in epoch ms.
/// Used by the tick-side lint scan to sum today's already-spent
/// `memory_lint:*` token cost from `cost_records` and stay under the
/// daily budget.
fn today_start_ms_utc() -> i64 {
    use chrono::{Datelike, TimeZone, Utc};
    let now = Utc::now();
    Utc.with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

// ─── Memory OS runtime config ─────────────────────────────────────────
//
// Phase 3, 4, and 5 each added new positional parameters to
// `ProactiveService::new` — first one bool, then another bool, then a
// (bool, u32, Arc<dyn …>) triple. By Phase 5 the signature had grown
// 5 trailing positional args and Phase 6+ would extend it further.
//
// This struct bundles every Memory OS knob that the proactive runtime
// needs into a single Clone container, so the constructor signature
// stops growing and future phases just add another field with a
// sensible default. The struct is cloned into `ProactiveStateRefs`
// once per spawned tick task — cheap because the bools are Copy and
// the Arc<dyn LintAnalyzer> is a refcount bump.

/// Runtime-side Memory OS feature flags + analyzer trait objects
/// consumed by ProactiveService's tick loop. Constructed once at
/// AppState bootstrap from `MemubotConfig::memory_os` plus the
/// AppState-owned analyzer trait objects, then handed to
/// `ProactiveService::new` as a single argument.
///
/// **Forward compatibility**: when a Phase 6+ feature needs a new
/// proactive-runtime knob, add a field here with a sensible default
/// in the `Default` impl. The constructor signature does NOT change.
#[derive(Clone)]
pub struct MemoryOsRuntimeConfig {
    /// Phase 3 — gates the periodic wiki_artifacts(kind="index") regen.
    pub wiki_view_enabled: bool,
    /// Phase 4 — gates the periodic memory_health scenario.
    pub memory_health_enabled: bool,
    /// Phase 5 — gates the periodic memory_lint scenario.
    pub memory_lint_enabled: bool,
    /// Phase 5 — daily token cap; lint orchestrator bails out before
    /// consuming a candidate that would push today's
    /// `cost_records WHERE model LIKE 'memory_lint%'` past this.
    pub memory_lint_daily_token_budget: u32,
    /// Phase 5 — LLM seam for the memory_lint scenario. Phase 5 ships
    /// `memory_lint::StubAnalyzer` as the default; a follow-up swap to
    /// a real Anthropic/OpenAI client is a single AppState field
    /// change — no proactive code path moves.
    pub lint_analyzer: Arc<dyn crate::proactive::scenarios::memory_lint::LintAnalyzer>,
    /// Phase 6.1 — gates the periodic tier_escalator scenario
    /// (mention_count → enrichment_tier transitions). Zero LLM cost.
    pub tier_escalator_enabled: bool,
    /// Phase 6.1 — daily upgrade cap. Downgrades are uncapped because
    /// they SAVE token spend on the next synthesis pass.
    pub tier_escalator_daily_cap: u32,
    /// L3 §4.12.1 RETAINED — gates the periodic Importance-Aware
    /// Decay batch. Zero LLM cost. Runs every 360 ticks (~3h).
    pub importance_decay_enabled: bool,
    /// L3 §4.12.1 RETAINED — per-batch cap on node count. Bounds
    /// per-tick work so the service stays responsive on slow disks.
    pub importance_decay_batch_size: u32,
    /// L3 §4.12.4 R1 — gates the periodic Concept Drift Detection scan.
    pub drift_detection_enabled: bool,
    /// L3 §4.12.4 R1 — per-scan cap on candidate EntityPages.
    pub drift_detection_batch_size: u32,
    /// Sprint 1 — gates the openhuman-style stability_detector +
    /// PROFILE.md rebuild. Default ON; rebuild itself is zero cost
    /// when the candidate buffer is empty.
    pub learning_enabled: bool,
    /// Sprint 1 — handle to the LearningScheduler that runs every
    /// 60 ticks (~30 min). Built at AppState bootstrap with the
    /// shared FacetStore + Buffer references.
    pub learning_scheduler: Option<Arc<crate::learning::scheduler::LearningScheduler>>,
    /// Sprint 1 — shared FacetCache the prompt builder reads
    /// `## User Profile (Learned)` from. Updated after every
    /// rebuild_now in the tick block.
    pub facet_cache: Option<Arc<crate::learning::cache::FacetCache>>,
    /// Sprint 2.1a — absolute disk path of PROFILE.md. When `Some`,
    /// the tick block writes the rendered profile to this path after
    /// every stability rebuild so the user has a human-readable
    /// snapshot they can inspect/diff/version-control. `None` skips
    /// the disk write (covers the no-Documents-dir edge case).
    /// Resolved at AppState bootstrap; default
    /// `~/Documents/workground/brain/PROFILE.md`.
    pub profile_md_path: Option<std::path::PathBuf>,
    /// Bundle 26-B — see
    /// `MemubotConfig::memory_os::skill_prune_min_unused_days`. Read
    /// by the `% 240` prune branch in `tick_inner`. Snapshotted at
    /// proactive-service start; `set_skill_prune_min_unused_days`
    /// triggers a silent proactive restart so the new value takes
    /// effect on the next tick rather than requiring an app restart.
    pub skill_prune_min_unused_days: u32,
    /// Bundle 26-D — see
    /// `MemubotConfig::memory_os::skill_promote_min_returned_count`.
    /// Read by the `% 60` promote branch in `tick_inner`.
    /// Snapshotted at proactive-service start; the matching set
    /// command does a silent restart.
    pub skill_promote_min_returned_count: u32,
}

impl MemoryOsRuntimeConfig {
    /// Build from a parsed `MemubotConfig` + the trait-object analyzer
    /// owned by AppState. Centralises the wiring so call sites stay
    /// short and future flags don't leak into main.rs.
    pub fn from_memubot_config(
        cfg: &crate::memubot_config::MemoryOsConfig,
        lint_analyzer: Arc<dyn crate::proactive::scenarios::memory_lint::LintAnalyzer>,
    ) -> Self {
        Self {
            wiki_view_enabled: cfg.wiki_view_enabled,
            memory_health_enabled: cfg.memory_health_enabled,
            memory_lint_enabled: cfg.memory_lint_enabled,
            memory_lint_daily_token_budget: cfg.memory_lint_daily_token_budget,
            lint_analyzer,
            tier_escalator_enabled: cfg.tier_escalator_enabled,
            tier_escalator_daily_cap: cfg.tier_escalator_daily_cap,
            importance_decay_enabled: cfg.importance_decay_enabled,
            importance_decay_batch_size: cfg.importance_decay_batch_size,
            drift_detection_enabled: cfg.drift_detection_enabled,
            drift_detection_batch_size: cfg.drift_detection_batch_size,
            learning_enabled: cfg.learning_enabled,
            learning_scheduler: None,  // wired in AppState bootstrap
            facet_cache: None,         // wired in AppState bootstrap
            profile_md_path: None,     // wired in AppState bootstrap
            skill_prune_min_unused_days: cfg.skill_prune_min_unused_days,
            skill_promote_min_returned_count: cfg.skill_promote_min_returned_count,
        }
    }

    /// Test-friendly default. Mirrors `MemoryOsConfig::default()`
    /// (every flag on, neutral budget) and installs the deterministic
    /// `StubAnalyzer` so unit tests don't need LLM credentials.
    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self {
            wiki_view_enabled: true,
            memory_health_enabled: true,
            memory_lint_enabled: true,
            memory_lint_daily_token_budget: 50_000,
            lint_analyzer: Arc::new(crate::proactive::scenarios::memory_lint::StubAnalyzer)
                as Arc<dyn crate::proactive::scenarios::memory_lint::LintAnalyzer>,
            tier_escalator_enabled: true,
            tier_escalator_daily_cap: 10,
            importance_decay_enabled: false,  // off in tests; tests opt-in
            importance_decay_batch_size: 100,
            drift_detection_enabled: false,  // off in tests; tests opt-in
            drift_detection_batch_size: 50,
            learning_enabled: false,  // off in tests by default — tests opt-in by setting handles
            learning_scheduler: None,
            facet_cache: None,
            profile_md_path: None,
            skill_prune_min_unused_days: 30,
            skill_promote_min_returned_count: 3,
        }
    }
}

impl Default for MemoryOsRuntimeConfig {
    fn default() -> Self {
        Self {
            wiki_view_enabled: true,
            memory_health_enabled: true,
            memory_lint_enabled: true,
            memory_lint_daily_token_budget: 50_000,
            lint_analyzer: Arc::new(crate::proactive::scenarios::memory_lint::StubAnalyzer)
                as Arc<dyn crate::proactive::scenarios::memory_lint::LintAnalyzer>,
            tier_escalator_enabled: true,
            tier_escalator_daily_cap: 10,
            importance_decay_enabled: true,
            importance_decay_batch_size: 100,
            drift_detection_enabled: true,
            drift_detection_batch_size: 50,
            learning_enabled: true,
            learning_scheduler: None,
            facet_cache: None,
            profile_md_path: None,
            skill_prune_min_unused_days: 30,
            skill_promote_min_returned_count: 3,
        }
    }
}

// ─── 状态引用结构体 ───────────────────────────────────────────────────

/// 辅助结构：持有所有需要跨 spawned task 共享的 Arc 引用
///
/// 由于 `ProactiveService` 本身不是 `Clone`（包含 JoinHandle），
/// 需要将纯数据部分的 Arc 引用打包成可 Clone 的结构体，
/// 传递给 `tokio::spawn` 的异步任务。
#[derive(Clone)]
struct ProactiveStateRefs {
    /// 轮询计数
    tick_count: Arc<AtomicU64>,
    /// 实际行动计数
    action_count: Arc<AtomicU64>,
    /// 无需行动计数
    no_message_count: Arc<AtomicU64>,
    /// 是否有新上下文（原子标记）
    has_new_context: Arc<AtomicBool>,
    /// 上次 tick 时间
    last_tick_at: Arc<RwLock<Option<String>>>,
    /// 上次行动时间
    last_action_at: Arc<RwLock<Option<String>>>,
    /// Per-session 上下文消息滑动窗口
    context_messages: Arc<RwLock<HashMap<String, SessionContextWindow>>>,
    /// 当前状态
    state: Arc<RwLock<ProactiveState>>,
    /// 消息持久化存储
    storage: Arc<ProactiveStorage>,
    /// 消息总线（用于发布主动消息事件）
    infra: Arc<InfraService>,
    /// 场景管理器
    scenario_manager: Arc<ScenarioManager>,
    /// 执行日志收集器
    execution_log_collector: Arc<ExecutionLogCollector>,
    /// 多模态输入队列
    multimodal_queue: Arc<MultimodalQueue>,
    /// Provider 服务（用于动态获取 LLM provider）
    provider_service: Arc<ProviderService>,
    /// memU 客户端（Python 不可用时为 None）
    memu_client: Option<Arc<MemUClient>>,
    /// Memory Graph 存储
    memory_graph_store: Arc<MemoryGraphStore>,
    /// Memory OS Phase 3/4/5 runtime knobs — bundled into one struct
    /// to stop the constructor signature from growing per phase. See
    /// `MemoryOsRuntimeConfig` for the layout + per-field semantics.
    memory_os: MemoryOsRuntimeConfig,
    /// Tauri AppHandle（用于发射 IPC 事件，测试时为 None）
    app_handle: Option<tauri::AppHandle>,
    /// 自上次触发以来的新消息数
    new_message_count: Arc<AtomicUsize>,
    /// 自上次触发以来的新执行次数
    new_execution_count: Arc<AtomicUsize>,
    /// 当前活跃的 Space ID（用于记忆召回）
    active_space_id: Arc<RwLock<String>>,
    /// 最近一次有 user/assistant 消息的 session_id（来自 InfraEvent.metadata
    /// 的 conversation_id 字段）。用于把 proactive-learning IPC 事件
    /// 标记到具体 session — 否则前端就只能在每个 session 都展示，造成噪声。
    last_active_session_id: Arc<RwLock<Option<String>>>,
    /// 任务记忆管理器（记录历史任务执行结果）
    task_memory_manager: Arc<TaskMemoryManager>,
    /// 工具使用记忆管理器（追踪工具调用模式和统计）
    tool_memory_manager: Arc<ToolUsageMemoryManager>,
    /// 五路混合检索引擎（向量+关键词+时间+图关系+文件）
    hybrid_search_engine: Arc<HybridSearchEngine>,
    /// 双轨会话桥接器（将传统 conversations 统一桥接到记忆系统）
    conversation_bridge: Arc<ConversationBridge>,
    /// 失败记忆管理器
    failure_memory_manager: Arc<FailureMemoryManager>,
    /// 偏好提取器
    preference_extractor: Arc<PreferenceExtractor>,
    /// 人格模型
    personality_model: Arc<PersonalityModel>,
    /// 主动召回服务
    proactive_recall_service: Arc<ProactiveRecallService>,
    /// Gene 候选池 — 收集 self_eval 产出的 LearningCard，等待蒸馏
    gene_candidate_pool: Arc<RwLock<VecDeque<GeneCandidate>>>,
    /// Gene 候选池有新候选的标记
    new_gene_candidates: Arc<AtomicBool>,
    /// 主数据库连接（用于查询 agent_messages/agent_turns/agent_sessions）
    db: Arc<Mutex<Connection>>,
    /// 上次人格画像更新时间
    last_personality_update: Arc<Mutex<Instant>>,
    /// GEP Gene 文件存储
    gene_repo: Arc<Mutex<GeneRepository>>,
    /// GEP Gene 生命周期管理器
    gene_lifecycle: Arc<Mutex<GeneLifecycleManager>>,
    /// Gene 进化配置
    gene_evolution_config: GeneEvolutionConfig,
    /// Bundle 22 — uClaw 数据根目录（典型 `~/.uclaw/`）。
    /// `skill_extraction` 抽到合格新 skill 后，除了存为 memory_graph
    /// `Procedure` 节点，还会同步落盘成
    /// `<data_dir>/skills/_auto_extracted/<slug>/SKILL.md`，让 disk-tier
    /// 的 `SkillsRegistry` 在下次启动时扫到，关闭"自演化 → 可见
    /// SKILL.md"循环。
    data_dir: std::path::PathBuf,
    /// Bundle 23 — same-session skill visibility. After Bundle 22
    /// writes a SKILL.md to disk, this handle lets the proactive
    /// service trigger `skills_registry.discover()` immediately so
    /// the newly persisted skill is discoverable by `skill_search`
    /// in the same agent loop — no restart required.
    skills_registry: std::sync::Arc<tokio::sync::RwLock<crate::skills::SkillsRegistry>>,
}

// ─── Gene Candidate Pool Helpers ───────────────────────────────────────────

/// Injection priority for gene_candidate_pool entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidatePriority {
    /// UserCorrection: push_front — highest distillation priority
    High,
    /// self_eval / tool_failure: push_back
    Normal,
}

/// Unified helper to inject a GeneCandidate into the pool with capacity control.
///
/// When the pool is full (≥20), the lowest-score entry is evicted regardless of
/// insertion priority, ensuring the pool always retains the most valuable candidates.
fn inject_candidate(pool: &mut VecDeque<GeneCandidate>, candidate: GeneCandidate, priority: CandidatePriority) {
    const MAX_CANDIDATES: usize = 20;
    if pool.len() >= MAX_CANDIDATES {
        // Evict the lowest-score entry (unified strategy across all injection sites)
        let mut min_idx = 0usize;
        let mut min_score = f64::MAX;
        for (i, c) in pool.iter().enumerate() {
            let s = c.score.unwrap_or(0.0);
            if s < min_score {
                min_score = s;
                min_idx = i;
            }
        }
        pool.remove(min_idx);
    }
    match priority {
        CandidatePriority::High => pool.push_front(candidate),
        CandidatePriority::Normal => pool.push_back(candidate),
    }
}

/// 24/7 主动代理服务
///
/// 通过 `ManagedService` trait 由 `ServiceManager` 统一管理。
/// 启动后会创建两个后台 tokio task：
/// 1. `context_listener` — 订阅 InfraService，维护上下文滑动窗口
/// 2. `tick_loop` — 按配置间隔轮询，检测新上下文后运行 agent loop
pub struct ProactiveService {
    /// 配置
    config: ProactiveConfig,
    /// 当前状态
    state: Arc<RwLock<ProactiveState>>,
    /// 是否正在运行（控制后台任务生命周期）
    is_running: Arc<AtomicBool>,

    /// Per-session 上下文消息滑动窗口
    context_messages: Arc<RwLock<HashMap<String, SessionContextWindow>>>,
    /// 是否有新的上下文消息（自上次 tick 以来）
    has_new_context: Arc<AtomicBool>,

    /// 是否正在等待用户输入（wait_user_confirm 工具）
    is_waiting_user_input: Arc<AtomicBool>,
    /// 用户输入的响应内容
    user_input_response: Arc<RwLock<Option<String>>>,

    /// 消息总线
    infra: Arc<InfraService>,
    /// 消息持久化存储
    storage: Arc<ProactiveStorage>,

    /// 场景管理器
    scenario_manager: Arc<ScenarioManager>,
    /// 执行日志收集器
    execution_log_collector: Arc<ExecutionLogCollector>,
    /// 多模态输入队列
    multimodal_queue: Arc<MultimodalQueue>,
    /// Provider 服务（动态获取 LLM 配置）
    provider_service: Arc<ProviderService>,
    /// memU 客户端（Python 不可用时为 None）
    memu_client: Option<Arc<MemUClient>>,
    /// Memory Graph 存储
    memory_graph_store: Arc<MemoryGraphStore>,
    /// Memory OS Phase 3/4/5 runtime knobs.
    memory_os: MemoryOsRuntimeConfig,
    /// Tauri AppHandle（用于发射 IPC 事件，测试时为 None）
    app_handle: Option<tauri::AppHandle>,

    /// 统计：轮询次数
    tick_count: Arc<AtomicU64>,
    /// 统计：实际行动次数
    action_count: Arc<AtomicU64>,
    /// 统计：无需行动次数
    no_message_count: Arc<AtomicU64>,
    /// 上次 tick 时间
    last_tick_at: Arc<RwLock<Option<String>>>,
    /// 上次行动时间
    last_action_at: Arc<RwLock<Option<String>>>,
    /// 自上次场景触发以来的新消息数
    new_message_count: Arc<AtomicUsize>,
    /// 自上次场景触发以来的新执行次数
    new_execution_count: Arc<AtomicUsize>,
    /// 当前活跃的 Space ID（用于记忆召回，默认 "default"）
    active_space_id: Arc<RwLock<String>>,
    /// 最近一次有消息流过来的 session_id（见 ProactiveStateRefs 同名字段）
    last_active_session_id: Arc<RwLock<Option<String>>>,
    /// 任务记忆管理器
    task_memory_manager: Arc<TaskMemoryManager>,
    /// 工具使用记忆管理器
    tool_memory_manager: Arc<ToolUsageMemoryManager>,
    /// 五路混合检索引擎
    hybrid_search_engine: Arc<HybridSearchEngine>,
    /// 双轨会话桥接器
    conversation_bridge: Arc<ConversationBridge>,
    /// 失败记忆管理器
    failure_memory_manager: Arc<FailureMemoryManager>,
    /// 偏好提取器
    preference_extractor: Arc<PreferenceExtractor>,
    /// 人格模型
    personality_model: Arc<PersonalityModel>,
    /// 主动召回服务
    proactive_recall_service: Arc<ProactiveRecallService>,

    /// 主数据库连接
    db: Arc<Mutex<Connection>>,

    /// 上次人格画像更新时间
    last_personality_update: Arc<Mutex<Instant>>,

    /// GEP Gene 文件存储
    gene_repo: Arc<Mutex<GeneRepository>>,

    /// GEP Gene 生命周期管理器
    gene_lifecycle: Arc<Mutex<GeneLifecycleManager>>,

    /// Gene 进化配置
    gene_evolution_config: GeneEvolutionConfig,

    /// Gene 候选池 — 收集 self_eval 产出的 LearningCard，等待蒸馏
    gene_candidate_pool: Arc<RwLock<VecDeque<GeneCandidate>>>,
    /// Gene 候选池有新候选的标记
    new_gene_candidates: Arc<AtomicBool>,

    /// Bundle 22 — see ProactiveStateRefs::data_dir.
    data_dir: std::path::PathBuf,
    /// Bundle 23 — see ProactiveStateRefs::skills_registry.
    skills_registry: std::sync::Arc<tokio::sync::RwLock<crate::skills::SkillsRegistry>>,
    /// 轮询循环任务句柄
    tick_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
    /// 上下文监听任务句柄
    listener_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
}

/// Self-eval 会话评估汇总（激活 KR4 数据回路）
#[derive(Debug, Clone)]
struct SessionEvalSummary {
    /// 分析窗口内的评估总数
    total_evals: usize,
    /// 平均得分
    avg_score: f64,
    /// 最近 10 条评估的平均得分（用于趋势检测）
    recent_avg_score: f64,
    /// 最近 10 条评估的条目数
    recent_count: usize,
    /// 总学习记录数
    total_learnings: usize,
    /// 最近一次评估的时间戳（毫秒）
    last_eval_at: Option<i64>,
    /// 是否存在得分下降趋势（recent_avg < overall_avg - 0.1）
    score_degrading: bool,
}

impl ProactiveService {
    /// 创建新的 ProactiveService 实例
    ///
    /// - `config`: 主动服务配置（来自 MemubotConfig）
    /// - `infra`: 消息总线（用于订阅对话事件、发布主动消息事件）
    /// - `storage`: 消息持久化存储
    /// - `memory_os`: Memory OS runtime knobs. Replaces five trailing positional
    ///   args (wiki_view_enabled / memory_health_enabled / memory_lint_enabled /
    ///   memory_lint_daily_token_budget / lint_analyzer) that accumulated over
    ///   Phase 3-5. Build with `MemoryOsRuntimeConfig::from_memubot_config(...)`
    ///   or `Default::default()`.
    pub fn new(
        config: ProactiveConfig,
        infra: Arc<InfraService>,
        storage: Arc<ProactiveStorage>,
        scenario_manager: Arc<ScenarioManager>,
        execution_log_collector: Arc<ExecutionLogCollector>,
        multimodal_queue: Arc<MultimodalQueue>,
        provider_service: Arc<ProviderService>,
        memu_client: Option<Arc<MemUClient>>,
        memory_graph_store: Arc<MemoryGraphStore>,
        app_handle: Option<tauri::AppHandle>,
        db: Arc<Mutex<Connection>>,
        gene_repo: Arc<Mutex<GeneRepository>>,
        gene_evolution_config: GeneEvolutionConfig,
        memory_os: MemoryOsRuntimeConfig,
        // Bundle 22 — root uClaw data dir (typically ~/.uclaw/).
        // skill_extraction lands persisted SKILL.md under
        // `<data_dir>/skills/_auto_extracted/`. Passing as an explicit
        // arg (rather than deriving from gene_repo's base_path) so the
        // semantics stay obvious at call sites.
        data_dir: std::path::PathBuf,
        // Bundle 23 — same-session skill visibility. Required to
        // rescan disk after auto-extraction lands a new SKILL.md.
        skills_registry: std::sync::Arc<tokio::sync::RwLock<crate::skills::SkillsRegistry>>,
    ) -> Self {
        let task_memory_manager = Arc::new(TaskMemoryManager::new(memory_graph_store.clone()));
        let tool_memory_manager = Arc::new(ToolUsageMemoryManager::new(memory_graph_store.clone()));
        let hybrid_search_engine = Arc::new(HybridSearchEngine::new(
            memory_graph_store.clone(),
        ));
        let conversation_bridge = Arc::new(ConversationBridge::new(memory_graph_store.clone()));
        let failure_memory_manager = Arc::new(FailureMemoryManager::new(memory_graph_store.clone()));
        let preference_extractor = Arc::new(PreferenceExtractor::new(memory_graph_store.clone()));
        let personality_model = Arc::new(PersonalityModel::new(memory_graph_store.clone()));
        let proactive_recall_service = Arc::new(ProactiveRecallService::new(
            memory_graph_store.clone(),
            task_memory_manager.clone(),
            tool_memory_manager.clone(),
            failure_memory_manager.clone(),
        ));

        // Extract GEP base path before gene_repo is moved into Self
        let gep_base_path = gene_repo
            .lock()
            .map_err(|e| anyhow::anyhow!("GeneRepository lock poisoned: {}", e))
            .unwrap()
            .base_path()
            .clone();

        Self {
            config,
            state: Arc::new(RwLock::new(ProactiveState::Stopped)),
            is_running: Arc::new(AtomicBool::new(false)),
            context_messages: Arc::new(RwLock::new(HashMap::new())),
            has_new_context: Arc::new(AtomicBool::new(false)),
            is_waiting_user_input: Arc::new(AtomicBool::new(false)),
            user_input_response: Arc::new(RwLock::new(None)),
            infra,
            storage,
            scenario_manager,
            execution_log_collector,
            multimodal_queue,
            provider_service,
            memu_client,
            memory_graph_store,
            memory_os,
            app_handle,
            tick_count: Arc::new(AtomicU64::new(0)),
            action_count: Arc::new(AtomicU64::new(0)),
            no_message_count: Arc::new(AtomicU64::new(0)),
            last_tick_at: Arc::new(RwLock::new(None)),
            last_action_at: Arc::new(RwLock::new(None)),
            new_message_count: Arc::new(AtomicUsize::new(0)),
            new_execution_count: Arc::new(AtomicUsize::new(0)),
            active_space_id: Arc::new(RwLock::new("default".to_string())),
            last_active_session_id: Arc::new(RwLock::new(None)),
            task_memory_manager,
            tool_memory_manager,
            hybrid_search_engine,
            conversation_bridge,
            failure_memory_manager,
            preference_extractor,
            personality_model,
            proactive_recall_service,
            db,
            last_personality_update: Arc::new(Mutex::new(Instant::now())),
            gene_repo,
            gene_lifecycle: Arc::new(Mutex::new(GeneLifecycleManager::new(gep_base_path))),
            gene_evolution_config,
            gene_candidate_pool: Arc::new(RwLock::new(VecDeque::new())),
            new_gene_candidates: Arc::new(AtomicBool::new(false)),
            data_dir,
            skills_registry,
            tick_handle: Arc::new(RwLock::new(None)),
            listener_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// 克隆所有 Arc 引用，打包成 ProactiveStateRefs
    ///
    /// 用于传递给 spawned 的 tokio task，避免直接 move self。
    fn clone_state_refs(&self) -> ProactiveStateRefs {
        ProactiveStateRefs {
            tick_count: self.tick_count.clone(),
            action_count: self.action_count.clone(),
            no_message_count: self.no_message_count.clone(),
            has_new_context: self.has_new_context.clone(),
            last_tick_at: self.last_tick_at.clone(),
            last_action_at: self.last_action_at.clone(),
            context_messages: self.context_messages.clone(),
            state: self.state.clone(),
            storage: self.storage.clone(),
            infra: self.infra.clone(),
            scenario_manager: self.scenario_manager.clone(),
            execution_log_collector: self.execution_log_collector.clone(),
            multimodal_queue: self.multimodal_queue.clone(),
            provider_service: self.provider_service.clone(),
            memu_client: self.memu_client.clone(),
            memory_graph_store: self.memory_graph_store.clone(),
            memory_os: self.memory_os.clone(),
            app_handle: self.app_handle.clone(),
            new_message_count: self.new_message_count.clone(),
            new_execution_count: self.new_execution_count.clone(),
            active_space_id: self.active_space_id.clone(),
            last_active_session_id: self.last_active_session_id.clone(),
            task_memory_manager: self.task_memory_manager.clone(),
            tool_memory_manager: self.tool_memory_manager.clone(),
            hybrid_search_engine: self.hybrid_search_engine.clone(),
            conversation_bridge: self.conversation_bridge.clone(),
            failure_memory_manager: self.failure_memory_manager.clone(),
            preference_extractor: self.preference_extractor.clone(),
            personality_model: self.personality_model.clone(),
            proactive_recall_service: self.proactive_recall_service.clone(),
            db: self.db.clone(),
            last_personality_update: self.last_personality_update.clone(),
            gene_repo: self.gene_repo.clone(),
            gene_lifecycle: self.gene_lifecycle.clone(),
            gene_evolution_config: self.gene_evolution_config.clone(),
            gene_candidate_pool: self.gene_candidate_pool.clone(),
            new_gene_candidates: self.new_gene_candidates.clone(),
            data_dir: self.data_dir.clone(),
            skills_registry: self.skills_registry.clone(),
        }
    }

    /// Get a reference to the GEP GeneRepository.
    pub fn gene_repository(&self) -> Arc<Mutex<GeneRepository>> {
        self.gene_repo.clone()
    }

    /// 启动上下文监听任务
    ///
    /// 订阅 InfraService 的消息总线，将 Incoming / Outgoing 消息
    /// 写入上下文滑动窗口，并设置 `has_new_context` 标记。
    async fn start_context_listener(&self) {
        let mut rx = self.infra.subscribe();
        let context = self.context_messages.clone();
        let has_new = self.has_new_context.clone();
        let is_running = self.is_running.clone();
        let new_msg_count = self.new_message_count.clone();
        let new_exec_count = self.new_execution_count.clone();
        let exec_log_collector = self.execution_log_collector.clone();
        let last_session = self.last_active_session_id.clone();
        let active_space_id = self.active_space_id.clone();
        let tool_memory = self.tool_memory_manager.clone();
        let gene_pool = self.gene_candidate_pool.clone();
        let new_gene_candidates_flag = self.new_gene_candidates.clone();

        let handle = tokio::spawn(async move {
            while is_running.load(Ordering::SeqCst) {
                match rx.recv().await {
                    Ok(event) => {
                        match event.event_type {
                            // 用户消息和 Bot 回复 → 维护上下文滑动窗口
                            InfraEventType::MessageIncoming | InfraEventType::MessageOutgoing => {
                                // Determine session key from metadata
                                let session_key = event.metadata
                                    .get("conversation_id")
                                    .or_else(|| event.metadata.get("session_id"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("default")
                                    .to_string();

                                let mut windows = context.write().await;
                                let window = windows
                                    .entry(session_key.clone())
                                    .or_insert_with(|| SessionContextWindow::new(
                                        session_key.clone(),
                                        CONTEXT_WINDOW_SIZE,
                                    ));
                                window.messages.push_back(event.message);
                                if window.messages.len() > CONTEXT_WINDOW_SIZE {
                                    window.messages.pop_front();
                                }
                                window.touch();

                                // Enforce max session windows (LRU eviction)
                                if windows.len() > MAX_SESSION_WINDOWS {
                                    let mut entries: Vec<_> = windows.iter().collect();
                                    entries.sort_by_key(|(_, w)| w.last_active_at);
                                    if let Some((oldest_key, _)) = entries.first() {
                                        let key = (*oldest_key).to_string();
                                        windows.remove(&key);
                                        tracing::debug!(
                                            "[ProactiveService] LRU evicted session window: {}",
                                            key
                                        );
                                    }
                                }

                                has_new.store(true, Ordering::SeqCst);
                                new_msg_count.fetch_add(1, Ordering::SeqCst);
                                // Track last-active session so the next proactive
                                // extraction tick can tag its IPC payload with
                                // the session that most likely sourced it.
                                *last_session.write().await = Some(session_key);
                            }
                            // 工具执行事件 → 记录到 ExecutionLogCollector
                            InfraEventType::ToolExecuted => {
                                let metadata = &event.metadata;
                                let tool_name = metadata.get("tool_name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                let success = metadata.get("success")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                let duration_ms = metadata.get("duration_ms")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                let tool_input_str = metadata.get("tool_input")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("{}");
                                let tool_output_str = &event.message.content;

                                let log = crate::proactive::scenarios::types::ExecutionLog {
                                    session_id: String::new(),
                                    iteration: 0,
                                    tool_name: tool_name.clone(),
                                    tool_input: serde_json::json!({ "summary": tool_input_str }),
                                    tool_output: serde_json::json!({ "summary": tool_output_str }),
                                    success,
                                    duration_ms,
                                    timestamp: chrono::Utc::now().timestamp(),
                                    context_summary: "tool execution".to_string(),
                                };
                                exec_log_collector.push(log).await;
                                new_exec_count.fetch_add(1, Ordering::SeqCst);
                                has_new.store(true, Ordering::SeqCst);

                                // 记录工具使用到 ToolUsageMemoryManager
                                let space_id = active_space_id.read().await.clone();
                                let tool_usage = ToolUsageRecord {
                                    tool_name: tool_name.clone(),
                                    success,
                                    duration_ms,
                                    output_size_bytes: Some(tool_output_str.len() as u64),
                                    parameters_fingerprint: Some(tool_input_str.to_string()),
                                    session_id: last_session.read().await.clone(),
                                    task_description: None,
                                };
                                let _ = tool_memory.record_tool_usage(&space_id, &tool_usage);

                                // P0-2: 工具失败聚合注入到 gene_candidate_pool
                                // When a tool fails, extract the error pattern and inject as a
                                // GeneCandidate so the GEP distillation pipeline can learn from it.
                                if !success {
                                    let error_summary: String = {
                                        let s = tool_output_str.trim();
                                        if s.len() > 200 {
                                            format!("{}...", &s[..200])
                                        } else {
                                            s.to_string()
                                        }
                                    };
                                    let session_id = last_session.read().await.clone().unwrap_or_default();

                                    // Only inject if this is a novel failure (simple dedup by error prefix)
                                    let dedup_key = format!("{}:{}", tool_name, &error_summary[..error_summary.len().min(80)]);
                                    let mut pool = gene_pool.write().await;
                                    let already_exists = pool.iter().any(|c| {
                                        c.content.contains(&dedup_key[..dedup_key.len().min(40)])
                                    });

                                    if !already_exists {
                                        let candidate = GeneCandidate {
                                            source: "tool_failure".to_string(),
                                            content: format!(
                                                "Tool '{}' failed: {} | Session: {}",
                                                tool_name, error_summary, session_id
                                            ),
                                            card_type: Some(LearningCardType::FailureLesson),
                                            score: Some(0.3), // Medium-low score = moderate distillation priority
                                            session_id: Some(session_id),
                                            reasoning: Some(format!(
                                                "Tool '{}' execution failed. Error pattern may indicate a systemic issue.",
                                                tool_name
                                            )),
                                            timestamp: chrono::Utc::now(),
                                        };

                                        inject_candidate(&mut pool, candidate, CandidatePriority::Normal);
                                        new_gene_candidates_flag.store(true, Ordering::SeqCst);
                                        has_new.store(true, Ordering::SeqCst);
                                        tracing::info!(
                                            tool_name = %tool_name,
                                            pool_size = pool.len(),
                                            "[ProactiveService] Tool failure injected into gene candidate pool"
                                        );
                                    }
                                }
                            }
                            // 工作区切换事件 → 更新 active_space_id
                            InfraEventType::WorkspaceSwitched => {
                                if let Some(new_id) = event.metadata
                                    .get("new_workspace_id")
                                    .and_then(|v| v.as_str())
                                {
                                    let old = active_space_id.read().await.clone();
                                    *active_space_id.write().await = new_id.to_string();
                                    tracing::info!(
                                        "[ProactiveService] 工作区切换: {} -> {}",
                                        old, new_id
                                    );
                                }
                            }
                            // Gene 学习事件 → 解析 LearningCard 推入候选池
                            InfraEventType::SkillLearned => {
                                if let Some(card_json) = event.metadata.get("learning_card") {
                                    let card_obj: serde_json::Value = match serde_json::from_value(card_json.clone()) {
                                        Ok(v) => v,
                                        Err(e) => {
                                            tracing::warn!("[ProactiveService] Failed to parse learning_card: {}", e);
                                            continue;
                                        }
                                    };

                                    let card_type = match card_obj.get("card_type").and_then(|v| v.as_str()).unwrap_or("noise") {
                                        "failure_lesson" => LearningCardType::FailureLesson,
                                        "success_pattern" => LearningCardType::SuccessPattern,
                                        "optimization_tip" => LearningCardType::OptimizationTip,
                                        _ => LearningCardType::Noise,
                                    };

                                    // Skip noise (self_eval already filtered, double-check)
                                    if card_type == LearningCardType::Noise {
                                        continue;
                                    }

                                    let strategy_hint: StrategyHint = card_obj
                                        .get("strategy_hint")
                                        .map(|v| serde_json::from_value(v.clone()).unwrap_or_default())
                                        .unwrap_or_default();

                                    let learning_card = LearningCard {
                                        raw: event.message.content.clone(),
                                        card_type,
                                        failure_signal: card_obj.get("failure_signal").and_then(|v| v.as_str()).map(String::from),
                                        tool_name: card_obj.get("tool_name").and_then(|v| v.as_str()).map(String::from),
                                        strategy_hint,
                                        files_touched: vec![],
                                        session_id: event.metadata
                                            .get("session_id")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("unknown")
                                            .to_string(),
                                        score: event.metadata
                                            .get("score")
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or(0.0) as f32,
                                        timestamp: event.timestamp,
                                    };

                                    let candidate = GeneCandidate {
                                        source: "self_eval".to_string(),
                                        content: learning_card.raw.clone(),
                                        card_type: Some(learning_card.card_type.clone()),
                                        score: Some(learning_card.score as f64),
                                        session_id: Some(learning_card.session_id.clone()),
                                        reasoning: learning_card.strategy_hint.reason.clone(),
                                        timestamp: chrono::Utc::now(),
                                    };

                                    let mut pool = gene_pool.write().await;
                                    inject_candidate(&mut pool, candidate, CandidatePriority::Normal);
                                    new_gene_candidates_flag.store(true, Ordering::SeqCst);
                                    has_new.store(true, Ordering::SeqCst);
                                    tracing::info!(
                                        "[ProactiveService] Gene candidate added, pool_size={}",
                                        pool.len()
                                    );
                                }
                            }
                            // P1-3c: 用户纠正事件 → 解析为高优先级 FailureLesson 注入候选池
                            InfraEventType::UserCorrection => {
                                let source = event.metadata
                                    .get("source")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown");
                                let session_id = event.metadata
                                    .get("session_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown");
                                let feedback = event.metadata
                                    .get("feedback")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&event.message.content);
                                let trigger_context = event.metadata
                                    .get("trigger_context")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");

                                // Build a LearningCard from user correction
                                let learning_card = LearningCard {
                                    raw: format!(
                                        "[UserCorrection:{}] {} — Context: {}",
                                        source, feedback, trigger_context
                                    ),
                                    card_type: LearningCardType::FailureLesson,
                                    failure_signal: Some(source.to_string()),
                                    tool_name: event.metadata
                                        .get("tool_name")
                                        .and_then(|v| v.as_str())
                                        .map(String::from),
                                    strategy_hint: StrategyHint {
                                        condition: Some(format!("User {} detected", source)),
                                        action: Some(feedback.to_string()),
                                        reason: Some(trigger_context.to_string()),
                                    },
                                    files_touched: vec![],
                                    session_id: session_id.to_string(),
                                    score: 0.1, // User corrections signal high failure confidence
                                    timestamp: event.timestamp,
                                };

                                let candidate = GeneCandidate {
                                    source: format!("user_correction:{}", source),
                                    content: learning_card.raw.clone(),
                                    card_type: Some(LearningCardType::FailureLesson),
                                    score: Some(0.1), // Low score = high priority for distillation
                                    session_id: Some(session_id.to_string()),
                                    reasoning: Some(format!(
                                        "User {} feedback captured: {}", source, feedback
                                    )),
                                    timestamp: chrono::Utc::now(),
                                };

                                // Inject into gene candidate pool with priority (push front for user corrections)
                                let mut pool = gene_pool.write().await;
                                inject_candidate(&mut pool, candidate, CandidatePriority::High);
                                new_gene_candidates_flag.store(true, Ordering::SeqCst);
                                has_new.store(true, Ordering::SeqCst);
                                tracing::info!(
                                    source = %source,
                                    session_id = %session_id,
                                    pool_size = pool.len(),
                                    "[ProactiveService] UserCorrection injected into gene candidate pool"
                                );
                            }
                            _ => {} // 忽略其他事件类型
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            "[ProactiveService] 消息接收落后 {} 条，部分上下文可能丢失",
                            n
                        );
                    }
                    Err(_) => {
                        tracing::info!("[ProactiveService] 消息总线已关闭，上下文监听退出");
                        break;
                    }
                }
            }
            tracing::debug!("[ProactiveService] 上下文监听任务已退出");
        });

        *self.listener_handle.write().await = Some(handle);
    }

    /// 启动轮询循环任务
    ///
    /// 按 `config.interval_ms` 间隔执行 tick，每次 tick 检查是否有新上下文，
    /// 有则运行 agent loop。
    async fn start_tick_loop(&self) {
        let interval = self.config.interval_ms;
        let is_running = self.is_running.clone();
        let refs = self.clone_state_refs();

        let handle = tokio::spawn(async move {
            tracing::info!(
                "[ProactiveService] 轮询循环启动，间隔: {}ms",
                interval
            );

            loop {
                // 先等待一个间隔周期
                tokio::time::sleep(tokio::time::Duration::from_millis(interval)).await;

                // 检查是否需要退出
                if !is_running.load(Ordering::SeqCst) {
                    break;
                }

                // 执行单次 tick
                if let Err(e) = Self::tick_inner(&refs).await {
                    tracing::error!("[ProactiveService] tick 执行错误: {}", e);
                }
            }

            tracing::debug!("[ProactiveService] 轮询循环任务已退出");
        });

        *self.tick_handle.write().await = Some(handle);
    }

    /// 单次轮询 tick
    ///
    /// 1. 更新 tick 计数和时间戳
    /// 2. 检查是否有新上下文（无则跳过）
    /// 3. 切换状态为 Thinking
    /// 4. 运行 agent loop（当前为 placeholder）
    /// 5. 处理 agent 返回结果
    /// 6. 切换状态回 Idle
    async fn tick_inner(refs: &ProactiveStateRefs) -> anyhow::Result<()> {
        // 递增 tick 计数 (also used by Phase 3/4/5 modulo-based dispatch
        // below; we fetch_add first so the first tick is index 1 not 0,
        // matching the "% N == 0" pattern used historically by this file).
        refs.tick_count.fetch_add(1, Ordering::SeqCst);
        *refs.last_tick_at.write().await = Some(chrono::Utc::now().to_rfc3339());

        // 每 20 个 tick（约 10 分钟，按默认 30s 间隔）将传统会话桥接到记忆系统
        // 解决 conversations 与 agent_sessions 双轨割裂导致的记忆流失问题
        if refs.tick_count.load(Ordering::SeqCst) % 20 == 0 {
            let space_id = refs.active_space_id.read().await.clone();
            match refs.conversation_bridge.bridge_incremental(&space_id, 50) {
                Ok(stats) => {
                    if stats.messages_enqueued > 0 {
                        tracing::info!(
                            enqueued = stats.messages_enqueued,
                            conversations = stats.conversations_with_new,
                            scanned = stats.conversations_scanned,
                            duration_ms = stats.duration_ms,
                            "[ProactiveService] ConversationBridge 增量同步完成"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "[ProactiveService] ConversationBridge 增量同步失败"
                    );
                }
            }
        }

        // 每 20 个 tick 运行 Gene 生命周期检查（退役、变异等）
        if refs.tick_count.load(Ordering::SeqCst) % 20 == 0 {
            if let Ok(mut lifecycle) = refs.gene_lifecycle.lock() {
                if let Ok(repo) = refs.gene_repo.lock() {
                    match lifecycle.check_all_genes(&repo, &refs.gene_evolution_config) {
                        Ok(report) => {
                            if report.stale_count > 0 || report.retired_count > 0 || report.mutations_performed > 0 {
                                tracing::info!(
                                    stale = report.stale_count,
                                    retired = report.retired_count,
                                    mutations = report.mutations_performed,
                                    "[ProactiveService] Gene lifecycle check completed"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!("[ProactiveService] Gene lifecycle check failed: {}", e);
                        }
                    }
                }
            }
        }

        // Memory OS Foundation Phase 4 — periodic health checks.
        // Every 60 ticks (~30 min @ default 30s interval) run the
        // seven structural integrity checks. Zero LLM, ~50ms wall time
        // on typical local DBs. Schedule is offset from Phase 3's wiki
        // index regen (every 10 ticks) so the two scans don't collide
        // on the SQLite lock more often than necessary — when both fire
        // on the same tick the wiki regen runs first by source order.
        if refs.memory_os.memory_health_enabled && refs.tick_count.load(Ordering::SeqCst) % 60 == 0 {
            let space_id = refs.active_space_id.read().await.clone();
            let store = refs.memory_graph_store.clone();
            let result = tokio::task::spawn_blocking(move || {
                let conn = store
                    .conn
                    .lock()
                    .map_err(|e| anyhow::anyhow!("DB lock poisoned: {}", e))?;
                crate::proactive::scenarios::memory_health::run_health_checks(&conn, &space_id)
                    .map_err(|e| anyhow::anyhow!("run_health_checks: {}", e))
            })
            .await;
            match result {
                Ok(Ok(outcome)) => {
                    if outcome.total_inserted > 0 {
                        tracing::info!(
                            inserted = outcome.total_inserted,
                            active = outcome.active_total,
                            duration_ms = outcome.duration_ms,
                            "[ProactiveService] memory_health: new findings detected"
                        );
                    } else {
                        tracing::debug!(
                            active = outcome.active_total,
                            duration_ms = outcome.duration_ms,
                            "[ProactiveService] memory_health: no new findings"
                        );
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "[ProactiveService] memory_health check failed");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "[ProactiveService] memory_health spawn_blocking failed");
                }
            }
        }

        // Memory OS Foundation Phase 5 — LLM lint scan.
        // Every 120 ticks (~60 min @ default 30s interval). The actual
        // cost cap is enforced *inside* run_lint_checks via the daily
        // token budget and today_spent computed from cost_records, so
        // even if the tick rate were doubled the cost ceiling holds.
        if refs.memory_os.memory_lint_enabled && refs.tick_count.load(Ordering::SeqCst) % 120 == 0 {
            let space_id = refs.active_space_id.read().await.clone();
            let store = refs.memory_graph_store.clone();
            let analyzer = refs.memory_os.lint_analyzer.clone();
            let budget = refs.memory_os.memory_lint_daily_token_budget;
            let db = refs.db.clone();
            tokio::spawn(async move {
                // Sum today's lint token spend on the blocking pool so we
                // never hold the rusqlite lock across the analyzer call.
                let today_start_ms = today_start_ms_utc();
                let today_spent = tokio::task::spawn_blocking(move || {
                    let c = match db.lock() {
                        Ok(c) => c,
                        Err(_) => return 0u32,
                    };
                    c.query_row(
                        "SELECT COALESCE(SUM(input_tokens + output_tokens), 0) \
                         FROM cost_records \
                         WHERE model LIKE 'memory_lint%' AND created_at >= ?1",
                        rusqlite::params![today_start_ms],
                        |r| r.get::<_, i64>(0),
                    )
                    .unwrap_or(0) as u32
                })
                .await
                .unwrap_or(0);

                let cfg = crate::proactive::scenarios::memory_lint::LintRunConfig {
                    daily_token_budget: budget,
                    ..Default::default()
                };
                match crate::proactive::scenarios::memory_lint::run_lint_checks(
                    store, analyzer, &space_id, &cfg, today_spent,
                )
                .await
                {
                    Ok(outcome) => {
                        if outcome.total_inserted > 0 {
                            tracing::info!(
                                inserted = outcome.total_inserted,
                                tokens = outcome.total_tokens,
                                analyzer = %outcome.analyzer_descriptor,
                                duration_ms = outcome.duration_ms,
                                "[ProactiveService] memory_lint: new findings"
                            );
                        } else if outcome.skipped_due_to_budget > 0 {
                            tracing::warn!(
                                skipped = outcome.skipped_due_to_budget,
                                today_spent,
                                budget,
                                "[ProactiveService] memory_lint: budget exhausted"
                            );
                        } else {
                            tracing::debug!(
                                "[ProactiveService] memory_lint: no new findings"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "[ProactiveService] memory_lint failed");
                    }
                }
            });
        }

        // Memory OS Foundation Phase 6.1 — tier escalator scan.
        // Every 240 ticks (~2h @ default 30s interval). Zero LLM, but
        // upgrades are capped at `tier_escalator_daily_cap` per UTC day
        // because each upgrade may eventually trigger a Phase 6.2
        // `EntitySynthesizer` LLM call. Downgrades are uncapped.
        // Schedule offset from health (60) / lint (120) / wiki (10)
        // helps avoid SQLite-lock collisions; gcd is 10, but with 240
        // we land on the same tick as health & lint at most once every
        // ~ 60min which is acceptable.
        if refs.memory_os.tier_escalator_enabled
            && refs.tick_count.load(Ordering::SeqCst) % 240 == 0
        {
            let space_id = refs.active_space_id.read().await.clone();
            let store = refs.memory_graph_store.clone();
            let db = refs.db.clone();
            let cap = refs.memory_os.tier_escalator_daily_cap;
            tokio::task::spawn_blocking(move || {
                let today_spent = crate::proactive::scenarios::tier_escalator::count_todays_upgrades(&db)
                    .unwrap_or(0);
                let cfg = crate::proactive::scenarios::tier_escalator::TierEscalatorConfig {
                    daily_upgrade_cap: cap,
                    ..Default::default()
                };
                let outcome = crate::proactive::scenarios::tier_escalator::run_tier_escalator(
                    store, db, &space_id, &cfg, today_spent,
                );
                match outcome {
                    Ok(o) => {
                        if o.upgraded > 0 || o.downgraded > 0 || o.daily_cap_hit {
                            tracing::info!(
                                upgraded = o.upgraded,
                                downgraded = o.downgraded,
                                skipped_due_to_cap = o.skipped_due_to_cap,
                                cap_hit = o.daily_cap_hit,
                                pages_scanned = o.pages_scanned,
                                "[ProactiveService] tier_escalator: ran",
                            );
                        } else {
                            tracing::debug!(
                                pages_scanned = o.pages_scanned,
                                "[ProactiveService] tier_escalator: no changes",
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "[ProactiveService] tier_escalator failed");
                    }
                }
            });
        }

        // L3 §4.12.1 RETAINED — Importance-Aware Decay batch.
        // Every 360 ticks (~3h @ 30s tick interval). Zero LLM cost.
        // Iterates up to `batch_size` nodes from
        // DEFAULT_BATCH_KINDS, ordered by NULL-last_computed_at then
        // oldest-last_computed_at; calls compute_importance + upserts
        // `memory_importance_scores`. Offset from tier_escalator (240)
        // and other 60/120 schedules so co-firing is rare.
        if refs.memory_os.importance_decay_enabled
            && refs.memory_os.importance_decay_batch_size > 0
            && refs.tick_count.load(Ordering::SeqCst) % 360 == 0
        {
            let store = refs.memory_graph_store.clone();
            let batch_size = refs.memory_os.importance_decay_batch_size as usize;
            let now_ms = chrono::Utc::now().timestamp_millis();
            tokio::task::spawn_blocking(move || {
                let conn = match store.conn.lock() {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(error = %e, "[ProactiveService] importance_decay: DB lock failed");
                        return;
                    }
                };
                match crate::memory_graph::importance_decay::batch_recompute_importance(
                    &conn,
                    crate::memory_graph::importance_decay::DEFAULT_BATCH_KINDS,
                    batch_size,
                    now_ms,
                ) {
                    Ok(outcome) => {
                        if outcome.recomputed > 0 || outcome.errored > 0 {
                            tracing::info!(
                                recomputed = outcome.recomputed,
                                errored = outcome.errored,
                                batch_size,
                                "[ProactiveService] importance_decay: batch done"
                            );
                        } else {
                            tracing::debug!(
                                "[ProactiveService] importance_decay: no eligible nodes"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "[ProactiveService] importance_decay: batch failed"
                        );
                    }
                }
            });
        }

        // L3 §4.12.4 R1 — Concept Drift Detection scan. Every 480 ticks
        // (~4h @ 30s tick interval). Zero LLM cost. Scans EntityPages
        // with multiple versions, computes content drift, records a
        // Bundle 26-B — skill-distillation prune. Every ~2 hours
        // (at default 30s tick), scan `_auto_extracted/` and move
        // skills that have been on disk > 7 days WITHOUT EVER being
        // returned by `skill_search` to `_archive/`. Safe to run
        // anytime — operates only on never-used skills, doesn't
        // touch in-flight extraction directories. v2 (Bundle 26-B2)
        // will add LLM-driven merge of similar skills; v1 is prune
        // only.
        if refs.tick_count.load(Ordering::SeqCst) % 240 == 0 {
            let data_dir = refs.data_dir.clone();
            // Bundle 26-B (settings exposure) — pull the stale-day
            // threshold from the runtime config snapshot rather than
            // hardcoding it. Default 30 days (corrected from the
            // inline `7.0` literal shipped in the original 26-B
            // commit — was a leftover debug value); user-tunable via
            // `set_skill_prune_min_unused_days`.
            let min_unused_days =
                refs.memory_os.skill_prune_min_unused_days as f64;
            tokio::task::spawn_blocking(move || {
                let now_ms = chrono::Utc::now().timestamp_millis();
                match crate::proactive::skill_distillation::run_prune_pass(
                    &data_dir, now_ms, min_unused_days,
                ) {
                    Ok(report) => {
                        if !report.archived.is_empty() || !report.errors.is_empty() {
                            tracing::info!(
                                scanned = report.scanned,
                                archived = report.archived.len(),
                                kept = report.skipped_kept,
                                errors = report.errors.len(),
                                "[Bundle 26-B] skill prune pass complete"
                            );
                        } else {
                            tracing::debug!(
                                scanned = report.scanned,
                                "[Bundle 26-B] skill prune: nothing to archive"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "[Bundle 26-B] skill prune pass failed"
                        );
                    }
                }
            });
        }

        // Bundle 26-D — Skill → Gene promotion. Every ~60 ticks
        // (~30 min at 30s tick), scan `_auto_extracted/` for skills
        // with `returned_count >= 3` and no `promoted_at`. Push each
        // as a GeneCandidate (source="skill_promotion") into the
        // existing gene_candidate_pool — the GeneEvolutionScenario
        // already in place will distill them into Genes on its next
        // trigger. After successful push, stamp `promoted_at` to
        // prevent re-promotion on every tick.
        //
        // v1 uses `returned_count >= 3` as the eligibility signal.
        // v2 (when Bundle 26-A2 wires record_outcome) will add
        // `success_rate >= 0.7 && observed_outcomes >= 3` to filter
        // out skills the LLM finds but consistently fails with.
        if refs.tick_count.load(Ordering::SeqCst) % 60 == 0 {
            let data_dir = refs.data_dir.clone();
            let gene_pool = refs.gene_candidate_pool.clone();
            let new_gene_flag = refs.new_gene_candidates.clone();
            let has_new = refs.has_new_context.clone();
            // Bundle 26-D (settings exposure) — pull the promotion
            // threshold from runtime config (default 3).
            let min_returned = refs.memory_os.skill_promote_min_returned_count;
            tokio::spawn(async move {
                let candidates = tokio::task::spawn_blocking(move || {
                    crate::proactive::skill_distillation::find_promotion_candidates(
                        &data_dir, min_returned, 2000,
                    )
                })
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default();

                if candidates.is_empty() {
                    return;
                }

                let mut pushed = 0;
                for cand in &candidates {
                    // Dedup against the pool by content prefix
                    // (same heuristic as the existing tool_failure
                    // injection path). Skip if a near-match is
                    // already pending distillation.
                    let dedup_key = cand
                        .skill_md_excerpt
                        .chars()
                        .take(40)
                        .collect::<String>();
                    {
                        let pool = gene_pool.read().await;
                        let exists = pool.iter().any(|c| c.content.contains(&dedup_key));
                        if exists {
                            continue;
                        }
                    }

                    let candidate = crate::agent::gep::types::GeneCandidate {
                        source: "skill_promotion".to_string(),
                        content: format!(
                            "[Bundle 26-D] Promoted skill {} (returned_count={}):\n\n{}",
                            cand.slug, cand.returned_count, cand.skill_md_excerpt
                        ),
                        card_type: Some(
                            crate::agent::gep::types::LearningCardType::SuccessPattern,
                        ),
                        score: Some(0.65), // moderate-high distillation priority
                        session_id: None,
                        reasoning: Some(format!(
                            "Skill returned by skill_search {} times — pattern is empirically useful, ready for promotion to a Gene (passive injection via system prompt).",
                            cand.returned_count
                        )),
                        timestamp: chrono::Utc::now(),
                    };

                    {
                        let mut pool = gene_pool.write().await;
                        pool.push_back(candidate);
                    }
                    pushed += 1;

                    // Mark promoted_at so we don't re-inject on the
                    // next tick. Best-effort: a failed stamp means
                    // we'll push again next time, which is wasteful
                    // but the dedup above absorbs it.
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    if let Err(e) =
                        crate::proactive::skill_distillation::mark_promoted(&cand.dir, now_ms)
                    {
                        tracing::warn!(
                            slug = %cand.slug,
                            error = %e,
                            "[Bundle 26-D] mark_promoted failed (will re-attempt next tick)"
                        );
                    }
                }

                if pushed > 0 {
                    new_gene_flag.store(true, Ordering::SeqCst);
                    has_new.store(true, Ordering::SeqCst);
                    tracing::info!(
                        candidates_found = candidates.len(),
                        pushed = pushed,
                        "[Bundle 26-D] promoted skills to gene_candidate_pool"
                    );
                }
            });
        }

        // `drift_events` row when drift crosses threshold. Offset from
        // importance_decay (360) so the two heavy memory_graph scans
        // rarely co-fire.
        if refs.memory_os.drift_detection_enabled
            && refs.memory_os.drift_detection_batch_size > 0
            && refs.tick_count.load(Ordering::SeqCst) % 480 == 0
        {
            let store = refs.memory_graph_store.clone();
            let batch_size = refs.memory_os.drift_detection_batch_size as usize;
            let now_ms = chrono::Utc::now().timestamp_millis();
            tokio::task::spawn_blocking(move || {
                let conn = match store.conn.lock() {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(error = %e, "[ProactiveService] drift_detection: DB lock failed");
                        return;
                    }
                };
                match crate::memory_graph::drift_detection::scan_and_record_drift(
                    &conn, batch_size, now_ms,
                ) {
                    Ok(outcome) => {
                        if outcome.flagged > 0 {
                            tracing::info!(
                                scanned = outcome.scanned,
                                flagged = outcome.flagged,
                                "[ProactiveService] drift_detection: scan done"
                            );
                        } else {
                            tracing::debug!(
                                scanned = outcome.scanned,
                                "[ProactiveService] drift_detection: no drift flagged"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "[ProactiveService] drift_detection: scan failed"
                        );
                    }
                }
            });
        }

        // Memory OS Sprint 1.10 — stability_detector rebuild.
        // Every 60 ticks (~30 min @ default 30s interval) — drain the
        // candidate buffer, score with stability_detector, write back
        // to user_profile_facets, refresh the in-memory FacetCache.
        // Zero LLM cost; runs on tokio::task::spawn_blocking because
        // FacetStore takes the rusqlite Mutex.
        //
        // Sprint 2.1a — after the cache refresh, render PROFILE.md from
        // the just-refreshed snapshots and atomically write it to
        // `profile_md_path`. The disk file is a human-readable mirror
        // of the cache: the user can `cat` it, version-control it, or
        // hand-edit the unmanaged prelude/postlude sections (managed
        // blocks get overwritten on the next rebuild). The whole disk
        // op is best-effort: failures log + leave the previous file
        // untouched (we never poison the cache on IO trouble).
        if refs.memory_os.learning_enabled && refs.tick_count.load(Ordering::SeqCst) % 60 == 0 {
            if let (Some(scheduler), Some(cache)) = (
                refs.memory_os.learning_scheduler.as_ref().cloned(),
                refs.memory_os.facet_cache.as_ref().cloned(),
            ) {
                let now_ms = chrono::Utc::now().timestamp_millis();
                let profile_path = refs.memory_os.profile_md_path.clone();
                let result = tokio::task::spawn_blocking(move || -> Result<(), anyhow::Error> {
                    let outcome = scheduler
                        .rebuild_now(now_ms)
                        .map_err(|e| anyhow::anyhow!("learning::rebuild_now: {}", e))?;
                    // Refresh the cache from the just-written facets so
                    // the next prompt build sees the new state without
                    // a second tick wait.
                    let store_handle = scheduler.store_handle();
                    let n = cache
                        .refresh_from(&store_handle, now_ms)
                        .map_err(|e| anyhow::anyhow!("FacetCache::refresh_from: {}", e))?;
                    tracing::debug!(
                        promoted_active = outcome.promoted_to_active,
                        promoted_provisional = outcome.promoted_to_provisional,
                        demoted = outcome.demoted_for_budget,
                        forgotten = outcome.forgotten,
                        total = outcome.total,
                        cache_size = n,
                        "[ProactiveService] learning: stability rebuild + cache refresh"
                    );

                    // Sprint 2.1a — render + write PROFILE.md from the
                    // freshly-refreshed cache. If the path isn't set
                    // (no Documents dir, or learning bootstrap couldn't
                    // resolve a brain root) we skip silently — the
                    // cache + system-prompt injection still work.
                    if let Some(path) = profile_path.as_ref() {
                        use std::collections::HashMap;
                        // Read the existing file so we preserve the
                        // user's prelude/postlude across overwrites.
                        // Missing file ⇒ empty contents (default
                        // header gets emitted by render).
                        let prev = match crate::memory_graph::profile_md::read(path) {
                            Ok(p) => p,
                            Err(e) => {
                                tracing::warn!(
                                    path = %path.display(),
                                    error = %e,
                                    "[ProactiveService] Sprint 2.1a: read failed, using empty contents"
                                );
                                crate::memory_graph::profile_md::ProfileMdContents::empty()
                            }
                        };
                        let mut active: HashMap<_, _> = HashMap::new();
                        for class in crate::memory_graph::profile_md::CLASS_RENDER_ORDER {
                            active.insert(*class, cache.active_by_class(*class));
                        }
                        let text = crate::memory_graph::profile_md::render(&prev, &active);
                        if let Err(e) = crate::memory_graph::profile_md::write(path, &text) {
                            tracing::warn!(
                                path = %path.display(),
                                error = %e,
                                "[ProactiveService] Sprint 2.1a: PROFILE.md write failed"
                            );
                        } else {
                            tracing::debug!(
                                path = %path.display(),
                                bytes = text.len(),
                                "[ProactiveService] Sprint 2.1a: PROFILE.md written"
                            );
                        }
                    }
                    Ok(())
                })
                .await;
                if let Err(e) = result {
                    tracing::warn!(error = %e, "[ProactiveService] learning rebuild spawn_blocking failed");
                } else if let Ok(Err(e)) = result {
                    tracing::warn!(error = %e, "[ProactiveService] learning rebuild errored (kept previous cache)");
                }
            }
        }

        // Memory OS Foundation Phase 3 — periodic wiki index regeneration.
        // Every 10 ticks (~5 min @ default 30s interval) we refresh
        // `wiki_artifacts(kind="index")` from the current EntityPage set.
        // SQL-only, never calls LLM; the overview is regenerated only on
        // explicit `memory_wiki_regenerate` IPC calls (see Task 3.3).
        if refs.memory_os.wiki_view_enabled && refs.tick_count.load(Ordering::SeqCst) % 10 == 0 {
            let space_id = refs.active_space_id.read().await.clone();
            let store = refs.memory_graph_store.clone();
            // Run on the blocking pool — rusqlite lock acquisition is
            // sync and we don't want to stall the tokio runtime even
            // for a sub-millisecond contention.
            let result = tokio::task::spawn_blocking(move || {
                let conn = store
                    .conn
                    .lock()
                    .map_err(|e| anyhow::anyhow!("DB lock poisoned: {}", e))?;
                crate::memory_graph::wiki_synth::regenerate_index(
                    &conn,
                    &space_id,
                    crate::memory_graph::wiki_synth::RegenerateTrigger::Tick,
                )
                .map_err(|e| anyhow::anyhow!("regenerate_index: {}", e))
            })
            .await;
            match result {
                Ok(Ok(outcome)) => {
                    tracing::debug!(
                        bytes = outcome.bytes_written,
                        "[ProactiveService] wiki index regenerated"
                    );
                }
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "[ProactiveService] wiki index regen failed");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "[ProactiveService] wiki index spawn_blocking failed");
                }
            }
        }

        // 每 30 次 tick 分析 session_evals 数据（激活 KR4 数据回路）
        if refs.tick_count.load(Ordering::SeqCst) % 30 == 0 {
            let db = refs.db.clone();
            let pool = refs.gene_candidate_pool.clone();
            let flag = refs.new_gene_candidates.clone();
            tokio::task::spawn_blocking(move || {
                Self::analyze_session_evals(&db, &pool, &flag);
            })
            .await
            .ok();
        }

        // 每 100 次 tick 更新人格画像（并带最小时间间隔 5 分钟）
        if refs.tick_count.load(Ordering::SeqCst) % 100 == 0 {
            let space = refs.active_space_id.read().await.clone();
            if !space.is_empty() {
                let should_update = refs
                    .last_personality_update
                    .lock()
                    .map(|guard| guard.elapsed() > Duration::from_secs(300))
                    .unwrap_or(false);
                if should_update {
                    if let Ok(mut guard) = refs.last_personality_update.lock() {
                        *guard = Instant::now();
                    }
                    let personality = refs.personality_model.clone();
                    tokio::spawn(async move {
                        if let Err(e) = personality.update_personality_profile(&space) {
                            tracing::warn!("personality update failed: {}", e);
                        }
                    });
                }
            }
        }

        // 检查是否有新上下文消息
        if !refs.has_new_context.load(Ordering::SeqCst) {
            return Ok(()); // 无新上下文，跳过本次 tick
        }

        // 清除新上下文标记
        refs.has_new_context.store(false, Ordering::SeqCst);

        // 切换状态为 Thinking
        *refs.state.write().await = ProactiveState::Thinking;
        tracing::info!("[ProactiveService] 检测到新上下文，运行主动 agent loop");

        // ── 场景驱动 Agent Loop ──────────────────────────────────────
        let response = Self::run_scenario_loop(refs).await?;
        // ──────────────────────────────────────────────────────────────

        // 处理 agent 返回结果
        if response.trim() == NO_MESSAGE_MARKER {
            // Agent 判断无需行动
            refs.no_message_count.fetch_add(1, Ordering::SeqCst);
            tracing::debug!("[ProactiveService] Agent 判断无需行动 (NO_MESSAGE)");
        } else {
            // Agent 生成了主动消息
            refs.action_count.fetch_add(1, Ordering::SeqCst);
            let now = chrono::Utc::now().to_rfc3339();
            *refs.last_action_at.write().await = Some(now.clone());

            let preview: String = response.chars().take(50).collect();
            tracing::info!(
                "[ProactiveService] Agent 生成主动消息: {}...",
                preview
            );

            // 构造主动消息并持久化
            let msg = ProactiveMessage {
                id: uuid::Uuid::new_v4().to_string(),
                content: response.clone(),
                generated_at: now,
                trigger_reason: "上下文变化触发".to_string(),
                tools_used: vec![],
            };

            if let Err(e) = refs.storage.save_message(&msg) {
                tracing::error!("[ProactiveService] 保存主动消息失败: {}", e);
            }

            // 通过消息总线通知前端（发布为 outgoing 消息）
            refs.infra
                .publish_outgoing(
                    "proactive",
                    &response,
                    serde_json::json!({
                        "source": "proactive",
                        "message_id": msg.id,
                    }),
                )
                .await;

            // 记录任务到 TaskMemoryManager
            let space_id = refs.active_space_id.read().await.clone();
            let task_record = TaskRecord {
                task_type: TaskType::Planning,
                description: format!(
                    "Proactive scenario evaluation: {}",
                    extract_summary_text(&response)
                ),
                status: TaskStatus::Success,
                files_changed: Vec::new(),
                tools_used: Vec::new(),
                duration_ms: 0,
                error_messages: Vec::new(),
                solution_summary: Some(response.chars().take(200).collect()),
                session_id: refs.last_active_session_id.read().await.clone(),
            };
            let _ = refs.task_memory_manager.record_task(&space_id, &task_record);
        }

        // 切换状态回 Idle
        *refs.state.write().await = ProactiveState::Idle;

        Ok(())
    }

    /// 场景驱动的主动 Agent Loop
    ///
    /// 1. 构建 ScenarioContext
    /// 2. 评估所有场景是否触发
    /// 3. 对触发的场景构建上下文并调用 LLM
    /// 4. 合并响应
    async fn run_scenario_loop(refs: &ProactiveStateRefs) -> anyhow::Result<String> {
        // 1. 构建 ScenarioContext — 先从 active session 窗口收集消息
        let active_space_id = refs.active_space_id.read().await.clone();
        let active_session_id = refs.last_active_session_id.read().await.clone();

        let ctx_windows = refs.context_messages.read().await;
        let recent_messages: Vec<_> = active_session_id
            .as_ref()
            .and_then(|sid| ctx_windows.get(sid))
            .map(|w| w.messages.iter().cloned().collect())
            .unwrap_or_default();
        drop(ctx_windows);

        let execution_logs = refs.execution_log_collector.recent(50).await;
        let pending_multimodal = refs.multimodal_queue.peek_all().await;
        let last_trigger_map = refs.scenario_manager.get_last_trigger_map().await;
        let tick_count = refs.tick_count.load(Ordering::SeqCst);
        let new_message_count = refs.new_message_count.load(Ordering::SeqCst);
        let new_execution_count = refs.new_execution_count.load(Ordering::SeqCst);

        // 检查最近是否有失败
        let recent_failures = refs.execution_log_collector.failures(1).await;
        let has_failures = !recent_failures.is_empty();

        // 构建会话上下文摘要 — 从 agent_messages/agent_turns/agent_sessions 提取
        let session_context = if let Some(ref session_id) = active_session_id {
            let tool_calls: Vec<_> = execution_logs.iter()
                .take(10)
                .map(|log| crate::proactive::scenarios::types::ToolCallSummary {
                    tool_name: log.tool_name.clone(),
                    success: log.success,
                    duration_ms: log.duration_ms,
                    summary: log.tool_output.get("summary")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .collect();

            // 尝试从主 DB 提取会话元数据（失败不影响主流程）
            let (reasoning_steps, cumulative_tokens, turn_count, workspace_files) =
                Self::extract_session_metadata(&refs.db, session_id);

            Some(crate::proactive::scenarios::types::SessionContext {
                tool_calls,
                reasoning_steps,
                cumulative_tokens,
                turn_count,
                workspace_files,
            })
        } else {
            None
        };

        // 预计算已有技能指纹（用于 skill_extraction 场景的前置去重）。
        // 查询成本低（≤50 行），且可避免不必要的大量重复技能生成，
        // ROI 远高于 token 增量（50条×100字符≈1,250 tokens）。
        let skill_fingerprints =
            Self::compute_skill_fingerprints_for_extraction(refs.memory_graph_store.as_ref(), &active_space_id, 50);

        // 从 Gene 候选池读取当前快照
        let (gene_count, gene_candidates_snapshot) = {
            let pool = refs.gene_candidate_pool.read().await;
            let count = pool.len();
            // Convert GeneCandidate to LearningCard snapshot for scenario context
            let cards: Vec<LearningCard> = pool.iter().map(|c| LearningCard {
                raw: c.content.clone(),
                card_type: c.card_type.clone().unwrap_or(LearningCardType::FailureLesson),
                failure_signal: None,
                tool_name: None,
                strategy_hint: StrategyHint {
                    condition: None,
                    action: None,
                    reason: c.reasoning.clone(),
                },
                files_touched: vec![],
                session_id: c.session_id.clone().unwrap_or_default(),
                score: c.score.unwrap_or(0.0) as f32,
                timestamp: c.timestamp.timestamp_millis(),
            }).collect();
            (count, cards)
        };

        // Build existing Gene fingerprints from GeneRepository for dedup in distillation
        let existing_gene_fingerprints = {
            let repo = refs.gene_repo.lock().unwrap_or_else(|e| {
                tracing::warn!("[ProactiveService] gene_repo lock poisoned: {}", e);
                std::process::abort()
            });
            repo.list_active_genes()
                .unwrap_or_default()
                .iter()
                .map(|g| format!("[{}] {}: {}", g.category, g.gene_id, g.summary))
                .collect::<Vec<_>>()
        };

        let scenario_ctx = ScenarioContext {
            recent_messages,
            execution_logs,
            pending_multimodal,
            last_trigger_at: last_trigger_map,
            tick_count,
            new_message_count,
            new_execution_count,
            has_failures,
            active_space_id,
            active_session_id,
            session_context,
            existing_skill_fingerprints: skill_fingerprints,
            gene_candidate_count: gene_count,
            gene_candidates: gene_candidates_snapshot,
            existing_gene_fingerprints,
        };

        // 2. 评估场景触发
        let triggered_scenarios = refs.scenario_manager.evaluate_all(&scenario_ctx).await;

        if triggered_scenarios.is_empty() {
            tracing::debug!("[ProactiveService] 无场景触发，返回 NO_MESSAGE");
            return Ok(NO_MESSAGE_MARKER.to_string());
        }

        tracing::info!(
            "[ProactiveService] {} 个场景触发: {:?}",
            triggered_scenarios.len(),
            triggered_scenarios.iter().map(|s| s.name()).collect::<Vec<_>>()
        );

        // 3. 对每个触发的场景构建上下文并调用 LLM
        let mut responses = Vec::new();

        for scenario in &triggered_scenarios {
            // plan_mode_calibration runs its logic in build_context and
            // needs no LLM call — short-circuit here after calling it.
            if scenario.name() == "plan_mode_calibration" {
                let _ = scenario.build_context(&scenario_ctx).await;
                refs.scenario_manager.mark_triggered(scenario.name()).await;
                continue;
            }

            match scenario.build_context(&scenario_ctx).await {
                Ok(output) => {
                    // 增强 system prompt：通过五路混合检索引擎注入召回的记忆
                    let enhanced_system_prompt = {
                        // skill_extraction 场景使用更轻量的召回：
                        // 指纹已在 context_messages 中作为去重参考，
                        // 此处仅保留少量混合检索结果作为补充上下文。
                        let max_results = if scenario.name() == "skill_extraction" {
                            6  // 轻量：指纹已替代大部分召回
                        } else {
                            15  // 默认：五路融合结果
                        };

                        let recall_query = format!("{} {}", scenario.name(), scenario.description());
                        let hybrid_request = HybridSearchRequest {
                            query: recall_query,
                            space_id: scenario_ctx.active_space_id.clone(),
                            session_id: scenario_ctx.active_session_id.clone(),
                            time_range: None,
                            file_paths: None,
                            max_results,
                        };

                        let base_prompt = match refs.hybrid_search_engine.search(&hybrid_request, None).await {
                            Ok(result) => {
                                let memory_ctx = HybridSearchEngine::format_for_prompt(&result);
                                if memory_ctx.trim().is_empty() {
                                    output.system_prompt.clone()
                                } else {
                                    format!(
                                        "{}\n\n## Previously Learned Context\n{}",
                                        output.system_prompt,
                                        memory_ctx
                                    )
                                }
                            }
                            Err(e) => {
                                tracing::debug!(scenario = %scenario.name(), error = %e, "Hybrid recall for scenario failed, using base prompt");
                                output.system_prompt.clone()
                            }
                        };

                        // 注入当前时间作为权威时间上下文
                        let time_block = build_proactive_time_block();
                        format!("{}\n{}", base_prompt, time_block)
                    };

                    // 构建 LLM 消息
                    let mut messages = vec![];
                    messages.push(ChatMessage::system(&enhanced_system_prompt));

                    // 添加场景上下文消息
                    for (role, content) in &output.context_messages {
                        match role.as_str() {
                            "user" => messages.push(ChatMessage::user(content)),
                            "assistant" => messages.push(ChatMessage::assistant(content)),
                            _ => messages.push(ChatMessage::user(content)),
                        }
                    }

                    // 添加额外指令（如果有）
                    if let Some(ref instructions) = output.additional_instructions {
                        messages.push(ChatMessage::user(instructions));
                    }

                    // 调用 LLM
                    match Self::call_llm_for_scenario(refs, &messages).await {
                        Ok(llm_response) => {
                            // memU 记忆持久化
                            if llm_response.trim() != NO_MESSAGE_MARKER {
                                // skill_extraction 场景：解析 XML 并存储为 Procedure 节点
                                if scenario.name() == "skill_extraction" {
                                    let mut parsed_skills = crate::proactive::skill_parser::parse_skill_report(&llm_response);
                                    tracing::info!(
                                        parsed_count = parsed_skills.len(),
                                        skill_names = ?parsed_skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
                                        "[skill_extraction] parse_skill_report completed"
                                    );
                                    // Classify failure types from execution logs and stamp
                                    // each new skill with signals_seen (empirical counterpart
                                    // to the LLM-prescribed signals[]).
                                    let signals_seen = crate::proactive::scenarios::skill_extraction::extract_signals_seen(
                                        &scenario_ctx.execution_logs
                                    );
                                    for skill in &mut parsed_skills {
                                        skill.signals_seen = signals_seen.clone();
                                    }

                                    // Resolve session_id early for event publishing
                                    let session_id = refs.last_active_session_id.read().await.clone();

                                    let items_extracted = if !parsed_skills.is_empty() {
                                        let space_id = scenario_ctx.active_space_id.clone();
                                        let mut stored_count = 0usize;
                                        for skill in &parsed_skills {
                                            match crate::proactive::skill_parser::store_skill_as_procedure(
                                                &refs.memory_graph_store, skill, &space_id
                                            ) {
                                                Ok(node) => {
                                                    tracing::info!(
                                                        skill_name = %skill.name,
                                                        node_id = %node.id,
                                                        "Stored learned skill as Procedure node"
                                                    );
                                                    stored_count += 1;

                                                    // ─── Bundle 22 — persist to disk as SKILL.md ───
                                                    // The Procedure node above lives in
                                                    // memory_graph only; the disk-tier
                                                    // SkillsRegistry never sees it. Bundle 22
                                                    // mirrors the skill out to
                                                    // <data_dir>/skills/_auto_extracted/<slug>/
                                                    // SKILL.md so it shows up in skill_search
                                                    // on the next session start. Best-effort:
                                                    // a write failure logs a warning but does
                                                    // NOT roll back the Procedure node — disk
                                                    // persistence is the bonus path, not the
                                                    // source of truth.
                                                    match crate::proactive::skill_parser::persist_learned_skill_to_disk(
                                                        &refs.data_dir,
                                                        skill,
                                                    ) {
                                                        Ok(path) => {
                                                            tracing::info!(
                                                                skill_name = %skill.name,
                                                                node_id = %node.id,
                                                                path = %path.display(),
                                                                "[Bundle 22] persisted learned skill to disk"
                                                            );

                                                            // Bundle 23 — same-session
                                                            // visibility. Trigger disk-tier
                                                            // rescan so skill_search picks
                                                            // the new skill up THIS session,
                                                            // no restart.
                                                            let discover_count = {
                                                                let mut reg = refs.skills_registry.write().await;
                                                                reg.discover().len()
                                                            };
                                                            tracing::info!(
                                                                skill_name = %skill.name,
                                                                path = %path.display(),
                                                                discovered = discover_count,
                                                                "[Bundle 23] same-session skill visibility: disk-tier rescan done"
                                                            );

                                                            if let Some(ref app) = refs.app_handle {
                                                                let _ = app.emit(
                                                                    "agent:learned-skill-persisted",
                                                                    serde_json::json!({
                                                                        "skillName": skill.name,
                                                                        "nodeId": node.id.clone(),
                                                                        "path": path.display().to_string(),
                                                                        "sessionId": session_id.clone(),
                                                                        "registryDiscovered": discover_count,
                                                                        "sameSessionVisible": true,
                                                                        "timestamp": chrono::Utc::now().to_rfc3339(),
                                                                    }),
                                                                );
                                                            }
                                                        }
                                                        Err(e) => {
                                                            tracing::warn!(
                                                                skill_name = %skill.name,
                                                                node_id = %node.id,
                                                                err = %e,
                                                                "[Bundle 22] failed to persist learned skill to disk \
                                                                 (Procedure node already stored, so skill is still \
                                                                 discoverable via recall)"
                                                            );
                                                        }
                                                    }

                                                    // ─── Publish SkillLearned with learning_card ───
                                                    // Bridges skill_extraction → gene_candidate_pool:
                                                    // each extracted skill becomes a GeneCandidate,
                                                    // closing the learning loop between skill extraction
                                                    // and gene evolution.
                                                    {
                                                        let card_type = match skill.category.as_deref() {
                                                            Some("repair") => "failure_lesson",
                                                            Some("optimize") => "optimization_tip",
                                                            Some("innovate") => "success_pattern",
                                                            _ => "success_pattern",
                                                        };
                                                        let failure_signal = skill.signals_seen.first().cloned();
                                                        let description = skill.description.as_deref()
                                                            .unwrap_or(&skill.context);
                                                        let action_first_line = skill.steps
                                                            .lines()
                                                            .next()
                                                            .unwrap_or("")
                                                            .trim()
                                                            .to_string();
                                                        let reason_text = skill.principles
                                                            .lines()
                                                            .take(2)
                                                            .collect::<Vec<_>>()
                                                            .join(" ");

                                                        refs.infra.publish_skill_learned(
                                                            "proactive",
                                                            &skill.name,
                                                            serde_json::json!({
                                                                "scenario": "skill_extraction",
                                                                "session_id": session_id,
                                                                "score": 0.7,
                                                                "source": "skill_extraction",
                                                                "learning_card": {
                                                                    "card_type": card_type,
                                                                    "failure_signal": failure_signal,
                                                                    "tool_name": null,
                                                                    "strategy_hint": {
                                                                        "condition": description,
                                                                        "action": action_first_line,
                                                                        "reason": reason_text,
                                                                    },
                                                                },
                                                            }),
                                                        ).await;

                                                        tracing::info!(
                                                            skill_name = %skill.name,
                                                            card_type,
                                                            pool_feed = true,
                                                            "[skill_extraction] Published SkillLearned with learning_card → gene_candidate_pool"
                                                        );
                                                    }

                                                    // Best-effort: embed the skill body and persist
                                                    // to memory_versions.embedding_json. Failure
                                                    // is logged but never aborts skill storage.
                                                    if refs.memu_client.is_some() {
                                                        let body = crate::proactive::skill_parser::build_version_content(skill);
                                                        // Construct the text to embed: body + signals so that semantic search
                                                        // can match queries against trigger phrases even if they're not in the body.
                                                        let mut embed_text = body.clone();
                                                        if !skill.signals.is_empty() {
                                                            embed_text.push_str("\n\nTrigger signals: ");
                                                            embed_text.push_str(&skill.signals.join(", "));
                                                        }
                                                        if !skill.signals_seen.is_empty() {
                                                            embed_text.push_str("\n\nObserved error types: ");
                                                            embed_text.push_str(&skill.signals_seen.join(", "));
                                                        }
                                                        let store = Arc::clone(&refs.memory_graph_store);
                                                        let memu = refs.memu_client.clone();
                                                        let node_id = node.id.clone();
                                                        tokio::spawn(async move {
                                                            if let Some(vec) = crate::memu::embedding::embed_skill_body(&memu, &embed_text).await {
                                                                let json = crate::memu::embedding::serialize_embedding(&vec);
                                                                if let Ok(Some(ver)) = store.get_active_version(&node_id) {
                                                                    if let Err(e) = store.update_version_embedding(&ver.id, &json) {
                                                                        tracing::warn!(node_id, error = %e, "failed to persist skill embedding");
                                                                    }
                                                                }
                                                            }
                                                        });
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::warn!(
                                                        skill_name = %skill.name,
                                                        error = %e,
                                                        "Failed to store skill as Procedure node"
                                                    );
                                                }
                                            }
                                        }
                                        stored_count
                                    } else {
                                        // 没有解析到技能，fallback 到 memorize_with_config
                                        if let Some(ref memu) = refs.memu_client {
                                            match memu.memorize_with_config(
                                                &llm_response,
                                                &["skill", "tool"],
                                                None,
                                                "proactive_skill_extraction",
                                            ).await {
                                                Ok(result) => result.items_extracted,
                                                Err(e) => {
                                                    tracing::warn!(
                                                        scenario = %scenario.name(),
                                                        error = %e,
                                                        "Proactive memory extraction failed (fallback)"
                                                    );
                                                    0
                                                }
                                            }
                                        } else {
                                            0
                                        }
                                    };

                                    // Generic heartbeat event for frontend observability
                                    refs.infra.publish_skill_learned(
                                        "proactive",
                                        scenario.name(),
                                        serde_json::json!({
                                            "scenario": scenario.name(),
                                            "items_extracted": items_extracted,
                                        }),
                                    ).await;

                                    // Tauri IPC 发射到前端
                                    let summary = extract_summary_text(&llm_response);
                                    // Diagnostic: surface the sessionId we tag the
                                    // event with so we can correlate with the
                                    // frontend's filter when chips don't show.
                                    tracing::info!(
                                        items_extracted,
                                        session_id = ?session_id,
                                        scenario = "skill_extraction",
                                        "Emitting agent:proactive-learning IPC event"
                                    );
                                    if let Some(ref handle) = refs.app_handle {
                                        let _ = handle.emit("agent:proactive-learning", serde_json::json!({
                                            "scenario": "skill_extraction",
                                            "items_extracted": items_extracted,
                                            "categories": ["procedure"],
                                            "timestamp": chrono::Utc::now().to_rfc3339(),
                                            "summary": summary,
                                            "sessionId": session_id,
                                        }));
                                    }
                                } else if scenario.name() == "gene_evolution" {
                                    // GEP Gene Evolution: parse Gene XML from LLM output
                                    let distillation_outcome: Option<(Gene, Capsule, EvolutionEvent)> =
                                    match distillation::parse_gene_xml(&llm_response) {
                                        Ok(gene) => {
                                            // Validate gene
                                            if let Err(e) = distillation::validate_gene(&gene) {
                                                tracing::warn!("[gene_evolution] Gene validation failed: {}", e);
                                                None
                                            } else {
                                                // Check duplicates and store — keep lock scope minimal
                                                let mut repo = refs.gene_repo.lock().unwrap();
                                                let existing = repo.list_active_genes().unwrap_or_default();
                                                if let Some(dup_id) = distillation::check_duplicate(&gene, &existing) {
                                                    tracing::info!("[gene_evolution] Duplicate gene detected: {}", dup_id);
                                                    None
                                                } else {
                                                    let mut gene = gene;
                                                    match repo.store_gene(&mut gene) {
                                                        Ok(()) => {
                                                            // Generate initial Capsule
                                                            let capsule = Capsule {
                                                                id: format!("cap_init_{}", &gene.asset_id[..8]),
                                                                gene_asset_id: gene.asset_id.clone(),
                                                                gene_id: gene.gene_id.clone(),
                                                                trigger: gene.signals_match.clone(),
                                                                summary: format!("Initial capsule for gene {}", gene.gene_id),
                                                                confidence: 0.8,
                                                                blast_radius: BlastRadius { files: 0, lines: 0 },
                                                                outcome: CapsuleOutcome {
                                                                    status: OutcomeStatus::Success,
                                                                    score: 0.8,
                                                                },
                                                                raw_streak: 0,
                                                                effective_streak: 0.0,
                                                                env_fingerprint: EnvFingerprint::default(),
                                                                created_at: chrono::Utc::now().timestamp_millis(),
                                                                lineage: vec![],
                                                            };
                                                            let _ = repo.store_capsule(&capsule);

                                                            // Store EvolutionEvent
                                                            let event = EvolutionEvent {
                                                                intent: gene.category.to_string(),
                                                                capsule_id: capsule.id.clone(),
                                                                genes_used: vec![gene.asset_id.clone()],
                                                                mutations_tried: 0,
                                                                total_cycles: 1,
                                                                created_at: chrono::Utc::now().timestamp_millis(),
                                                            };
                                                            let _ = repo.store_event(&event);

                                                            tracing::info!(
                                                                gene_id = %gene.gene_id,
                                                                asset_id = %gene.asset_id,
                                                                "[gene_evolution] New gene distilled and stored"
                                                            );
                                                            Some((gene, capsule, event))
                                                        }
                                                        Err(e) => {
                                                            tracing::error!("[gene_evolution] Failed to store gene: {}", e);
                                                            None
                                                        }
                                                    }
                                                }
                                            } // MutexGuard dropped here
                                        }
                                        Err(e) => {
                                            tracing::warn!("[gene_evolution] Failed to parse Gene XML: {}", e);
                                            None
                                        }
                                    };

                                    // Clear consumed candidates (after MutexGuard is dropped)
                                    if distillation_outcome.is_some() {
                                        refs.gene_candidate_pool.write().await.clear();
                                        refs.new_gene_candidates.store(false, Ordering::SeqCst);
                                    }
                                } else {
                                    // 其他场景保持原有的 memorize_with_config 逻辑
                                    if let Some(ref memu) = refs.memu_client {
                                        let (memory_types, source_type) = match scenario.name() {
                                            "conversation_learning" => (
                                                vec!["profile", "behavior", "event", "knowledge"],
                                                "proactive_conversation_learning",
                                            ),
                                            "multimodal_context" => (
                                                vec!["multimodal", "knowledge"],
                                                "proactive_multimodal_context",
                                            ),
                                            _other => (
                                                vec!["knowledge"],
                                                "proactive_unknown",
                                            ),
                                        };

                                        match memu.memorize_with_config(
                                            &llm_response,
                                            &memory_types,
                                            None,
                                            source_type,
                                        ).await {
                                            Ok(result) => {
                                                tracing::info!(
                                                    scenario = %scenario.name(),
                                                    items = result.items_extracted,
                                                    categories = ?result.categories_updated,
                                                    "Proactive memory extraction complete"
                                                );

                                                // InfraService 事件
                                                refs.infra.publish_memory_extracted(
                                                    "proactive",
                                                    scenario.name(),
                                                    serde_json::json!({
                                                        "scenario": scenario.name(),
                                                        "items_extracted": result.items_extracted,
                                                    }),
                                                ).await;

                                                // Tauri IPC 发射到前端
                                                let summary = extract_summary_text(&llm_response);
                                                let scenario_key = match scenario.name() {
                                                    "conversation_learning" => "conversation_learning",
                                                    "multimodal_context" => "multimodal_context",
                                                    _ => "conversation_learning",
                                                };
                                                let session_id = refs.last_active_session_id.read().await.clone();
                                                tracing::info!(
                                                    items_extracted = result.items_extracted,
                                                    session_id = ?session_id,
                                                    scenario = scenario_key,
                                                    "Emitting agent:proactive-learning IPC event"
                                                );
                                                if let Some(ref handle) = refs.app_handle {
                                                    let _ = handle.emit("agent:proactive-learning", serde_json::json!({
                                                        "scenario": scenario_key,
                                                        "items_extracted": result.items_extracted,
                                                        "categories": result.categories_updated,
                                                        "timestamp": chrono::Utc::now().to_rfc3339(),
                                                        "summary": summary,
                                                        "sessionId": session_id,
                                                    }));
                                                }
                                            }
                                            Err(e) => {
                                                tracing::warn!(
                                                    scenario = %scenario.name(),
                                                    error = %e,
                                                    "Proactive memory extraction failed"
                                                );
                                            }
                                        }
                                    }
                                }

                                responses.push(llm_response);
                            }
                            // 标记场景已触发
                            refs.scenario_manager.mark_triggered(scenario.name()).await;
                        }
                        Err(e) => {
                            tracing::warn!(
                                "[ProactiveService] 场景 {} LLM 调用失败: {}",
                                scenario.name(),
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "[ProactiveService] 场景 {} build_context 失败: {}",
                        scenario.name(),
                        e
                    );
                }
            }
        }

        // 场景触发后重置计数器
        refs.new_message_count.store(0, Ordering::SeqCst);
        refs.new_execution_count.store(0, Ordering::SeqCst);

        // 如果 multimodal 场景被触发了，清空已处理的多模态队列
        if triggered_scenarios.iter().any(|s| s.name() == "multimodal_context") {
            refs.multimodal_queue.drain_all().await;
        }

        // 4. 合并响应
        if responses.is_empty() {
            Ok(NO_MESSAGE_MARKER.to_string())
        } else {
            Ok(responses.join("\n\n---\n\n"))
        }
    }

    /// 从主 DB（agent_messages / agent_turns / agent_sessions）提取会话元数据。
    ///
    /// 填充 SessionContext 中此前为空的字段：
    /// - reasoning_steps: 最近 5 条 agent_messages 中的 reasoning 文本
    /// - cumulative_tokens: agent_messages 表中 input/output tokens 累计
    /// - turn_count: agent_turns 表中该 session 的轮次总数
    /// - workspace_files: agent_sessions.attached_dirs 中的目录列表
    ///
    /// 所有 DB 错误均被吞没（返回默认空值），不影响主 tick 流程。
    fn extract_session_metadata(
        db: &Arc<Mutex<Connection>>,
        session_id: &str,
    ) -> (
        Vec<String>,
        Option<crate::proactive::scenarios::types::TokenUsage>,
        Option<usize>,
        Vec<String>,
    ) {
        let conn = match db.lock() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("[ProactiveService] 获取 DB 锁失败: {}", e);
                return (vec![], None, None, vec![]);
            }
        };

        // 1. reasoning_steps: 最近 5 条有 reasoning 的 agent_messages
        let reasoning_steps: Vec<String> = {
            let mut stmt = match conn.prepare(
                "SELECT reasoning FROM agent_messages \
                 WHERE session_id = ?1 AND reasoning IS NOT NULL AND reasoning != '' \
                 ORDER BY created_at DESC LIMIT 5"
            ) {
                Ok(s) => s,
                Err(_) => return (vec![], None, None, vec![]),
            };
            let rows: Vec<String> = stmt
                .query_map(rusqlite::params![session_id], |row| row.get(0))
                .into_iter()
                .flat_map(|r| r.filter_map(|v| v.ok()))
                .collect();
            rows
        };

        // 2. cumulative_tokens: SUM input/output tokens from agent_messages
        let cumulative_tokens: Option<crate::proactive::scenarios::types::TokenUsage> = {
            conn.query_row(
                "SELECT COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0) \
                 FROM agent_messages WHERE session_id = ?1",
                rusqlite::params![session_id],
                |row| {
                    Ok(crate::proactive::scenarios::types::TokenUsage {
                        input_tokens: row.get::<_, i64>(0)? as u64,
                        output_tokens: row.get::<_, i64>(1)? as u64,
                    })
                },
            ).ok()
        };

        // 3. turn_count: COUNT from agent_turns
        let turn_count: Option<usize> = {
            conn.query_row(
                "SELECT COUNT(*) FROM agent_turns WHERE session_id = ?1",
                rusqlite::params![session_id],
                |row| row.get::<_, i64>(0).map(|v| v as usize),
            ).ok()
        };

        // 4. workspace_files: attached_dirs JSON array from agent_sessions
        let workspace_files: Vec<String> = {
            let dirs_json: Option<String> = conn.query_row(
                "SELECT attached_dirs FROM agent_sessions WHERE id = ?1",
                rusqlite::params![session_id],
                |row| row.get(0),
            ).ok().flatten();

            dirs_json
                .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
                .unwrap_or_default()
        };

        (reasoning_steps, cumulative_tokens, turn_count, workspace_files)
    }

    /// 从主 DB 的 session_evals 表读取并分析评估数据。
    ///
    /// 激活 KR4 数据回路：session_evals 不再只写不读。
    /// 定期分析近期评估质量趋势，检测得分下降等异常信号，
    /// 并将退化信号注入 Gene 候选池供蒸馏使用。
    fn analyze_session_evals(
        db: &Arc<Mutex<Connection>>,
        gene_candidate_pool: &Arc<RwLock<VecDeque<GeneCandidate>>>,
        new_gene_candidates_flag: &Arc<AtomicBool>,
    ) -> Option<SessionEvalSummary> {
        let conn = match db.lock() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("[ProactiveService] analyze_session_evals: DB lock failed: {}", e);
                return None;
            }
        };

        // 查询最近 200 条 session_evals
        let mut stmt = match conn.prepare(
            "SELECT score, learnings, session_id, created_at FROM session_evals \
             ORDER BY created_at DESC LIMIT 200"
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("[ProactiveService] analyze_session_evals: prepare failed: {}", e);
                return None;
            }
        };

        let rows: Vec<(f64, String, String, i64)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .into_iter()
            .flat_map(|r| r.filter_map(|v| v.ok()))
            .collect();

        if rows.is_empty() {
            return None;
        }

        let total_evals = rows.len();
        let total_score: f64 = rows.iter().map(|r| r.0).sum();
        let avg_score = total_score / total_evals as f64;

        let total_learnings: usize = rows
            .iter()
            .filter_map(|r| serde_json::from_str::<Vec<String>>(&r.1).ok())
            .map(|l| l.len())
            .sum();

        // 最近 10 条的趋势
        let recent_count = total_evals.min(10);
        let recent_slice = &rows[..recent_count];
        let recent_avg_score: f64 = recent_slice.iter().map(|r| r.0).sum::<f64>() / recent_count as f64;

        let last_eval_at = rows.first().map(|r| r.3);

        // 如果最近 10 条平均分显著低于总体平均（差值 > 0.1），标记为退化
        let score_degrading = recent_count >= 5 && (avg_score - recent_avg_score) > 0.1;

        tracing::info!(
            total = total_evals,
            avg = %format!("{:.2}", avg_score),
            recent_avg = %format!("{:.2}", recent_avg_score),
            learnings = total_learnings,
            degrading = score_degrading,
            "[ProactiveService] session_evals analysis"
        );

        // 如果检测到得分退化，将信号注入 Gene 候选池
        if score_degrading {
            let degraded_sessions: Vec<String> = recent_slice
                .iter()
                .filter(|r| r.0 < avg_score)
                .map(|r| r.2.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .take(5)
                .collect();

            let observation = format!(
                "Recent self-eval scores ({:.2} avg over {} evals) are \
                 declining compared to historical average ({:.2} over {} evals). \
                 Degraded sessions: {}.",
                recent_avg_score, recent_count,
                avg_score, total_evals,
                degraded_sessions.join(", ")
            );

            // 使用 tokio spawn 异步注入候选（避免阻塞 tick）
            let pool = gene_candidate_pool.clone();
            let flag = new_gene_candidates_flag.clone();
            tokio::spawn(async move {
                let candidate = GeneCandidate {
                    source: "session_evals_analysis".to_string(),
                    content: observation,
                    card_type: Some(LearningCardType::FailureLesson),
                    score: Some(recent_avg_score),
                    session_id: Some("proactive_analytics".to_string()),
                    reasoning: Some(
                        "Self-eval score degradation detected: agent performance may be declining. \
                         Consider reviewing recent session patterns and adjusting strategies."
                            .to_string(),
                    ),
                    timestamp: chrono::Utc::now(),
                };
                pool.write().await.push_back(candidate);
                flag.store(true, Ordering::SeqCst);
                tracing::info!("[ProactiveService] Injected score-degradation signal into gene candidate pool");
            });
        }

        Some(SessionEvalSummary {
            total_evals,
            avg_score,
            recent_avg_score,
            recent_count,
            total_learnings,
            last_eval_at,
            score_degrading,
        })
    }

    /// 为 skill_extraction 场景计算已有技能的紧凑指纹列表。
    ///
    /// 每条指纹格式: "title | description(≤60chars) | category | cited:N"
    /// 按 cited_count DESC 排序，取 top-N。用于注入到 LLM 上下文
    /// 供提取阶段前置去重——LLM 在生成新 `<skill>` 前可逐条对比。
    ///
    /// Token 预算：50 条 × ~100 字符 ≈ 1,250 tokens（含格式开销）。
    /// 与完整 SOP 注入（每条 300-800 字符）相比节省 60-80%。
    fn compute_skill_fingerprints_for_extraction(
        store: &MemoryGraphStore,
        space_id: &str,
        limit: usize,
    ) -> Vec<String> {
        let nodes = match store.list_top_learned_skills(space_id, limit) {
            Ok(nodes) => nodes,
            Err(e) => {
                tracing::warn!("Failed to list skills for fingerprints: {}", e);
                return vec![];
            }
        };

        nodes
            .into_iter()
            .map(|detail| {
                let node = &detail.node;
                let meta = node.metadata.as_ref();
                let desc = meta
                    .and_then(|m| m.get("description"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let desc_short: String = if desc.chars().count() > 60 {
                    format!("{}…", desc.chars().take(60).collect::<String>())
                } else {
                    desc.to_string()
                };
                let category = meta
                    .and_then(|m| m.get("category"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("-");
                let cited = meta
                    .and_then(|m| m.get("cited_count"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                format!(
                    "{} | {} | {} | cited:{}",
                    node.title, desc_short, category, cited
                )
            })
            .collect()
    }

    /// 调用 LLM 处理场景
    ///
    /// 从 ProviderService 动态获取当前活动的 LLM 配置，
    /// 创建 provider 并执行调用。
    async fn call_llm_for_scenario(
        refs: &ProactiveStateRefs,
        messages: &[ChatMessage],
    ) -> anyhow::Result<String> {
        // 从 ProviderService 获取当前活动的 LLM 配置
        let llm_config_tuple = refs.provider_service.get_active_llm_config().await;
        let (provider_id, model, api_key, base_url, _api) = match llm_config_tuple {
            Some(cfg) => cfg,
            None => {
                tracing::debug!("[ProactiveService] LLM provider 未配置，返回 NO_MESSAGE");
                return Ok(NO_MESSAGE_MARKER.to_string());
            }
        };

        // 构建 LlmConfig 并创建 provider
        let llm_config = crate::llm::llm_config_from_provider(
            &provider_id,
            &model,
            &api_key,
            &base_url,
            4096,  // 主动服务使用较小的 max_tokens
            0.7,
            None, // TODO(Task 2): effective api
        );
        let provider = crate::llm::create_provider(&llm_config)
            .map_err(|e| anyhow::anyhow!("LLM provider 创建失败: {}", e))?;

        let config = CompletionConfig {
            model: model.clone(),
            max_tokens: 4096,
            temperature: 0.7,
            thinking_enabled: false,
        };

        // 调用 LLM（不使用工具）
        let output = provider
            .complete(messages.to_vec(), vec![], &config)
            .await
            .map_err(|e| anyhow::anyhow!("LLM 调用失败: {}", e))?;

        // 提取文本响应
        match output {
            crate::agent::types::RespondOutput::Text { text, .. } => Ok(text),
            crate::agent::types::RespondOutput::ToolCalls { text, .. } => {
                // 主动服务当前不处理工具调用，取文本部分
                Ok(text.unwrap_or_else(|| NO_MESSAGE_MARKER.to_string()))
            }
        }
    }

    /// 设置用户确认响应
    ///
    /// 当 agent 使用 `wait_user_confirm` 工具时，状态切换为 WaitingUserInput，
    /// 前端收集用户输入后通过此方法回传。
    pub async fn set_user_input(&self, input: String) {
        *self.user_input_response.write().await = Some(input);
        self.is_waiting_user_input.store(false, Ordering::SeqCst);
        tracing::info!("[ProactiveService] 收到用户确认响应");
    }

    // ─── Accessor Methods ────────────────────────────────────────────────

    pub fn failure_memory(&self) -> &Arc<FailureMemoryManager> { &self.failure_memory_manager }
    pub fn preference_extractor(&self) -> &Arc<PreferenceExtractor> { &self.preference_extractor }
    pub fn personality_model(&self) -> &Arc<PersonalityModel> { &self.personality_model }
    pub fn proactive_recall(&self) -> &Arc<ProactiveRecallService> { &self.proactive_recall_service }

    /// 获取当前主动服务状态报告
    pub async fn get_proactive_status(&self) -> ProactiveStatus {
        let state = *self.state.read().await;
        let windows = self.context_messages.read().await;
        let context_count: usize = windows.values().map(|w| w.messages.len()).sum();
        drop(windows);

        ProactiveStatus {
            state,
            is_running: self.is_running.load(Ordering::SeqCst),
            tick_count: self.tick_count.load(Ordering::SeqCst),
            action_count: self.action_count.load(Ordering::SeqCst),
            no_message_count: self.no_message_count.load(Ordering::SeqCst),
            last_tick_at: self.last_tick_at.read().await.clone(),
            last_action_at: self.last_action_at.read().await.clone(),
            context_message_count: context_count,
        }
    }
}

// ─── cited_count 周期性衰减 ───────────────────────────────────────────

/// 对所有已学习技能的 `cited_count` 应用 5% 衰减（floor(prev * 0.95)）。
///
/// 返回实际执行了更新的节点数。`cited_count` 已为 0 的节点跳过，
/// 不产生写操作。
pub fn decay_cited_counts(
    store: &MemoryGraphStore,
    space_id: &str,
) -> Result<usize, crate::error::Error> {
    let nodes = store.list_top_learned_skills(space_id, 10_000)?;
    let mut updated = 0;
    for detail in nodes {
        let mut meta = detail.node.metadata.clone().unwrap_or(serde_json::json!({}));
        let prev = meta.get("cited_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let next = ((prev as f64) * 0.95).floor() as u64;
        if next == prev {
            continue; // no change (includes cited_count == 0 case)
        }
        if let Some(obj) = meta.as_object_mut() {
            obj.insert(
                "cited_count".to_string(),
                serde_json::Value::Number(serde_json::Number::from(next)),
            );
        }
        if store.update_node(&detail.node.id, None, None, Some(&meta)).is_ok() {
            updated += 1;
        }
    }
    tracing::info!(updated, "cited_count decay tick complete");
    Ok(updated)
}

// ─── ManagedService 实现 ──────────────────────────────────────────────

#[async_trait]
impl ManagedService for ProactiveService {
    /// 服务名称
    fn name(&self) -> &str {
        "proactive"
    }

    /// 启动主动服务
    ///
    /// 如果配置中 `enabled = false`，跳过启动。
    /// 否则启动上下文监听和轮询循环两个后台任务。
    async fn start(&self) -> anyhow::Result<()> {
        if !self.config.enabled {
            tracing::info!("[ProactiveService] 主动服务已禁用，跳过启动");
            return Ok(());
        }

        tracing::info!(
            "[ProactiveService] 启动主动服务，轮询间隔: {}ms，最大迭代: {}",
            self.config.interval_ms,
            self.config.max_iterations
        );

        // 设置运行标记
        self.is_running.store(true, Ordering::SeqCst);

        // 启动上下文监听
        self.start_context_listener().await;

        // 启动轮询循环
        self.start_tick_loop().await;

        // 启动 cited_count 周衰减任务（独立于主 tick loop，每 7 天触发一次）
        {
            let store = Arc::clone(&self.memory_graph_store);
            tokio::spawn(async move {
                const ONE_WEEK: std::time::Duration =
                    std::time::Duration::from_secs(7 * 24 * 60 * 60);
                let mut ticker = tokio::time::interval(ONE_WEEK);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                // Skip the immediate t=0 fire — first decay runs after one full week
                ticker.tick().await;
                loop {
                    ticker.tick().await;
                    if let Err(e) = decay_cited_counts(&store, "default") {
                        tracing::warn!(err = %e, "decay_cited_counts failed");
                    }
                }
            });
        }

        // One-shot backfill: embed legacy skill versions that have NULL embedding_json.
        // Runs at idle priority (spawned independently so it never blocks the tick loop).
        if self.memu_client.is_some() {
            let store = Arc::clone(&self.memory_graph_store);
            let memu = self.memu_client.clone();
            tokio::spawn(async move {
                // Short initial delay so the bridge is fully ready.
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                // TODO(multi-space): backfill currently iterates only the "default" space.
                // Once multi-space skill storage ships, iterate all spaces here so legacy
                // embeddings get filled across the board. For v1 (single "default" space)
                // this is correct.
                match store.list_versions_without_embedding("default", 500) {
                    Ok(pairs) if !pairs.is_empty() => {
                        tracing::info!(count = pairs.len(), "embedding backfill: starting");
                        let mut filled = 0usize;
                        for (version_id, content) in &pairs {
                            if let Some(vec) = crate::memu::embedding::embed_skill_body(&memu, content).await {
                                let json = crate::memu::embedding::serialize_embedding(&vec);
                                if let Err(e) = store.update_version_embedding(version_id, &json) {
                                    tracing::warn!(version_id, error = %e, "embedding backfill: write failed");
                                } else {
                                    filled += 1;
                                }
                            }
                            // Yield between calls so we don't saturate the bridge.
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        }
                        tracing::info!(filled, total = pairs.len(), "embedding backfill: complete");
                    }
                    Ok(_) => {
                        tracing::debug!("embedding backfill: no versions need embedding");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "embedding backfill: failed to list versions");
                    }
                }
            });
        }

        // 切换状态为 Idle（等待首次 tick）
        *self.state.write().await = ProactiveState::Idle;

        tracing::info!("[ProactiveService] 主动服务启动完成");
        Ok(())
    }

    /// 停止主动服务
    ///
    /// 设置 is_running = false 并 abort 所有后台任务，
    /// 确保优雅关闭。
    async fn stop(&self) -> anyhow::Result<()> {
        tracing::info!("[ProactiveService] 正在停止主动服务...");

        // 清除运行标记（后台任务会自行检测并退出）
        self.is_running.store(false, Ordering::SeqCst);

        // 切换状态为 Stopped
        *self.state.write().await = ProactiveState::Stopped;

        // 中止轮询循环任务
        if let Some(h) = self.tick_handle.write().await.take() {
            h.abort();
            tracing::debug!("[ProactiveService] 轮询循环任务已中止");
        }

        // 中止上下文监听任务
        if let Some(h) = self.listener_handle.write().await.take() {
            h.abort();
            tracing::debug!("[ProactiveService] 上下文监听任务已中止");
        }

        tracing::info!("[ProactiveService] 主动服务已停止");
        Ok(())
    }

    /// 获取当前服务状态
    fn status(&self) -> ServiceStatus {
        if self.is_running.load(Ordering::SeqCst) {
            ServiceStatus::Running
        } else {
            ServiceStatus::Stopped
        }
    }

    /// 获取完整健康信息
    fn health(&self) -> ServiceHealth {
        ServiceHealth {
            name: self.name().to_string(),
            status: self.status(),
            uptime_secs: None, // TODO: 在集成阶段添加启动时间追踪
            last_error: None,
            metrics: serde_json::json!({
                "tick_count": self.tick_count.load(Ordering::SeqCst),
                "action_count": self.action_count.load(Ordering::SeqCst),
                "no_message_count": self.no_message_count.load(Ordering::SeqCst),
                "is_waiting_user_input": self.is_waiting_user_input.load(Ordering::SeqCst),
            }),
        }
    }
}

// ─── 单元测试 ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::Mutex;

    use crate::memory_graph::store::MemoryGraphStore;

    /// 创建测试用的 ProactiveStorage（内存数据库）
    fn make_test_storage() -> Arc<ProactiveStorage> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS proactive_messages (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                generated_at TEXT NOT NULL,
                trigger_reason TEXT NOT NULL DEFAULT '',
                tools_used TEXT NOT NULL DEFAULT '[]',
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            );",
        )
        .unwrap();
        Arc::new(ProactiveStorage {
            conn: Mutex::new(conn),
        })
    }

    /// 创建测试用的 ProviderService
    fn make_test_provider_service() -> Arc<ProviderService> {
        // 使用临时目录创建 ProviderService
        let tmp = tempfile::tempdir().unwrap();
        Arc::new(ProviderService::new(tmp.path()).unwrap())
    }

    /// 创建测试用的 MemoryGraphStore（内存数据库）
    fn make_test_memory_graph_store() -> Arc<MemoryGraphStore> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        let conn = Arc::new(std::sync::Mutex::new(conn));
        Arc::new(MemoryGraphStore::new(conn))
    }

    /// 创建完整的测试服务实例
    fn make_test_service(config: ProactiveConfig, infra: Arc<InfraService>) -> ProactiveService {
        let storage = make_test_storage();
        let scenario_manager = Arc::new(ScenarioManager::new());
        let execution_log_collector = Arc::new(ExecutionLogCollector::new());
        let multimodal_queue = Arc::new(MultimodalQueue::new());
        let provider_service = make_test_provider_service();
        let memory_graph_store = make_test_memory_graph_store();
        ProactiveService::new(
            config,
            infra,
            storage,
            scenario_manager,
            execution_log_collector,
            multimodal_queue,
            provider_service,
            None, // memu_client
            memory_graph_store,
            None, // app_handle — 测试环境不需要 Tauri IPC
            Arc::new(std::sync::Mutex::new(Connection::open_in_memory().unwrap())),
            Arc::new(std::sync::Mutex::new(
                crate::agent::gep::repository::GeneRepository::new(
                    std::env::temp_dir().join("uclaw_gep_test")
                ).expect("GeneRepository for test")
            )),
            crate::memubot_config::GeneEvolutionConfig::default(),
            // Phase 3/4/5 runtime knobs bundled — every flag on,
            // StubAnalyzer installed, default budget. Matches
            // MemoryOsConfig::default() so tests reflect production
            // happy-path behavior.
            MemoryOsRuntimeConfig::for_tests(),
            // Bundle 22 — data_dir for test runs lands in a process-
            // unique temp dir so concurrent tests don't collide on
            // _auto_extracted/ writes.
            std::env::temp_dir().join("uclaw_proactive_test"),
            // Bundle 23 — fresh SkillsRegistry for tests.
            std::sync::Arc::new(tokio::sync::RwLock::new(
                crate::skills::SkillsRegistry::new(),
            )),
        )
    }

    /// 测试：创建服务并获取初始状态
    #[tokio::test]
    async fn test_new_service_initial_state() {
        let infra = Arc::new(InfraService::new());
        let config = ProactiveConfig::default();
        let service = make_test_service(config, infra);
        let status = service.get_proactive_status().await;

        assert_eq!(status.state, ProactiveState::Stopped);
        assert!(!status.is_running);
        assert_eq!(status.tick_count, 0);
        assert_eq!(status.action_count, 0);
        assert_eq!(status.context_message_count, 0);
    }

    /// 测试：disabled 时 start 不会启动后台任务
    #[tokio::test]
    async fn test_start_when_disabled() {
        let infra = Arc::new(InfraService::new());
        let config = ProactiveConfig {
            enabled: false,
            ..Default::default()
        };
        let service = make_test_service(config, infra);
        service.start().await.unwrap();

        // 应该仍然是 Stopped 状态（因为 disabled 直接跳过）
        assert!(!service.is_running.load(Ordering::SeqCst));
    }

    /// 测试：start + stop 生命周期
    #[tokio::test]
    async fn test_start_and_stop_lifecycle() {
        let infra = Arc::new(InfraService::new());
        let config = ProactiveConfig {
            enabled: true,
            interval_ms: 100, // 短间隔便于测试
            ..Default::default()
        };
        let service = make_test_service(config, infra);

        // 启动
        service.start().await.unwrap();
        assert!(service.is_running.load(Ordering::SeqCst));
        assert_eq!(*service.state.read().await, ProactiveState::Idle);

        // 等待一会儿让 tick 运行
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // 停止
        service.stop().await.unwrap();
        assert!(!service.is_running.load(Ordering::SeqCst));
        assert_eq!(*service.state.read().await, ProactiveState::Stopped);
    }

    /// 测试：ManagedService trait 实现
    #[tokio::test]
    async fn test_managed_service_trait() {
        let infra = Arc::new(InfraService::new());
        let config = ProactiveConfig::default();
        let service = make_test_service(config, infra);

        assert_eq!(service.name(), "proactive");
        assert_eq!(service.status(), ServiceStatus::Stopped);

        let health = service.health();
        assert_eq!(health.name, "proactive");
        assert_eq!(health.status, ServiceStatus::Stopped);
    }

    /// 测试：上下文监听接收消息
    #[tokio::test]
    async fn test_context_listener_receives_messages() {
        let infra = Arc::new(InfraService::new());
        let config = ProactiveConfig {
            enabled: true,
            interval_ms: 10_000, // 长间隔避免 tick 干扰
            ..Default::default()
        };
        let service = make_test_service(config, infra.clone());
        service.start().await.unwrap();

        // 等待监听任务启动
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // 通过消息总线发送消息
        infra
            .publish_incoming("test", "你好", serde_json::json!({}))
            .await;

        // 等待消息被处理
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // 验证上下文窗口（测试用 metadata 不含 conversation_id，消息进入 "default" session）
        let ctx = service.context_messages.read().await;
        let default_window = ctx.get("default").expect("default session window should exist");
        assert_eq!(default_window.messages.len(), 1);
        assert_eq!(default_window.messages[0].content, "你好");

        // has_new_context 应该为 true
        assert!(service.has_new_context.load(Ordering::SeqCst));

        // new_message_count 应该递增
        assert_eq!(service.new_message_count.load(Ordering::SeqCst), 1);

        service.stop().await.unwrap();
    }

    /// 测试：set_user_input
    #[tokio::test]
    async fn test_set_user_input() {
        let infra = Arc::new(InfraService::new());
        let config = ProactiveConfig::default();
        let service = make_test_service(config, infra);

        // 模拟等待状态
        service
            .is_waiting_user_input
            .store(true, Ordering::SeqCst);

        service.set_user_input("确认执行".to_string()).await;

        assert!(!service.is_waiting_user_input.load(Ordering::SeqCst));
        let resp = service.user_input_response.read().await;
        assert_eq!(resp.as_deref(), Some("确认执行"));
    }

    /// 辅助：向 store 插入一个带有 cited_count 的 Procedure 节点，返回 node id
    fn insert_learned_skill(
        store: &MemoryGraphStore,
        space_id: &str,
        name: &str,
        cited_count: u64,
    ) -> String {
        use crate::memory_graph::models::{MemoryNode, MemoryNodeKind};
        let now = chrono::Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        let meta = serde_json::json!({
            "skill_type": "learned",
            "cited_count": cited_count,
            "enabled": true,
        });
        let node = MemoryNode {
            id: id.clone(),
            space_id: space_id.to_string(),
            kind: MemoryNodeKind::Procedure,
            title: name.to_string(),
            metadata: Some(meta),
            created_at: now.clone(),
            updated_at: now,
        };
        store.create_node(&node).expect("insert_learned_skill: create_node failed");
        id
    }

    /// 辅助：读取节点当前的 cited_count
    fn read_cited_count(store: &MemoryGraphStore, node_id: &str) -> u64 {
        let node = store
            .get_node(node_id)
            .expect("read_cited_count: get_node failed")
            .expect("read_cited_count: node not found");
        node.metadata
            .as_ref()
            .and_then(|m| m.get("cited_count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    }

    /// Test: cited=20 → floor(20 * 0.95) = 19 after one decay call
    #[test]
    fn decay_applies_to_learned_skills() {
        let store = make_test_memory_graph_store();
        let node_id = insert_learned_skill(&store, "default", "my_skill", 20);

        let updated = decay_cited_counts(&store, "default").expect("decay failed");
        assert_eq!(updated, 1, "expected 1 node updated");
        assert_eq!(read_cited_count(&store, &node_id), 19, "expected cited_count=19");
    }

    /// Test: cited=0 → no-op (floor(0 * 0.95) == 0, skip write)
    #[test]
    fn decay_floors_at_zero_not_negative() {
        let store = make_test_memory_graph_store();
        let node_id = insert_learned_skill(&store, "default", "zero_skill", 0);

        let updated = decay_cited_counts(&store, "default").expect("decay failed");
        assert_eq!(updated, 0, "expected 0 nodes updated (no-op)");
        assert_eq!(read_cited_count(&store, &node_id), 0, "cited_count should remain 0");
    }
}
