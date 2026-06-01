use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tauri::Manager;
use sha2::{Digest, Sha256};

use crate::settings::UserSettings;
use crate::agent::session::SessionManager;
use crate::config::LlmConfig;
use crate::notifications::SharedNotificationManager;
use crate::background::SharedBackgroundManager;
use crate::memory::MemoryStore;
use crate::skills::SkillsRegistry;
use crate::mcp::SharedMcpManager;
use crate::channels::ChannelManager;
use crate::providers::service::ProviderService;
use crate::safety::SafetyManager;
use crate::memu::client::MemUClient;
use crate::memu::bridge::MemUBridge;
use crate::memory_graph::store::MemoryGraphStore;
use crate::infra::InfraService;
use crate::proactive::ProactiveService;
use crate::services::ServiceManager;
use crate::observability::MetricsService;
use crate::memubot_config::MemubotConfig;

// ─── Pending Approvals ──────────────────────────────────────────────────

/// Result of an approval decision from the user.
#[derive(Debug, Clone, Default)]
pub struct ApprovalResult {
    pub approved: bool,
    pub always_allow: bool,
    pub tool_name: Option<String>,
    /// Path approval: "once" | "session" | "deny" (only set when payload kind=="path")
    pub path_scope: Option<String>,
    /// Path approval: which absolute paths to grant for "session" scope
    pub paths: Option<Vec<String>>,
}

/// Manages pending tool approval requests using oneshot channels.
/// When a tool needs approval, we store a oneshot::Sender keyed by tool_id.
/// The agent loop awaits on the corresponding Receiver.
/// When the user responds, approve_tool_call sends the result through the Sender.
pub struct PendingApprovals {
    pending: std::sync::Mutex<HashMap<String, tokio::sync::oneshot::Sender<ApprovalResult>>>,
}

impl PendingApprovals {
    pub fn new() -> Self {
        Self {
            pending: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Register a pending approval and return a receiver to await the result.
    pub fn register(&self, tool_id: String) -> tokio::sync::oneshot::Receiver<ApprovalResult> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().unwrap().insert(tool_id, tx);
        rx
    }

    /// Resolve a pending approval. Returns true if the approval was found and resolved.
    pub fn resolve(&self, tool_id: &str, result: ApprovalResult) -> bool {
        if let Some(tx) = self.pending.lock().unwrap().remove(tool_id) {
            tx.send(result).is_ok()
        } else {
            false
        }
    }
}

/// Result of an ask_user response.
#[derive(Debug, Clone)]
pub struct AskUserResult {
    /// Map of question_index → answer string
    pub answers: std::collections::HashMap<String, serde_json::Value>,
}

/// Manages pending ask_user requests from agent to user.
/// Mirrors PendingApprovals — oneshot per request_id.
pub struct PendingAskUsers {
    pending: std::sync::Mutex<HashMap<String, tokio::sync::oneshot::Sender<AskUserResult>>>,
}

impl PendingAskUsers {
    pub fn new() -> Self {
        Self { pending: std::sync::Mutex::new(HashMap::new()) }
    }

    pub fn register(&self, request_id: String) -> tokio::sync::oneshot::Receiver<AskUserResult> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().unwrap().insert(request_id, tx);
        rx
    }

    pub fn resolve(&self, request_id: &str, result: AskUserResult) -> bool {
        if let Some(tx) = self.pending.lock().unwrap().remove(request_id) {
            tx.send(result).is_ok()
        } else {
            false
        }
    }
}

/// Decision from the user on an exit_plan_mode request.
#[derive(Debug, Clone)]
pub enum ExitPlanDecision {
    /// User accepted; switch session SafetyMode to Supervised and proceed.
    AcceptAndAuto,
    /// User accepted but wants to stay in Plan; agent may run only the
    /// pre-declared allowed_prompts.
    AcceptKeepPlan,
    /// User rejected; agent receives feedback as tool error.
    Reject { feedback: String },
}

#[derive(Debug, Clone)]
pub struct ExitPlanResult {
    pub decision: ExitPlanDecision,
}

pub struct PendingExitPlans {
    pending: std::sync::Mutex<HashMap<String, tokio::sync::oneshot::Sender<ExitPlanResult>>>,
}

impl PendingExitPlans {
    pub fn new() -> Self {
        Self { pending: std::sync::Mutex::new(HashMap::new()) }
    }
    pub fn register(&self, request_id: String) -> tokio::sync::oneshot::Receiver<ExitPlanResult> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().unwrap().insert(request_id, tx);
        rx
    }
    pub fn resolve(&self, request_id: &str, result: ExitPlanResult) -> bool {
        if let Some(tx) = self.pending.lock().unwrap().remove(request_id) {
            tx.send(result).is_ok()
        } else {
            false
        }
    }
}

// ─── Pi Sprint 2 ③ — Dual Interactive Queues ────────────────────────────────

/// A pair of queues shared between Tauri command producers and the agent
/// loop's ChatDelegate for a single session.
#[derive(Clone, Default)]
pub struct AgentQueues {
    pub steering: crate::agent::queues::SteeringQueue,
    pub follow_up: crate::agent::queues::FollowUpQueue,
}

// ────────────────────────────────────────────────────────────────────────────

/// Global application state managed by Tauri
pub struct AppState {
    pub data_dir: PathBuf,
    pub config_path: PathBuf,
    pub llm_config_path: PathBuf,
    pub db_path: PathBuf,
    pub workspace_root: PathBuf,
    pub settings: Arc<RwLock<UserSettings>>,
    pub llm_config: Arc<RwLock<LlmConfig>>,
    pub db_ready: bool,
    pub db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    pub session_manager: Arc<RwLock<SessionManager>>,

    // Bundle 27-A2 (pull-model recovery) — recovery.rs writes the
    // interrupted-recovery payload here on boot when Bundle 27-C
    // reports Unclean. The UI's AgentHeartbeatBanner queries
    // `consume_pending_recovery` on mount and renders the banner if
    // the conversation_id matches. Push-via-`agent:interrupted-recovered`
    // event was unreliable in dev mode because the event fires ~500ms
    // after boot, possibly before the React listener is registered.
    pub pending_recovery: Arc<std::sync::Mutex<Option<serde_json::Value>>>,

    // B0: Infrastructure
    pub notifications: SharedNotificationManager,
    pub background_tasks: SharedBackgroundManager,

    // B2: Infrastructure modules
    pub memory_store: Arc<MemoryStore>,
    pub skills_registry: Arc<RwLock<SkillsRegistry>>,
    pub mcp_manager: SharedMcpManager,
    pub channel_manager: Arc<RwLock<ChannelManager>>,
    pub im_channel_manager: Arc<crate::channels::manager::ImChannelManager>,
    pub im_session_registry: Arc<crate::channels::session_registry::ImSessionRegistry>,

    // Providers
    pub provider_service: Arc<ProviderService>,

    // Safety
    pub safety_manager: Arc<tokio::sync::RwLock<SafetyManager>>,

    // Tool approval
    pub pending_approvals: Arc<PendingApprovals>,

    // ask_user pending requests
    pub pending_ask_users: Arc<PendingAskUsers>,

    // exit_plan_mode pending requests
    pub pending_exit_plans: Arc<PendingExitPlans>,

    // memU memory service (None if Python is unavailable)
    pub memu_client: Option<Arc<MemUClient>>,

    // Memory graph store for Steward memory system
    pub memory_graph_store: Arc<MemoryGraphStore>,

    /// PR1 of 阶段 4 — registry mapping backend name → adapter. Empty
    /// HashMap in PR1; PR2-PR14 add concrete adapters one by one. See
    /// `docs/superpowers/specs/2026-05-29-stage4-memory-adapter-design.md`.
    pub memory_adapters:
        std::sync::Arc<std::collections::HashMap<String, std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>>>,

    /// PR1 of 阶段 4 — name of the backend used when callers don't
    /// specify one explicitly. Starts as `"bucket_seal"` (the eventual
    /// primary) even though no adapter is registered yet — when PR9
    /// registers `BucketSealAdapter`, the default is immediately live.
    pub default_memory_backend: std::sync::Arc<std::sync::RwLock<String>>,

    /// AI Wiki synthesizer — drives `wiki_artifacts(kind="overview")`
    /// regeneration. Memory OS Foundation Phase 3 ships
    /// `wiki_synth::StubSynthesizer` as the default; future PRs swap in
    /// a real Anthropic / OpenAI client without touching the IPC or
    /// scenario code paths.
    pub wiki_synthesizer: Arc<dyn crate::memory_graph::wiki_synth::WikiSynthesizer>,

    /// LLM lint analyzer — used by the Phase 5 memory_lint scenario
    /// to judge whether candidate findings (hub stubs, stale summaries,
    /// contradictions) are real. Phase 5 ships
    /// `memory_lint::StubAnalyzer` as the default; swap with a real
    /// client when Phase 5 has soaked in production.
    pub lint_analyzer: Arc<dyn crate::proactive::scenarios::memory_lint::LintAnalyzer>,

    /// EntityPage synthesizer — used by the Phase 6.2
    /// `memory_entity_page_synthesize_now` IPC + future auto-synth
    /// hooks. Defaults to `StubEntitySynthesizer`; flipping
    /// `memory_os.entity_synthesizer_enabled` to true installs the
    /// LLM-backed `RealEntitySynthesizer` at boot.
    pub entity_synthesizer:
        Arc<dyn crate::proactive::scenarios::entity_synthesizer::EntitySynthesizer>,

    /// Phase 7.4 — opt-in fs watcher over the brain dir. `None` when
    /// `memory_os.brain_watcher_enabled` is false at boot OR when the
    /// watcher failed to start (logged and ignored — manual Sync
    /// still works). The handle keeps the watcher + debounce worker
    /// alive for the lifetime of AppState.
    pub brain_watcher:
        std::sync::Mutex<Option<crate::memory_graph::brain_watcher::BrainWatcherHandle>>,

    /// Sprint 1 — openhuman-style stability_detector + PROFILE.md
    /// pipeline. Producer (chat-turn extractor) pushes into
    /// `learning_buffer`; ProactiveService scheduler tick drains
    /// every 30 min via `learning_scheduler` and refreshes
    /// `facet_cache`. Agent prompt builder reads `facet_cache` via
    /// `learning::prompt_section::UserProfileSection::render`.
    pub learning_buffer: Arc<crate::learning::candidate::Buffer>,
    pub learning_scheduler: Arc<crate::learning::scheduler::LearningScheduler>,
    pub facet_cache: Arc<crate::learning::cache::FacetCache>,
    /// Sprint 2.0 — LLM adapter shared with the chat-turn extractor.
    /// `Some` only when `memory_os.learning_enabled = true` AND
    /// `learning_llm_daily_token_budget > 0`. Same `MemoryOsLlmClient`
    /// type used by wiki/lint/entity — routes through the active
    /// provider configured in `provider_service`. `None` makes the
    /// dispatcher skip the LLM layer (regex still runs).
    pub learning_llm:
        Option<Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>,

    // ─── Phased Boot: 新增服务 ───────────────────────────────────────
    /// 中央消息总线
    pub infra_service: Arc<InfraService>,
    /// 服务管理器（统一管理所有后台服务的启停和健康监控）
    pub service_manager: Arc<ServiceManager>,
    /// 指标采集服务
    pub metrics_service: Arc<MetricsService>,
    /// memubot 功能配置（wrapped in RwLock so set_* Tauri commands can mutate + persist）
    pub memubot_config: Arc<tokio::sync::RwLock<MemubotConfig>>,

    /// Active agentic session cancellation tokens, keyed by conversation_id.
    /// Used by stop_agent_session to cancel a running loop.
    pub running_sessions: Arc<tokio::sync::Mutex<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>>,

    /// Per-session dual interactive queues (Pi Sprint 2 ③).
    /// Producers (Tauri commands) and the agent loop's ChatDelegate share
    /// the same `AgentQueues` pair via `agent_queues_for(session_id)`.
    pub agent_queues: Arc<std::sync::Mutex<std::collections::HashMap<String, AgentQueues>>>,

    /// Bundle 20 — per-session cached composed `memory_context` from
    /// the most recent **completed** background recall. The agent
    /// recall pipeline (Bundle 6) puts recall in `tokio::spawn` with
    /// a 400ms deadline on the main path; in production memU's
    /// retrieve_with_context routinely exceeds that, so the receiver
    /// is dropped before the task can send. The composed context was
    /// then thrown away — `delegate.set_memory_context` never ran,
    /// LLM never saw the recall, and downstream telemetry (Bundle
    /// 16-B `[M2-D]`) never fired.
    ///
    /// Bundle 20 fixes the leak by stashing every successfully
    /// composed context here keyed by `conversation_id`. The next
    /// turn's main path uses this as a fallback when its own 400ms
    /// deadline misses — net effect is "recall primes the NEXT
    /// turn", which preserves Bundle 6's TTFT win while delivering
    /// memory_context on every turn ≥ 2.
    ///
    /// Memory cost is bounded: each entry is a single composed
    /// String (~1-3 KB typical), keyed by session id. Stale sessions
    /// stay in the map until process exit — that's bounded by the
    /// number of conversations the user has, which is small.
    pub recall_ctx_cache: Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,

    /// Browser context manager — per-session Chrome lifecycle for Browser Agent v2.
    pub browser_context_manager: Arc<crate::browser::BrowserContextManager>,

    /// Aggregated Browser Runtime status source for Splash, Settings, and
    /// later supervised browser call sites.
    pub browser_runtime_status_service: Arc<crate::browser::BrowserRuntimeStatusService>,

    /// Live browser identity task registry — tracks identity-backed browser
    /// tasks in this app process so revocation can drain and checkpoint them.
    pub browser_identity_task_registry: Arc<crate::browser::identity_tasks::BrowserIdentityTaskRegistry>,

    // Evaluation harness
    pub trajectory_store: Arc<crate::agent::trajectory::TrajectoryStore>,
    pub tool_budget: Arc<crate::agent::tool_budget::ToolBudgetManager>,

    // Slice 1 — per-task TokenBudgetSnapshot collector. Populated by
    // the agent loop on every `delegate.on_usage()` tick; read by the
    // M2-J UI via `get_latest_token_budget`. Cheap to clone (internal
    // Arc<RwLock>).
    pub token_budget_collector: crate::agent::telemetry::TokenBudgetCollector,

    /// C2-Dirac-B2 — per-conversation latest `ComposeStats` from the
    /// ContextManager wire-up. Populated by the agent loop's
    /// `effective_system_prompt` each turn; read by the M2-J UI via
    /// `get_compose_stats`. Cheap to clone (internal Arc<RwLock>).
    pub compose_stats_collector: crate::agent::context_manager::ComposeStatsCollector,

    /// Files rail service — owns the filesystem watcher for the WorkspaceRail UI.
    pub files_rail_service: Arc<crate::files_rail::FilesRailService>,

    /// Humane Automation runtime — manages spec activation, subscriptions, and
    /// the § 7.3 command surface.  Registered into ServiceManager in main.rs
    /// Stage 3; constructed here so Tauri commands can borrow it via AppState.
    pub runtime_service: Arc<crate::automation::runtime::AppRuntimeService>,

    /// ProactiveService (None if proactive is disabled or init failed;
    /// populated asynchronously in main.rs Stage 3 via interior RwLock).
    pub proactive_service: Arc<tokio::sync::RwLock<Option<Arc<ProactiveService>>>>,

    // [R5] symphony_service 字段已删（symphony_graph 旧后端删除）。

    /// App launch instant — used to compute uptime_secs in diagnostics.
    pub boot_time: std::time::Instant,

    /// Knowledge ingestion service — drives ingest_files / ingest_url Tauri commands.
    pub ingestion: Arc<crate::ingestion::IngestionService>,

    /// gbrain MCP server ID stored after seed_bundled_gbrain succeeds.
    /// "gbrain" when seeded; None when bun/gbrain binaries are missing.
    pub gbrain_mcp_id: Arc<std::sync::Mutex<Option<String>>>,

    /// Sprint 2.2.5b — last-known outcome of `ensure_bundled_gbrain_initialized`.
    /// Stage 3 updates this slot before/after the init call so
    /// `get_system_diagnostics` can surface an actionable status in the
    /// Settings → 系统 tab (instead of users only finding init failures
    /// by tail-ing logs).
    pub gbrain_init_status: Arc<std::sync::Mutex<crate::mcp::GbrainInitStatus>>,

    /// 共享 Hook 总线 — ToolDispatcher 经此 observe-only 发射 PreToolUse/PostToolUse;
    /// Sprint 3 ② 在同一实例上注册订阅者。
    pub hook_bus: std::sync::Arc<crate::agent::hook_bus::HookBus>,

    /// Pi-lightweight single-handle replacement for the 4-Registry pattern.
    /// Created empty at boot; populated by builtin registrations + (P3-4+)
    /// subprocess plugin loader. See:
    /// docs/superpowers/specs/2026-05-28-stage3-agentapi-handle-design.md
    pub agent_api: Arc<crate::agent::api::AgentApi>,

    /// PR1 of Tier 1+2+3 batch — per-conversation cancellation tokens.
    /// Populated at `send_message`/`send_agent_message` entry; fired by
    /// the `cancel_conversation` Tauri command.
    pub cancellation_registry: Arc<crate::agent::cancellation_registry::CancellationRegistry>,

    // [R1 接线] pi_sessions WIP 暂移除：rusqlite 归零、pi 接入 workspace 后，
    // 会话句柄改由 crates/uclaw-pi-engine 的 PiEngine/SessionRegistry 持有，
    // 不再直接放 AppState。见 docs/R1-wiring-plan.md。
    // pub pi_sessions: Arc<tokio::sync::Mutex<std::collections::HashMap<String, pi::sdk::AgentSessionHandle>>>,
}

/// 启动默认 Hook 策略。本 slice 为 Allow-all(空 rules)—— 行为零变化。
/// 从 settings/DB 加载规则留后续(用户配置范围外)。
fn default_hook_policy() -> crate::policy_eval::PolicySpec {
    crate::policy_eval::PolicySpec::new()
}

impl AppState {
    pub fn new(app_handle: &tauri::AppHandle) -> Result<Self, crate::error::Error> {
        let data_dir = uclaw_utils_home::uclaw_home_pathbuf()
            .map_err(|_| crate::error::Error::Internal("Cannot find home directory".into()))?;

        std::fs::create_dir_all(&data_dir).ok();
        // Ensure bash temp dir exists for RollingTailBuffer overflow files.
        if let Ok(home) = uclaw_utils_home::uclaw_home_pathbuf() {
            let _ = std::fs::create_dir_all(home.join("temp"));
        }
        tracing::info!(data_dir = %data_dir.display(), "Initializing application state");

        let config_path = data_dir.join("config.json");
        let llm_config_path = data_dir.join("llm_config.json");
        let db_path = data_dir.join("uclaw.db");
        let workspace_root = dirs::document_dir()
            .ok_or_else(|| crate::error::Error::Internal("Cannot find Documents directory".into()))?
            .join("workground");
        std::fs::create_dir_all(&workspace_root).ok();

        let settings = UserSettings::load(&config_path)?;
        let llm_config = LlmConfig::load(&llm_config_path)?;
        let db_ready = db_path.exists();

        // Initialize database and session manager
        let db = Arc::new(std::sync::Mutex::new(
            crate::db::manager::Database::open(&db_path)?.into_inner(),
        ));
        tracing::info!(db_path = %db_path.display(), "Database opened");

        // Run migrations
        if let Ok(conn) = db.lock() {
            if let Err(e) = crate::db::migrations::run(&conn) {
                tracing::error!(error = %e, "DATABASE MIGRATION FAILED — app may be in inconsistent state");
            }
        }

        let session_manager = SessionManager::new(db.clone());

        // B0: Notification manager
        let notifications = Arc::new(tokio::sync::Mutex::new(
            crate::notifications::NotificationManager::new(app_handle.clone()),
        ));

        // B0: Background task manager
        let background_tasks = Arc::new(tokio::sync::Mutex::new(
            crate::background::BackgroundTaskManager::new(),
        ));

        // B2: Memory store
        let memory_store = Arc::new(MemoryStore::new(db.clone()));
        memory_store.ensure_table();

        // Resolve the Tauri resource dir up front — it's used to wire the
        // Bundled skills tier below, and (later in B-Stage) to find the
        // embedded Python runtime.
        let resource_dir = app_handle.path().resource_dir().ok();

        // [Scheme C] Replicate memory_schema.json to ~/.uclaw/memory_schema.json
        {
            let schema_dest = data_dir.join("memory_schema.json");
            let mut schema_src = None;

            // 1. Check Tauri resource_dir (Release bundle)
            if let Some(ref rd) = resource_dir {
                let bundled_schema = rd.join("memory_schema.json");
                if bundled_schema.exists() {
                    schema_src = Some(bundled_schema);
                }
            }
            // 2. Check development path (cargo run / cargo tauri dev)
            if schema_src.is_none() {
                let dev_schema = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("resources")
                    .join("memory_schema.json");
                if dev_schema.exists() {
                    schema_src = Some(dev_schema);
                }
            }

            if let Some(src) = schema_src {
                tracing::info!(
                    src = %src.display(),
                    dest = %schema_dest.display(),
                    "[Scheme C] Replicating memory_schema.json to data directory"
                );
                if let Err(e) = std::fs::copy(&src, &schema_dest) {
                    tracing::error!(
                        error = %e,
                        "[Scheme C] Failed to replicate memory_schema.json to data directory"
                    );
                }
            } else {
                tracing::warn!("[Scheme C] memory_schema.json not found in resources or dev paths — replication skipped");
            }
        }

        // B2: Skills registry
        //
        // Tier order matters: Bundled → User → Project. Later tiers
        // **shadow** earlier ones on name collision — that's the
        // intentional Fork affordance. A user-authored `tdd` in
        // `~/.uclaw/skills/tdd/SKILL.md` overrides the bundled `tdd` of
        // the same name without the user having to disable the bundled
        // copy first.
        let mut skills_reg = SkillsRegistry::new();

        // Bundled — comes from the Tauri resource dir, which `tauri.conf.json`
        // populates from the repo's top-level `skills/` directory. In dev
        // mode (`cargo tauri dev`) Tauri mirrors the dir into
        // `target/debug/skills/` so it works without a release bundle.
        if let Some(rd) = resource_dir.as_ref() {
            let bundled_skills = rd.join("skills");
            if bundled_skills.exists() {
                tracing::info!(
                    bundled_skills = %bundled_skills.display(),
                    "Registering bundled skills scan dir"
                );
                skills_reg.add_scan_dir(bundled_skills, crate::skills::SkillProvenance::Bundled);
            } else {
                tracing::debug!(
                    expected = %bundled_skills.display(),
                    "No bundled skills dir found in resource dir (running outside a bundle?)"
                );
            }
        }

        match crate::browser::playwright_skills::ensure_managed_playwright_skills(&data_dir) {
            Ok(managed_playwright_skills) => {
                tracing::info!(
                    skills_dir = %managed_playwright_skills.display(),
                    "Registering managed Playwright built-in skills scan dir"
                );
                skills_reg.add_scan_dir(
                    managed_playwright_skills,
                    crate::skills::SkillProvenance::Bundled,
                );
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "Failed to seed managed Playwright built-in skills"
                );
            }
        }

        // User — survives uClaw upgrades, holds forks and user-authored skills.
        let user_skills_dir = data_dir.join("skills");
        std::fs::create_dir_all(&user_skills_dir).ok();
        skills_reg.add_scan_dir(user_skills_dir, crate::skills::SkillProvenance::User);

        // Marketplace — recovery path: any _marketplace/<slug>/ dirs that exist on disk
        // (e.g. after a backup restore or across cold starts) are registered here so
        // SkillsRegistry stays in sync with the FS even if DB rows were already present.
        let marketplace_root = data_dir.join("skills").join("_marketplace");
        if let Ok(entries) = std::fs::read_dir(&marketplace_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    skills_reg.add_scan_dir(path, crate::skills::SkillProvenance::Marketplace);
                }
            }
        }

        // Project — dev-mode-only fallback. In a bundled app `current_dir()`
        // is the launch dir which won't contain a skills tree, so this is
        // effectively a no-op in production. The Bundled tier covers the
        // production case.
        let project_skills = std::env::current_dir()
            .map(|d| d.join("skills"))
            .unwrap_or_default();
        if project_skills.exists() {
            skills_reg.add_scan_dir(project_skills, crate::skills::SkillProvenance::Project);
        }
        // Initial discovery
        let discovered = skills_reg.discover();
        if !discovered.is_empty() {
            tracing::info!("Discovered {} skill(s) at startup", discovered.len());
        }
        let skills_registry = Arc::new(RwLock::new(skills_reg));

        // B2: MCP manager
        let mcp_manager = Arc::new(RwLock::new(
            crate::mcp::McpManager::new(&data_dir),
        ));

        // B2: Channel manager
        let channel_manager = Arc::new(RwLock::new(ChannelManager::new()));

        // IM channel manager and session registry
        let im_session_registry = Arc::new(
            crate::channels::session_registry::ImSessionRegistry::new(db.clone())
        );
        let im_channel_manager = Arc::new(
            crate::channels::manager::ImChannelManager::new(
                db.clone(),
                im_session_registry.clone(),
                app_handle.clone(),
            )
        );

        // Providers
        let provider_service = Arc::new(ProviderService::new(&data_dir)?);

        // Safety
        let safety_manager = Arc::new(tokio::sync::RwLock::new(SafetyManager::new(&data_dir)));

        // Tool approval
        let pending_approvals = Arc::new(PendingApprovals::new());

        // ask_user pending requests
        let pending_ask_users = Arc::new(PendingAskUsers::new());

        // exit_plan_mode pending requests
        let pending_exit_plans = Arc::new(PendingExitPlans::new());

        // memU integration (degraded mode if Python unavailable). The Tauri
        // resource dir was already resolved above for the Bundled skills tier.
        let memu_client = Self::try_init_memu(&data_dir, resource_dir.as_deref());

        // Eagerly start the memU bridge on Tauri's long-lived async runtime.
        //
        // Do not create a one-off Tokio runtime here: MemUBridge::spawn_subprocess
        // spawns stdout/stderr reader tasks and owns Tokio child pipes. If those
        // tasks are attached to a temporary runtime, the runtime is dropped as
        // soon as this health check returns, leaving the bridge with IO handles
        // bound to a shutting-down reactor.
        if let Some(ref client) = memu_client {
            let eager_client = Arc::clone(client);
            tauri::async_runtime::spawn(async move {
                match eager_client.health_check().await {
                    Ok(status) => tracing::info!("memU bridge health: {}", status),
                    Err(e) => tracing::warn!(
                        "memU bridge health check failed: {}; will retry later",
                        e
                    ),
                }
            });
        }

        // Memory graph store
        let memory_graph_store = {
            let store = MemoryGraphStore::new(db.clone());
            store.ensure_tables();
            let mgs = Arc::new(store);
            // Environment memory — scan once at startup for the default space
            crate::memory_graph::environment::persist_environment(
                &mgs,
                "default",
                &workspace_root,
            );
            mgs
        };

        // ─── Stage 1 完成：基础初始化 ─────────────────────────────────

        // Stage 1 补充：加载 MemubotConfig
        let memubot_config = MemubotConfig::load(&data_dir);
        tracing::info!("MemubotConfig loaded (memorization={}, proactive={}, local_api={}, power={})",
            memubot_config.memorization.enabled,
            memubot_config.proactive.enabled,
            memubot_config.local_api.enabled,
            memubot_config.power.prevent_sleep,
        );

        // Memory OS Foundation Phase 2 — apply the auto_link feature flag
        // to the store now that we know what the user configured. The
        // store defaults this to `true` in `MemoryGraphStore::new`; we
        // override here so a user-set `false` in memubot_config.json takes
        // effect from the very first create_version call.
        memory_graph_store.set_auto_link_enabled(memubot_config.memory_os.auto_link_enabled);
        tracing::info!(
            "Memory OS flags applied: entity_page={}, auto_link={}",
            memubot_config.memory_os.entity_page_enabled,
            memubot_config.memory_os.auto_link_enabled,
        );

        // Evaluation harness
        let trajectory_store = Arc::new(crate::agent::trajectory::TrajectoryStore::new(db.clone()));
        let tool_budget = Arc::new(crate::agent::tool_budget::ToolBudgetManager::new(&data_dir));

        // ─── Stage 2：核心服务 ─────────────────────────────────────────
        let infra_service = Arc::new(InfraService::new());
        tracing::info!("InfraService created");

        let metrics_service = Arc::new(MetricsService::new());
        tracing::info!("MetricsService created");

        let service_manager = Arc::new(ServiceManager::new());
        tracing::info!("ServiceManager created");

        // Files rail service — created here, registered into ServiceManager in main.rs Stage 3.
        let files_rail_service = Arc::new(crate::files_rail::FilesRailService::new(app_handle.clone()));
        let browser_context_manager = Arc::new(crate::browser::BrowserContextManager::new(app_handle.clone()));
        let browser_runtime_status_service = Arc::new(crate::browser::BrowserRuntimeStatusService::new(
            browser_context_manager.clone(),
        ));

        // AppRuntimeService — constructed here so it is available to Tauri commands via
        // AppState.  main.rs Stage 3 calls `state.runtime_service.clone()` to register it
        // into ServiceManager (no double-construction needed).
        let automation_memory_root = data_dir.join("automation_memory");
        let _ = std::fs::create_dir_all(&automation_memory_root);
        let runtime_service = {
            use crate::automation::runtime::AppRuntimeService;
            use crate::automation::sources::{
                CustomSource, FileSource, RssSource, ScheduleSource,
                WebhookSource, WebpageSource, WecomSource,
            };
            use crate::automation::memory::MemoryStore as AutomationMemoryStore;
            AppRuntimeService::new(
                db.clone(),
                Arc::new(ScheduleSource::new()),
                Arc::new(FileSource::new()),
                Arc::new(WebhookSource::with_global_registry()),
                Arc::new(WebpageSource::new()),
                Arc::new(RssSource::new()),
                Arc::new(WecomSource::new()),
                Arc::new(CustomSource::new()),
                infra_service.clone(),
                Arc::new(AutomationMemoryStore::new(automation_memory_root)),
                provider_service.clone(),
                Some(app_handle.clone()),
                Some(channel_manager.clone()),
                Some(browser_context_manager.clone()),
            )
        };

        // ─── Stage 3：注册后台服务到 ServiceManager（在后台异步完成启动）
        // 这些注册操作需要 async，因此在 setup 中通过 spawn 完成。
        // 此处仅构建 AppState，实际注册和启动在 main.rs setup 中执行。

        // Memory OS Phase 6b — pick wiki synthesizer impl based on
        // `wiki_real_synthesizer_enabled`. Stub stays the safe default
        // (no LLM calls, deterministic markdown); flipping the flag
        // routes overview regen through the configured active provider
        // via MemoryOsLlmClient. Restart is required for the swap to
        // take effect because the trait object is held by AppState.
        let wiki_synthesizer: Arc<dyn crate::memory_graph::wiki_synth::WikiSynthesizer> =
            if memubot_config.memory_os.wiki_real_synthesizer_enabled {
                use crate::memory_graph::memory_os_llm::MemoryOsLlmClient;
                use crate::memory_graph::wiki_synth::RealWikiSynthesizer;
                let llm = Arc::new(MemoryOsLlmClient::new(
                    provider_service.clone(),
                    db.clone(),
                ));
                tracing::info!("Memory OS Phase 6b: RealWikiSynthesizer installed");
                Arc::new(RealWikiSynthesizer::new(llm))
            } else {
                tracing::info!("Memory OS Phase 6b: StubSynthesizer (default — flip wiki_real_synthesizer_enabled to opt in)");
                Arc::new(crate::memory_graph::wiki_synth::StubSynthesizer)
            };

        // Memory OS Phase 6c — same pattern for the lint analyzer.
        // Stub stays the safe default; real analyzer plus the existing
        // Phase 5 daily-token cap (`memory_lint_daily_token_budget` +
        // `cost_records.model LIKE 'memory_lint%'`) are the active
        // safety mechanisms once flipped on.
        let lint_analyzer: Arc<dyn crate::proactive::scenarios::memory_lint::LintAnalyzer> =
            if memubot_config.memory_os.lint_real_analyzer_enabled {
                use crate::memory_graph::memory_os_llm::MemoryOsLlmClient;
                use crate::proactive::scenarios::memory_lint::RealLintAnalyzer;
                let llm = Arc::new(MemoryOsLlmClient::new(
                    provider_service.clone(),
                    db.clone(),
                ));
                tracing::info!("Memory OS Phase 6c: RealLintAnalyzer installed");
                Arc::new(RealLintAnalyzer::new(llm))
            } else {
                tracing::info!("Memory OS Phase 6c: StubAnalyzer (default — flip lint_real_analyzer_enabled to opt in)");
                Arc::new(crate::proactive::scenarios::memory_lint::StubAnalyzer)
            };

        // Memory OS Phase 6.2 — entity synthesizer trait object. Same
        // Stub/Real branch as 6b/6c; controls whether the manual
        // synthesize-now IPC + future auto-synth hooks call the LLM.
        let entity_synthesizer: Arc<
            dyn crate::proactive::scenarios::entity_synthesizer::EntitySynthesizer,
        > = if memubot_config.memory_os.entity_synthesizer_enabled {
            use crate::memory_graph::memory_os_llm::MemoryOsLlmClient;
            use crate::proactive::scenarios::entity_synthesizer::RealEntitySynthesizer;
            let llm = Arc::new(MemoryOsLlmClient::new(
                provider_service.clone(),
                db.clone(),
            ));
            tracing::info!("Memory OS Phase 6.2: RealEntitySynthesizer installed");
            Arc::new(RealEntitySynthesizer::new(llm))
        } else {
            tracing::info!(
                "Memory OS Phase 6.2: StubEntitySynthesizer (default — flip entity_synthesizer_enabled to opt in)"
            );
            Arc::new(crate::proactive::scenarios::entity_synthesizer::StubEntitySynthesizer)
        };

        // Memory OS Phase 7.4 — opt-in fs watcher. Resolves the default
        // brain root if no override is configured. Errors degrade
        // gracefully: the manual Sync button (Phase 7.2) still works.
        let brain_watcher_handle: Option<
            crate::memory_graph::brain_watcher::BrainWatcherHandle,
        > = if memubot_config.memory_os.brain_watcher_enabled
            && memubot_config.memory_os.entity_page_enabled
        {
            match crate::memory_graph::brain_io::BrainExportConfig::default_brain_root() {
                Some(root) => {
                    let store_clone = memory_graph_store.clone();
                    match crate::memory_graph::brain_watcher::start_brain_watcher(
                        store_clone,
                        root.clone(),
                        "default".to_string(),
                        crate::memory_graph::brain_watcher::DEFAULT_DEBOUNCE_MS,
                    ) {
                        Ok(h) => {
                            tracing::info!(
                                root = %root.display(),
                                "Memory OS Phase 7.4: brain_watcher started"
                            );
                            Some(h)
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                "Memory OS Phase 7.4: brain_watcher failed to start (manual Sync still works)"
                            );
                            None
                        }
                    }
                }
                None => {
                    tracing::warn!(
                        "Memory OS Phase 7.4: brain_watcher enabled but no Documents dir; not started"
                    );
                    None
                }
            }
        } else {
            None
        };

        // Memory OS Sprint 1.10 — learning pipeline bootstrap.
        // Producer side (chat-turn extractor) pushes candidates into
        // `learning_buffer`; ProactiveService scheduler tick (every
        // 60 ticks = 30 min) drains via `learning_scheduler` and
        // refreshes `facet_cache`. Always constructed regardless of
        // the flag so the IPC list/dismiss endpoints work even when
        // the periodic rebuild is disabled.
        let learning_buffer = Arc::new(
            crate::learning::candidate::Buffer::new(2048),
        );
        let learning_facet_store = Arc::new(
            crate::learning::scheduler::FacetStore::new(db.clone()),
        );
        let learning_scheduler = Arc::new(
            crate::learning::scheduler::LearningScheduler::new(
                learning_facet_store,
                learning_buffer.clone(),
            ),
        );
        let facet_cache = Arc::new(crate::learning::cache::FacetCache::new());
        tracing::info!(
            learning_enabled = memubot_config.memory_os.learning_enabled,
            "Memory OS Sprint 1: facet store + buffer + cache initialized"
        );

        // Memory OS Sprint 2.0 — LLM adapter for the chat-turn extractor.
        // Shares the same MemoryOsLlmClient used by wiki/lint/entity, so a
        // single provider configuration covers all four Memory OS users.
        // Built only when the learning flag is on AND the LLM budget is
        // non-zero — both are necessary because:
        // - flag off → extractor itself is skipped, no point holding a
        //   client we won't call
        // - budget 0 → regex-only operation, no LLM call ever happens
        let learning_llm: Option<
            Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>,
        > = if memubot_config.memory_os.learning_enabled
            && memubot_config.memory_os.learning_llm_daily_token_budget > 0
        {
            use crate::memory_graph::memory_os_llm::MemoryOsLlmClient;
            let llm = Arc::new(MemoryOsLlmClient::new(
                provider_service.clone(),
                db.clone(),
            ));
            tracing::info!(
                budget = memubot_config.memory_os.learning_llm_daily_token_budget,
                "Memory OS Sprint 2.0: learning LLM extractor installed"
            );
            Some(llm)
        } else {
            tracing::info!(
                "Memory OS Sprint 2.0: learning LLM extractor disabled (regex-only mode)"
            );
            None
        };

        // Knowledge ingestion service — constructed after provider_service and mcp_manager
        // are ready. Drives the ingest_* Tauri commands added in Task 8.
        let ingestion = Arc::new(crate::ingestion::IngestionService::new(
            provider_service.clone(),
            mcp_manager.clone(),
        ));

        tracing::info!("Application state initialized successfully (phased boot)");

        // Sprint 3 ② — build HookBus, register PolicySpecSubscriber (Allow-all default),
        // then Arc-wrap. HookBus has no interior mutability so registration must happen
        // before Arc::new.
        let hook_bus = {
            let mut bus = crate::agent::hook_bus::HookBus::new();
            bus.register(std::sync::Arc::new(
                crate::policy_eval::PolicySpecSubscriber::new(default_hook_policy()),
            ))
            .map_err(|e| crate::error::Error::Internal(format!("hook subscriber register: {e:?}")))?;
            std::sync::Arc::new(bus)
        };

        // P3-2.5 — build AgentApi, register 17 builtin tool descriptors,
        // then Arc-wrap. AgentApi has no interior mutability so registration
        // must happen before Arc::new.
        // P3-3.5 — wire ProviderService + HookBus into AgentApi at boot.
        // P3-4.5 — discover + register third-party plugins from $DATA_DIR/plugins/.
        let agent_api = {
            let mut api = crate::agent::api::AgentApi::new();
            crate::agent::tools::builtin_descriptors::register_all(&mut api);

            // P3-4: scan $DATA_DIR/plugins/ and register each plugin's contributions.
            // All errors are graceful — boot always succeeds with an empty plugin list.
            // Missing plugins dir returns Ok(vec![]) from PluginDiscovery::discover().
            let plugins_root = data_dir.join("plugins");
            let discovery = crate::plugins::PluginDiscovery::new(&plugins_root);
            match discovery.discover() {
                Ok(results) => {
                    for result in results {
                        match result {
                            Ok(loaded) => {
                                match crate::plugins::PluginRegistrar::register(&mut api, &loaded) {
                                    Ok(summary) => {
                                        tracing::info!(
                                            plugin_id = %summary.plugin_id,
                                            tools = ?summary.tools_registered,
                                            "[P3-4] plugin loaded"
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "[P3-4] plugin registration failed");
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "[P3-4] plugin discovery failed");
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "[P3-4] plugins directory scan failed");
                }
            }

            api.set_provider_service(provider_service.clone());
            api.set_hook_bus(hook_bus.clone());
            std::sync::Arc::new(api)
        };

        // Build the memory adapter registry — one entry per concrete adapter.
        // Task 2 of PR2 (阶段 4): LegacyKvAdapter wraps memory_store so the
        // same SQLite KV+FTS store is reachable via the trait registry.
        // Task 2 of PR3 (阶段 4): LegacyStewardAdapter wraps memory_graph_store
        // so the same graph store is also reachable via the registry.
        let legacy_kv_adapter = std::sync::Arc::new(
            crate::memory_adapter::LegacyKvAdapter::new(memory_store.clone()),
        ) as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;
        let legacy_steward_adapter = std::sync::Arc::new(
            crate::memory_adapter::LegacyStewardAdapter::new(memory_graph_store.clone()),
        ) as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;
        let mut memory_adapters_map: std::collections::HashMap<
            String,
            std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>,
        > = std::collections::HashMap::new();
        memory_adapters_map.insert(legacy_kv_adapter.name().to_string(), legacy_kv_adapter);
        memory_adapters_map.insert(legacy_steward_adapter.name().to_string(), legacy_steward_adapter);

        // Task 7 of PR9 (阶段 4): BucketSealAdapter ships the full bucket-seal
        // pipeline (PR5-8) as the first non-wrap MemoryAdapter. It boots with
        // InertEmbedder + InertSummariser (zero-vector embeddings, placeholder
        // summaries); a later PR swaps in real Ollama/LLM backends. Registered
        // under "bucket_seal" so callers can opt in via an explicit backend or
        // the "bucket_seal:" namespace prefix. The global default is NOT flipped
        // here — see `default_memory_backend` below.
        let bucket_seal_dir = data_dir.join("bucket_seal");
        std::fs::create_dir_all(&bucket_seal_dir).ok();
        let bucket_seal_db_path = bucket_seal_dir.join("chunks.db");
        let bucket_seal_content_root = bucket_seal_dir.join("content");
        std::fs::create_dir_all(&bucket_seal_content_root).ok();

        let bucket_seal_store = std::sync::Arc::new(
            crate::memory_bucket_seal::BucketSealStore::open(&bucket_seal_db_path)
                .expect("open bucket_seal chunks.db"),
        );
        bucket_seal_store
            .ensure_schema()
            .expect("apply bucket_seal SCHEMA");

        let bucket_seal_embedder: std::sync::Arc<dyn crate::memory_bucket_seal::Embedder> =
            std::sync::Arc::new(crate::memory_bucket_seal::InertEmbedder::new());
        let bucket_seal_summariser: std::sync::Arc<dyn crate::memory_bucket_seal::Summariser> =
            std::sync::Arc::new(crate::memory_bucket_seal::InertSummariser::new());

        let bucket_seal_adapter = std::sync::Arc::new(crate::memory_bucket_seal::BucketSealAdapter::new(
            bucket_seal_store,
            bucket_seal_content_root,
            bucket_seal_embedder,
            bucket_seal_summariser,
        ))
            as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;

        memory_adapters_map.insert(bucket_seal_adapter.name().to_string(), bucket_seal_adapter);
        let memory_adapters = std::sync::Arc::new(memory_adapters_map);

        Ok(Self {
            data_dir,
            config_path,
            llm_config_path,
            db_path,
            workspace_root,
            settings: Arc::new(RwLock::new(settings)),
            llm_config: Arc::new(RwLock::new(llm_config)),
            db_ready,
            db,
            session_manager: Arc::new(RwLock::new(session_manager)),
            // Bundle 27-A2 — pending recovery payload, set on boot by
            // recovery.rs when Bundle 27-C reports Unclean shutdown.
            pending_recovery: Arc::new(std::sync::Mutex::new(None)),
            notifications,
            background_tasks,
            memory_store,
            skills_registry,
            mcp_manager,
            channel_manager,
            im_channel_manager,
            im_session_registry,
            provider_service,
            safety_manager,
            pending_approvals,
            pending_ask_users,
            pending_exit_plans,
            memu_client,
            memory_graph_store,
            memory_adapters,
            // bucket_seal is REGISTERED (PR9) and reachable via an explicit
            // backend or the "bucket_seal:" namespace prefix. The global default
            // stays "legacy_kv" deliberately: BucketSealAdapter currently uses
            // InertEmbedder/InertSummariser (keyword-only, preview-scoped recall),
            // so flipping every unspecified-backend caller onto it is deferred to
            // a follow-up PR that lands a real embedder + summariser.
            default_memory_backend: std::sync::Arc::new(std::sync::RwLock::new(
                "legacy_kv".to_string(),
            )),
            // Picked above based on `memory_os.wiki_real_synthesizer_enabled`
            // (Phase 6b). Defaults to StubSynthesizer; flipping the flag
            // routes overview regen through the active LLM provider.
            wiki_synthesizer,
            // Picked above based on `memory_os.lint_real_analyzer_enabled`
            // (Phase 6c). Defaults to StubAnalyzer; flipping the flag
            // routes lint candidates through the active LLM provider.
            lint_analyzer,
            // Phase 6.2 entity synthesizer trait object (Stub or Real),
            // picked above. Drives the manual Synthesize button IPC.
            entity_synthesizer,
            // Phase 7.4 — Some when the fs watcher started successfully.
            brain_watcher: std::sync::Mutex::new(brain_watcher_handle),
            learning_buffer,
            learning_scheduler,
            facet_cache,
            learning_llm,
            infra_service,
            service_manager,
            metrics_service,
            memubot_config: Arc::new(tokio::sync::RwLock::new(memubot_config)),
            running_sessions: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            agent_queues: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            // Bundle 20 — see `recall_ctx_cache` field doc.
            recall_ctx_cache: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            browser_context_manager,
            browser_runtime_status_service,
            browser_identity_task_registry: Arc::new(
                crate::browser::identity_tasks::BrowserIdentityTaskRegistry::default(),
            ),
            trajectory_store,
            tool_budget,
            token_budget_collector: crate::agent::telemetry::TokenBudgetCollector::new(),
            compose_stats_collector: crate::agent::context_manager::ComposeStatsCollector::new(),
            files_rail_service,
            runtime_service,
            proactive_service: Arc::new(tokio::sync::RwLock::new(None)),
            boot_time: std::time::Instant::now(),
            ingestion,
            gbrain_mcp_id: Arc::new(std::sync::Mutex::new(None)),
            gbrain_init_status: Arc::new(std::sync::Mutex::new(
                crate::mcp::GbrainInitStatus::default(),
            )),
            hook_bus,
            agent_api,
            cancellation_registry: Arc::new(
                crate::agent::cancellation_registry::CancellationRegistry::new(),
            ),
            // [R1 接线] pi_sessions WIP 暂移除（见上方字段注释 / R1-wiring-plan.md）。
            // pi_sessions: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        })
    }

    /// Try to initialize the memU Python bridge.
    /// Returns None if Python is not available (degraded mode).
    fn try_init_memu(data_dir: &std::path::Path, resource_dir: Option<&std::path::Path>) -> Option<Arc<MemUClient>> {
        // 1. Locate memu_bridge.py
        let script_path = Self::find_bridge_script(resource_dir, data_dir)?;

        // 2. Locate Python
        let python_path = Self::find_python(resource_dir)?;

        tracing::info!(
            python = %python_path,
            script = %script_path.display(),
            "Initializing memU bridge"
        );

        // 3. Build LLM environment variables from providers.json (active provider)
        let mut llm_env = Vec::new();
        if let Some((api_key, base_url, model)) = Self::load_active_provider_config(data_dir) {
            if !api_key.is_empty() {
                llm_env.push(("MEMU_LLM_API_KEY".to_string(), api_key));
            }
            if !base_url.is_empty() {
                llm_env.push(("MEMU_LLM_BASE_URL".to_string(), base_url));
            }
            if !model.is_empty() {
                llm_env.push(("MEMU_LLM_CHAT_MODEL".to_string(), model));
            }
        }

        // Default to auto FastEmbed mode (use local embedding when fastembed is available)
        llm_env.push(("MEMU_EMBED_MODE".to_string(), "auto".to_string()));

        // Sprint 2.2 followon #4 — pin the FastEmbed model the bridge
        // loads, configurable via set_embedding_config. Loaded from
        // memubot_config.json if present; falls back to the schema
        // default otherwise (matches what set_embedding_config writes
        // on first save).
        let fastembed_model = crate::memubot_config::MemubotConfig::load(data_dir)
            .embedding_endpoint
            .fastembed_model;
        llm_env.push(("FASTEMBED_MODEL".to_string(), fastembed_model));

        let bridge = Arc::new(MemUBridge::new(python_path, script_path, data_dir.to_path_buf(), llm_env));
        let client = Arc::new(MemUClient::new(bridge));
        Some(client)
    }

    /// Load active provider's LLM config from providers.json.
    /// Returns (api_key, base_url, model_id) if found.
    fn load_active_provider_config(data_dir: &std::path::Path) -> Option<(String, String, String)> {
        let providers_path = data_dir.join("providers.json");
        let content = std::fs::read_to_string(&providers_path).ok()?;
        let config: serde_json::Value = serde_json::from_str(&content).ok()?;

        // Get active_model's provider_id
        let active = config.get("active_model")?;
        let provider_id = active.get("provider_id")?.as_str()?;
        let model_id = active.get("model_id").and_then(|v| v.as_str()).unwrap_or("");

        // Find the matching provider in the providers array
        let providers = config.get("providers")?.as_array()?;
        for p in providers {
            if p.get("provider_id").and_then(|v| v.as_str()) == Some(provider_id) {
                let api_key = p.get("api_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let base_url = p.get("base_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                tracing::info!("Loaded memU LLM config from providers.json (provider: {})", provider_id);
                return Some((api_key, base_url, model_id.to_string()));
            }
        }
        None
    }

    fn is_packaged_resource_dir(resource_dir: &std::path::Path) -> bool {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        !resource_dir.starts_with(manifest_dir)
    }

    /// Find the memU bridge script, checking bundled resource dir, dev path, and data dir.
    fn find_bridge_script(resource_dir: Option<&std::path::Path>, data_dir: &std::path::Path) -> Option<PathBuf> {
        // 1. Check Tauri resource_dir (Release bundle)
        if let Some(res_dir) = resource_dir {
            let bundled = res_dir.join("memu_bridge.py");
            if bundled.exists() {
                tracing::info!("Found bundled bridge script at {}", bundled.display());
                return Some(bundled);
            }
            if Self::is_packaged_resource_dir(res_dir) {
                tracing::warn!(
                    expected = %bundled.display(),
                    "Packaged memU bridge script missing; refusing to fall back to dev/data paths"
                );
                return None;
            }
        }

        // 2. Check development path (cargo run / cargo tauri dev)
        let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("memu")
            .join("memu_bridge.py");
        if dev_path.exists() {
            tracing::debug!("Found dev bridge script at {}", dev_path.display());
            return Some(dev_path);
        }

        // 3. Check data_dir (~/.uclaw/memu_bridge.py)
        let data_script = data_dir.join("memu_bridge.py");
        if data_script.exists() {
            tracing::debug!("Found bridge script in data dir at {}", data_script.display());
            return Some(data_script);
        }

        tracing::warn!("memU bridge script not found in any location");
        None
    }

    /// Find a suitable Python interpreter, preferring embedded Python in resource_dir.
    fn find_python(resource_dir: Option<&std::path::Path>) -> Option<String> {
        // 1. Check embedded Python (Release mode). Validate executability,
        // not just existence: resource copying can materialize a broken
        // `python3` launcher while the versioned `python3.13` binary is fine.
        if let Some(res_dir) = resource_dir {
            let bin_dir = if cfg!(target_os = "windows") {
                res_dir.join("python")
            } else {
                res_dir.join("python").join("bin")
            };
            let candidates = if cfg!(target_os = "windows") {
                vec![bin_dir.join("python.exe")]
            } else {
                vec![
                    bin_dir.join("python3.13"),
                    bin_dir.join("python"),
                    bin_dir.join("python3"),
                ]
            };
            if let Some(path_str) = Self::first_working_python(candidates, "embedded") {
                return Some(path_str);
            }
            if Self::is_packaged_resource_dir(res_dir) {
                tracing::warn!(
                    resource_dir = %res_dir.display(),
                    "Packaged Python runtime missing or unusable; refusing to fall back to dev/system Python"
                );
                return None;
            }
        }

        // 2. Check dev pyembed Python (cargo tauri dev)
        let dev_pyembed_bin = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("pyembed")
            .join("python")
            .join("bin");
        if let Some(path_str) = Self::first_working_python(
            vec![
                dev_pyembed_bin.join("python3.13"),
                dev_pyembed_bin.join("python"),
                dev_pyembed_bin.join("python3"),
            ],
            "dev pyembed",
        ) {
            return Some(path_str);
        }

        // 3. Fall back to system Python (development mode)
        let candidates = ["python3.13", "python3", "python"];
        for candidate in &candidates {
            if let Ok(output) = std::process::Command::new(candidate)
                .arg("--version")
                .output()
            {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout);
                    tracing::debug!("Found system Python: {} -> {}", candidate, version.trim());
                    return Some(candidate.to_string());
                }
            }
        }
        None
    }

    fn first_working_python(
        candidates: Vec<std::path::PathBuf>,
        source_label: &str,
    ) -> Option<String> {
        for candidate in candidates {
            if !candidate.exists() {
                continue;
            }
            let output = std::process::Command::new(&candidate)
                .arg("--version")
                .output();
            match output {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let version = if stdout.trim().is_empty() {
                        stderr.trim()
                    } else {
                        stdout.trim()
                    };
                    let path_str = candidate.to_string_lossy().into_owned();
                    tracing::info!(
                        source = source_label,
                        python = %path_str,
                        version,
                        "Found working Python"
                    );
                    return Some(path_str);
                }
                Ok(output) => {
                    tracing::warn!(
                        source = source_label,
                        python = %candidate.display(),
                        status = ?output.status.code(),
                        "Skipping unusable Python candidate"
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        source = source_label,
                        python = %candidate.display(),
                        error = %error,
                        "Skipping Python candidate that failed to launch"
                    );
                }
            }
        }
        None
    }

    /// gbrain Sprint 2.1 — find the bundled `bun` binary.
    /// Same find-resource-then-fall-back-to-dev shape as `find_python`.
    /// Returns `None` if neither location has the binary; caller (Stage
    /// 3 seed step in main.rs) treats `None` as "skip gbrain seed".
    ///
    /// Bundle 15 (dev-mode only): macOS 26.4+ Gatekeeper SIGKILLs the
    /// unsigned `target/debug/bun` copy that Tauri's resource-map step
    /// drops in during `cargo tauri dev`. In a packaged `.app` the
    /// bundled `bun` is notarized as part of the bundle and runs fine,
    /// so we MUST NOT prefer system Bun there — packaged behavior is
    /// unchanged.
    ///
    /// In dev mode (resource_dir under CARGO_MANIFEST_DIR OR None), we
    /// try a system Bun first (`~/.bun/bin/bun`, Homebrew, /usr/local).
    /// Falls back to the resource copy, then `bunembed/bun`, so
    /// machines without system Bun still work after running
    /// scripts/setup-bun-runtime.sh.
    pub fn find_bun_path(resource_dir: Option<&std::path::Path>) -> Option<PathBuf> {
        let in_packaged_resource_dir = resource_dir
            .map(Self::is_packaged_resource_dir)
            .unwrap_or(false);

        // 1. Dev-only — try system Bun first to dodge macOS Gatekeeper
        //    SIGKILL on unsigned dev binaries. Skipped entirely in
        //    packaged mode so we never depend on user-installed Bun in
        //    a shipped .app.
        if !in_packaged_resource_dir {
            if let Some(bun) =
                Self::first_working_bun(Self::system_bun_candidates(), "system")
            {
                return Some(bun);
            }
        }

        // 2. Bundled — Tauri resource dir contains `bun` (per tauri.conf.json
        //    "bunembed/bun": "bun" mapping)
        if let Some(res_dir) = resource_dir {
            if let Some(bun) = Self::first_working_bun(vec![res_dir.join("bun")], "resource") {
                return Some(bun);
            }
            if in_packaged_resource_dir {
                tracing::warn!(
                    expected = %res_dir.join("bun").display(),
                    "Packaged Bun missing or unusable; refusing to fall back to dev Bun"
                );
                return None;
            }
        }
        // 3. Dev — repo's bunembed/bun (after running setup-bun-runtime.sh)
        let dev_bun = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("bunembed")
            .join("bun");
        if let Some(bun) = Self::first_working_bun(vec![dev_bun], "dev") {
            return Some(bun);
        }
        tracing::warn!("Bun binary not found in system, bundle, or dev location");
        None
    }

    /// Bundle 15 — common system locations for an officially-installed
    /// Bun. Order matters: `~/.bun/bin/bun` first (the `bun.sh` curl
    /// installer's default), then Homebrew (`/opt/homebrew/bin/bun` on
    /// Apple Silicon, `/usr/local/bin/bun` on Intel + Linux).
    ///
    /// Returns paths whether they exist or not — `first_working_bun`
    /// filters non-existent entries cheaply via `candidate.exists()`.
    fn system_bun_candidates() -> Vec<std::path::PathBuf> {
        Self::system_bun_candidates_for_home(std::env::var_os("HOME").as_deref())
    }

    /// Bundle 15 — testable form of `system_bun_candidates` that lets a
    /// test supply a deterministic HOME without touching process env.
    fn system_bun_candidates_for_home(
        home: Option<&std::ffi::OsStr>,
    ) -> Vec<std::path::PathBuf> {
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        if let Some(home) = home {
            candidates.push(
                std::path::PathBuf::from(home)
                    .join(".bun")
                    .join("bin")
                    .join("bun"),
            );
        }
        candidates.push(std::path::PathBuf::from("/opt/homebrew/bin/bun"));
        candidates.push(std::path::PathBuf::from("/usr/local/bin/bun"));
        // Linux package managers / nix profile
        candidates.push(std::path::PathBuf::from("/usr/bin/bun"));
        candidates
    }

    fn first_working_bun(
        candidates: Vec<std::path::PathBuf>,
        source_label: &str,
    ) -> Option<std::path::PathBuf> {
        for candidate in candidates {
            if !candidate.exists() {
                continue;
            }
            let output = std::process::Command::new(&candidate)
                .arg("--version")
                .output();
            match output {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let version = if stdout.trim().is_empty() {
                        stderr.trim()
                    } else {
                        stdout.trim()
                    };
                    tracing::info!(
                        source = source_label,
                        bun = %candidate.display(),
                        version,
                        "Found working Bun"
                    );
                    return Some(candidate);
                }
                Ok(output) => {
                    tracing::warn!(
                        source = source_label,
                        bun = %candidate.display(),
                        status = ?output.status.code(),
                        "Skipping unusable Bun candidate"
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        source = source_label,
                        bun = %candidate.display(),
                        error = %error,
                        "Skipping Bun candidate that failed to launch"
                    );
                }
            }
        }
        None
    }

    /// gbrain Sprint 2.1 — find the gbrain CLI entry point.
    /// Mac-side Sprint 2.0 verification confirmed `src/cli.ts` is the
    /// stdio MCP entry; `bun src/cli.ts serve` is the spawn command.
    /// Same fallback shape as `find_bun_path`.
    pub fn find_gbrain_entry(resource_dir: Option<&std::path::Path>) -> Option<PathBuf> {
        // 1. Bundled — Tauri resource maps `gbrain-source` → `gbrain`
        if let Some(res_dir) = resource_dir {
            let bundled = res_dir.join("gbrain").join("src").join("cli.ts");
            if bundled.exists() {
                tracing::info!("Found bundled gbrain CLI at {}", bundled.display());
                return Some(bundled);
            }
            if Self::is_packaged_resource_dir(res_dir) {
                tracing::warn!(
                    expected = %bundled.display(),
                    "Packaged gbrain CLI missing; refusing to fall back to dev gbrain source"
                );
                return None;
            }
        }
        // 2. Dev — repo's gbrain-source/src/cli.ts
        let dev_entry = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("gbrain-source")
            .join("src")
            .join("cli.ts");
        if dev_entry.exists() {
            tracing::debug!("Found dev gbrain CLI at {}", dev_entry.display());
            return Some(dev_entry);
        }
        tracing::warn!("gbrain CLI entry not found in bundle or dev location");
        None
    }

    /// Sprint 2.2 — write self-describing launcher script + paths manifest
    /// to `gbrain_home/`. Lets any tool (other Cowork sessions, debug
    /// scripts, CLI users) invoke the bundled gbrain without knowing
    /// whether uClaw is dev-mode or installed as a release `.app`.
    ///
    /// `gbrain_home` is the path you want gbrain's `GBRAIN_HOME` env var
    /// to point at (typically `<data_dir>/gbrain` for uClaw — caller
    /// resolves the `data_dir → gbrain_home` mapping). Passing it directly
    /// lets the caller's owned `PathBuf` be captured into an async closure
    /// without `state_ref` lifetime issues (see the main.rs Stage 3 spawn).
    ///
    /// Overwrites on every boot so paths always reflect the current install.
    /// Best-effort caller: returns `io::Result` so the boot path can log a
    /// warning on failure (e.g. read-only data_dir) without aborting the
    /// gbrain seed.
    ///
    /// Files written:
    /// - `<gbrain_home>/run.sh` — POSIX launcher (chmod 0o755 on Unix).
    ///   Sets `GBRAIN_HOME=<gbrain_home>` and execs `<bun> <entry> "$@"`.
    /// - `<gbrain_home>/paths.json` — machine-readable manifest with
    ///   uclaw_version, absolute paths, and a generation timestamp.
    pub fn write_gbrain_launcher_files(
        gbrain_home: &std::path::Path,
        bun_path: &std::path::Path,
        entry_path: &std::path::Path,
    ) -> std::io::Result<()> {
        std::fs::create_dir_all(gbrain_home)?;

        // run.sh — POSIX launcher. Bakes in absolute paths and sets
        // GBRAIN_HOME so the gbrain CLI resolves its layout
        // (`.gbrain/brain.pglite/`, `.gbrain/config.json`) under our
        // chosen home rather than the user's default `~/.gbrain/`.
        let run_sh = gbrain_home.join("run.sh");
        let script = format!(
            "#!/usr/bin/env bash\n\
             # Auto-generated by uClaw at every boot — do not hand-edit.\n\
             # Usage:\n\
             #   ~/.uclaw/gbrain/run.sh init --pglite --yes\n\
             #   ~/.uclaw/gbrain/run.sh serve\n\
             #   ~/.uclaw/gbrain/run.sh recall \"some query\"\n\
             export GBRAIN_HOME={home_q}\n\
             exec {bun_q} {entry_q} \"$@\"\n",
            home_q = shell_quote_path(gbrain_home),
            bun_q = shell_quote_path(bun_path),
            entry_q = shell_quote_path(entry_path),
        );
        std::fs::write(&run_sh, script)?;

        // chmod +x — best-effort; if it fails the user can chmod manually.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                &run_sh,
                std::fs::Permissions::from_mode(0o755),
            );
        }

        // paths.json — machine-readable manifest. The brain layout
        // (`.gbrain/brain.pglite/` + `.gbrain/config.json`) is what
        // `gbrain init --pglite` actually produces, NOT the dead
        // `pgdata/` vestige from pre-PR-205 days.
        let brain_dir = gbrain_home.join(".gbrain").join("brain.pglite");
        let config_json = gbrain_home.join(".gbrain").join("config.json");
        let manifest = serde_json::json!({
            "uclaw_version": env!("CARGO_PKG_VERSION"),
            "bun_path": bun_path,
            "gbrain_entry": entry_path,
            "gbrain_home": gbrain_home,
            "brain_dir": brain_dir,
            "config_json": config_json,
            "generated_at_ms": chrono::Utc::now().timestamp_millis(),
        });
        let manifest_str = serde_json::to_string_pretty(&manifest)
            .map_err(std::io::Error::other)?;
        std::fs::write(gbrain_home.join("paths.json"), manifest_str)?;
        Ok(())
    }
}

// ─── gbrain Helpers ────────────────────────────────────────────────────────

/// Minimal POSIX shell quoter for filesystem paths. Wraps in single
/// quotes and escapes any embedded single quotes via the `'\''` idiom.
/// Sufficient for paths (which can contain spaces, dashes, etc.) but
/// NOT general shell metacharacters — paths don't legitimately contain
/// the shell special chars that single-quoting can't handle.
fn shell_quote_path(p: &std::path::Path) -> String {
    let s = p.to_string_lossy();
    if s.is_empty() {
        return "''".to_string();
    }
    let escaped = s.replace('\'', "'\\''");
    format!("'{}'", escaped)
}

// ─── Files Rail Helpers ────────────────────────────────────────────────────

/// Stable per-path hash used inside `MountRoot.id` for attached directories.
///
/// We **must not** use the path's position in `attached_dirs` because the
/// frontend's `fileTreeAtomFamily` is keyed by `mount_id` and caches state
/// for the life of the renderer process. Index-based IDs ("...:0", "...:1")
/// silently shuffle whenever the user detaches anything, so the next mount
/// at that index inherits the previous mount's cached tree → the rail shows
/// stale filenames until a manual refresh.
///
/// SHA-256 truncated to 16 hex chars (64 bits) is plenty of uniqueness for
/// a single user's attached dirs and keeps IDs short enough to log.
fn mount_id_hash(path: &Path) -> String {
    let bytes = path.to_string_lossy();
    let digest = Sha256::digest(bytes.as_bytes());
    let mut out = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        use std::fmt::Write as _;
        let _ = write!(out, "{:02x}", byte);
    }
    out
}

impl AppState {
    /// Resolve the user-visible active workspace root for runtime-scoped
    /// services. Falls back to the default workground root when the active
    /// workspace setting is missing, stale, or points at an empty legacy path.
    pub fn active_workspace_root_or_default(&self) -> PathBuf {
        self.db
            .lock()
            .ok()
            .map(|conn| resolve_active_workspace_root_from_conn(&conn, &self.workspace_root))
            .unwrap_or_else(|| self.workspace_root.clone())
    }

    /// Get (or create) the queue pair for a session. Producers (Tauri commands)
    /// and the consumer (ChatDelegate) share the same pair through this.
    pub fn agent_queues_for(&self, session_id: &str) -> AgentQueues {
        self.agent_queues
            .lock()
            .unwrap()
            .entry(session_id.to_string())
            .or_default()
            .clone()
    }

    /// Whether this session has an active agent run.
    pub async fn is_session_running(&self, session_id: &str) -> bool {
        self.running_sessions.lock().await.contains_key(session_id)
    }

    pub async fn files_rail_list_mounts(
        &self,
        session_id: Option<String>,
    ) -> Result<Vec<crate::files_rail::MountRoot>, crate::error::Error> {
        use crate::files_rail::{MountKind, MountRoot};
        let mut out: Vec<MountRoot> = Vec::new();

        // Resolve the session's space_id + attached_dirs in a single short
        // lock so we don't hold it across awaits later.
        let (space_id_for_session, session_attached_dirs): (Option<String>, Vec<String>) =
            if let Some(sid) = session_id.as_deref() {
                match self.db.lock() {
                    Ok(conn) => {
                        let space: Option<String> = conn
                            .query_row(
                                "SELECT space_id FROM agent_sessions WHERE id = ?1",
                                rusqlite::params![sid],
                                |r| r.get::<_, String>(0),
                            )
                            .ok();
                        let attached_json: Option<String> = conn
                            .query_row(
                                "SELECT attached_dirs FROM agent_sessions WHERE id = ?1",
                                rusqlite::params![sid],
                                |r| r.get::<_, String>(0),
                            )
                            .ok();
                        let attached = attached_json
                            .as_deref()
                            .and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
                            .unwrap_or_default();
                        (space, attached)
                    }
                    Err(_) => (None, Vec::new()),
                }
            } else {
                (None, Vec::new())
            };

        // Workspace mount — when a session_id resolves to a space with a custom
        // path, use that; otherwise fall back to the default ~/Documents/workground.
        let workspace_path: Option<std::path::PathBuf> = if let Some(space) = space_id_for_session
            .as_deref()
        {
            self.db.lock().ok().and_then(|conn| {
                conn.query_row(
                    "SELECT path FROM spaces WHERE id = ?1",
                    rusqlite::params![space],
                    |r| r.get::<_, Option<String>>(0),
                )
                .ok()
                .flatten()
                .filter(|s| !s.trim().is_empty())
                .map(std::path::PathBuf::from)
            })
        } else {
            None
        };
        let workspace_root = workspace_path.unwrap_or_else(|| {
            dirs::document_dir()
                .map(|d| d.join("workground"))
                .unwrap_or_default()
        });
        if workspace_root.exists() {
            let id = match space_id_for_session.as_deref() {
                Some(s) => format!("workspace:{}", s),
                None => "workspace:default".into(),
            };
            out.push(MountRoot {
                id,
                label: "工作区文件".into(),
                path: workspace_root,
                kind: MountKind::Workspace,
                editable: true,
            });
        }

        // W4d Issue 4 fix: workspace-level attached_dirs become MountRoots too.
        // Without this, dirs attached via attachWorkspaceDirectory (the standard
        // UI flow) never appear in the rail, which blocks testing of the
        // preview-write approval flow.
        let workspace_attached_dirs: Vec<String> = {
            let space_id_opt = space_id_for_session.clone().or_else(|| {
                // No session → fall back to the globally-active workspace.
                self.db.lock().ok().and_then(|conn| {
                    conn.query_row(
                        "SELECT value FROM settings WHERE key = 'active_workspace_id'",
                        [],
                        |r| r.get::<_, String>(0),
                    )
                    .ok()
                })
            });
            match space_id_opt {
                Some(space_id) => self
                    .db
                    .lock()
                    .ok()
                    .and_then(|conn| {
                        conn.query_row(
                            "SELECT attached_dirs FROM spaces WHERE id = ?1",
                            rusqlite::params![space_id],
                            |r| r.get::<_, Option<String>>(0),
                        )
                        .ok()
                        .flatten()
                    })
                    .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok())
                    .unwrap_or_default(),
                None => Vec::new(),
            }
        };

        for dir in workspace_attached_dirs.iter() {
            let pb = std::path::PathBuf::from(dir);
            if !pb.exists() {
                continue;
            }
            let space_id_label = space_id_for_session.as_deref().unwrap_or("default");
            let name = pb
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("attached")
                .to_string();
            out.push(MountRoot {
                id: format!("workspace-attached:{}:{}", space_id_label, mount_id_hash(&pb)),
                label: name,
                path: pb,
                kind: MountKind::AttachedDir,
                editable: false,
            });
        }

        // Session-scoped attached_dirs — one mount per path (filtered to existing).
        if let Some(sid) = session_id.as_deref() {
            for dir in session_attached_dirs.iter() {
                let pb = std::path::PathBuf::from(dir);
                if !pb.exists() {
                    continue;
                }
                let name = pb
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("attached")
                    .to_string();
                out.push(MountRoot {
                    id: format!("attached:{}:{}", sid, mount_id_hash(&pb)),
                    label: name,
                    path: pb,
                    kind: MountKind::AttachedDir,
                    editable: false,
                });
            }
        }

        Ok(out)
    }

    pub async fn files_rail_mount_path(
        &self,
        mount_id: &str,
        session_id_override: Option<String>,
    ) -> Result<std::path::PathBuf, crate::error::Error> {
        // Prefer the caller-supplied session_id (the frontend always knows which
        // session it is rendering). Fall back to mount_id parsing for
        // session:<id> / attached:<sid>:<idx> when the caller didn't pass it.
        let session = session_id_override.or_else(|| self.extract_session_from_mount(mount_id));
        let mounts = self.files_rail_list_mounts(session).await?;
        mounts
            .into_iter()
            .find(|m| m.id == mount_id)
            .map(|m| m.path)
            .ok_or_else(|| crate::error::Error::Internal(format!("mount not found: {}", mount_id)))
    }

    pub async fn files_rail_resolve_dir(
        &self,
        mount_id: &str,
        rel_path: &str,
        session_id_override: Option<String>,
    ) -> Result<(std::path::PathBuf, std::path::PathBuf), crate::error::Error> {
        let mount_root = self.files_rail_mount_path(mount_id, session_id_override).await?;
        let target = if rel_path.is_empty() || rel_path == "/" {
            mount_root.clone()
        } else {
            if rel_path.starts_with('/') || rel_path.split('/').any(|seg| seg == "..") {
                return Err(crate::error::Error::InvalidInput(
                    "invalid rel_path: absolute paths and .. segments are not allowed".into(),
                ));
            }
            mount_root.join(rel_path)
        };
        Ok((mount_root, target))
    }

    fn extract_session_from_mount(&self, mount_id: &str) -> Option<String> {
        if let Some(rest) = mount_id.strip_prefix("session:") {
            return Some(rest.to_string());
        }
        if let Some(rest) = mount_id.strip_prefix("attached:") {
            return rest.split(':').next().map(|s| s.to_string());
        }
        None
    }
}

pub(crate) fn resolve_active_workspace_root_from_conn(
    conn: &rusqlite::Connection,
    default_root: &Path,
) -> PathBuf {
    let path_from_db: Option<PathBuf> = (|| {
        let id: String = conn.query_row(
            "SELECT value FROM settings WHERE key = 'active_workspace_id'",
            [],
            |row| row.get::<_, String>(0),
        ).ok()?;
        conn.query_row(
            "SELECT path FROM spaces WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten()
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
    })();

    path_from_db.unwrap_or_else(|| default_root.to_path_buf())
}

#[cfg(test)]
mod gbrain_launcher_tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn workspace_conn(active_id: &str, active_path: Option<&str>) -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
        conn.execute(
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .expect("settings table");
        conn.execute("CREATE TABLE spaces (id TEXT PRIMARY KEY, path TEXT)", [])
            .expect("spaces table");
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('active_workspace_id', ?1)",
            rusqlite::params![active_id],
        )
        .expect("active setting");
        conn.execute(
            "INSERT INTO spaces (id, path) VALUES (?1, ?2)",
            rusqlite::params![active_id, active_path],
        )
        .expect("active space");
        conn
    }

    #[test]
    fn active_workspace_root_prefers_active_space_path() {
        let conn = workspace_conn("w4d", Some("/Users/ryanliu/Documents/workground/w4d-test"));
        let root = resolve_active_workspace_root_from_conn(
            &conn,
            std::path::Path::new("/Users/ryanliu/Documents/workground"),
        );

        assert_eq!(
            root,
            std::path::PathBuf::from("/Users/ryanliu/Documents/workground/w4d-test")
        );
    }

    #[test]
    fn active_workspace_root_falls_back_to_workground_not_process_cwd() {
        let conn = workspace_conn("legacy", Some(""));
        let root = resolve_active_workspace_root_from_conn(
            &conn,
            std::path::Path::new("/Users/ryanliu/Documents/workground"),
        );

        assert_eq!(
            root,
            std::path::PathBuf::from("/Users/ryanliu/Documents/workground")
        );
        assert!(!root.ends_with("src-tauri"));
    }

    #[test]
    fn write_gbrain_launcher_files_creates_run_sh_and_paths_json() {
        let data = tempdir().unwrap();
        let gbrain_home = data.path().join("gbrain");
        let bun = data.path().join("fake-bun");
        let entry = data.path().join("fake-cli.ts");
        // Write placeholder files so the canonicalized paths exist as files.
        fs::write(&bun, "").unwrap();
        fs::write(&entry, "").unwrap();

        AppState::write_gbrain_launcher_files(&gbrain_home, &bun, &entry)
            .expect("launcher write should succeed");
        let run_sh = gbrain_home.join("run.sh");
        let paths_json = gbrain_home.join("paths.json");
        assert!(run_sh.is_file(), "run.sh should exist");
        assert!(paths_json.is_file(), "paths.json should exist");

        // run.sh content: shebang + GBRAIN_HOME export + exec line.
        let script = fs::read_to_string(&run_sh).unwrap();
        assert!(script.starts_with("#!/usr/bin/env bash\n"), "shebang missing");
        assert!(
            script.contains(&format!("export GBRAIN_HOME='{}'", gbrain_home.display())),
            "GBRAIN_HOME export missing or wrong path"
        );
        assert!(
            script.contains("exec '"),
            "exec line missing"
        );
        // The exec line must reference bun and entry by absolute path.
        assert!(script.contains(&bun.display().to_string()), "bun path missing");
        assert!(script.contains(&entry.display().to_string()), "entry path missing");

        // paths.json: serde_json parseable, contains the expected keys.
        let manifest_raw = fs::read_to_string(&paths_json).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&manifest_raw)
            .expect("paths.json should be valid JSON");
        assert_eq!(manifest["uclaw_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(manifest["bun_path"], bun.to_string_lossy().as_ref());
        assert_eq!(manifest["gbrain_entry"], entry.to_string_lossy().as_ref());
        assert_eq!(manifest["gbrain_home"], gbrain_home.to_string_lossy().as_ref());
        // brain_dir is the canonical PGLite layout from PR #205.
        let brain_dir = gbrain_home.join(".gbrain").join("brain.pglite");
        assert_eq!(manifest["brain_dir"], brain_dir.to_string_lossy().as_ref());
        let config_json_path = gbrain_home.join(".gbrain").join("config.json");
        assert_eq!(manifest["config_json"], config_json_path.to_string_lossy().as_ref());
        // generated_at_ms is a number (we don't pin a specific value).
        assert!(manifest["generated_at_ms"].is_i64(), "generated_at_ms should be i64");
    }

    #[cfg(unix)]
    #[test]
    fn write_gbrain_launcher_files_marks_run_sh_executable() {
        use std::os::unix::fs::PermissionsExt;
        let data = tempdir().unwrap();
        let gbrain_home = data.path().join("gbrain");
        let bun = data.path().join("fake-bun");
        let entry = data.path().join("fake-cli.ts");
        std::fs::write(&bun, "").unwrap();
        std::fs::write(&entry, "").unwrap();

        AppState::write_gbrain_launcher_files(&gbrain_home, &bun, &entry).unwrap();
        let run_sh = gbrain_home.join("run.sh");
        let mode = std::fs::metadata(&run_sh).unwrap().permissions().mode();
        // mode includes file-type bits; mask to permission bits (lowest 9).
        assert_eq!(mode & 0o777, 0o755, "run.sh should be chmod 0o755");
    }

    #[test]
    fn write_gbrain_launcher_files_handles_paths_with_spaces() {
        let data = tempdir().unwrap();
        let gbrain_home = data.path().join("gbrain");
        // Create a sub-path with a space — exercises shell_quote_path
        let nested = data.path().join("with space");
        std::fs::create_dir_all(&nested).unwrap();
        let bun = nested.join("bun");
        let entry = nested.join("cli.ts");
        std::fs::write(&bun, "").unwrap();
        std::fs::write(&entry, "").unwrap();

        AppState::write_gbrain_launcher_files(&gbrain_home, &bun, &entry).unwrap();

        let run_sh_content = std::fs::read_to_string(
            gbrain_home.join("run.sh")
        ).unwrap();
        // Path with space must be single-quoted so the shell treats it as one arg.
        assert!(
            run_sh_content.contains(&format!("'{}'", bun.display())),
            "path with space should be single-quoted, got:\n{}",
            run_sh_content
        );
    }

    #[test]
    fn write_gbrain_launcher_files_handles_paths_with_single_quote() {
        // shell_quote_path's load-bearing branch: ' in input must
        // become '\'' in output (close-quote, escaped-quote, open-quote).
        // macOS filesystems allow single quotes in paths; if the escape
        // were wrong the resulting run.sh would silently exec the wrong
        // command or fail with a shell syntax error.
        let data = tempdir().unwrap();
        let gbrain_home = data.path().join("gbrain");
        let nested = data.path().join("it's");
        std::fs::create_dir_all(&nested).unwrap();
        let bun = nested.join("bun");
        let entry = nested.join("cli.ts");
        std::fs::write(&bun, "").unwrap();
        std::fs::write(&entry, "").unwrap();

        AppState::write_gbrain_launcher_files(&gbrain_home, &bun, &entry).unwrap();

        let content = std::fs::read_to_string(
            gbrain_home.join("run.sh")
        ).unwrap();
        // The path "it's" should be escaped in the shell as it'\''s
        // (the single quote becomes: close-quote, backslash-escaped-quote, open-quote).
        // This is the canonical POSIX way to include a literal single quote
        // inside a single-quoted string. The pattern is: '\\'' (literal 3-char sequence).
        assert!(
            content.contains("'\\''"),
            "single-quote in path must be escaped as '\\'' in the shell, got:\n{}",
            content
        );
    }
}

#[cfg(all(test, unix))]
mod memu_runtime_resolution_tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    fn write_executable(path: &std::path::Path, body: &str) {
        fs::write(path, body).unwrap();
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[test]
    fn find_python_skips_broken_python3_and_uses_versioned_binary() {
        let dir = tempdir().unwrap();
        let bin_dir = dir.path().join("python").join("bin");
        fs::create_dir_all(&bin_dir).unwrap();

        write_executable(
            &bin_dir.join("python3"),
            "#!/usr/bin/env bash\nexit 137\n",
        );
        write_executable(
            &bin_dir.join("python3.13"),
            "#!/usr/bin/env bash\necho Python 3.13.13\n",
        );

        let selected = AppState::find_python(Some(dir.path())).unwrap();
        assert_eq!(
            selected,
            bin_dir.join("python3.13").to_string_lossy().as_ref()
        );
    }

    #[test]
    fn find_bridge_script_prefers_release_resource_over_data_copy() {
        let resource_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let bundled = resource_dir.path().join("memu_bridge.py");
        let stale_data_copy = data_dir.path().join("memu_bridge.py");
        fs::write(&bundled, "# bundled\n").unwrap();
        fs::write(&stale_data_copy, "# stale data copy\n").unwrap();

        let selected =
            AppState::find_bridge_script(Some(resource_dir.path()), data_dir.path()).unwrap();
        assert_eq!(selected, bundled);
    }

    #[test]
    fn release_resource_resolution_uses_packaged_paths_for_memory_runtimes() {
        let resource_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();

        let bundled_bun = resource_dir.path().join("bun");
        write_executable(&bundled_bun, "#!/usr/bin/env bash\necho 1.2.3\n");

        let bundled_python_dir = resource_dir.path().join("python").join("bin");
        fs::create_dir_all(&bundled_python_dir).unwrap();
        let bundled_python = bundled_python_dir.join("python3.13");
        write_executable(&bundled_python, "#!/usr/bin/env bash\necho Python 3.13.13\n");

        let bundled_gbrain_entry = resource_dir
            .path()
            .join("gbrain")
            .join("src")
            .join("cli.ts");
        fs::create_dir_all(bundled_gbrain_entry.parent().unwrap()).unwrap();
        fs::write(&bundled_gbrain_entry, "console.log('gbrain')\n").unwrap();

        let bundled_bridge = resource_dir.path().join("memu_bridge.py");
        fs::write(&bundled_bridge, "# bundled bridge\n").unwrap();
        fs::write(data_dir.path().join("memu_bridge.py"), "# stale data bridge\n").unwrap();

        let bun = AppState::find_bun_path(Some(resource_dir.path())).unwrap();
        let python = AppState::find_python(Some(resource_dir.path())).unwrap();
        let gbrain_entry = AppState::find_gbrain_entry(Some(resource_dir.path())).unwrap();
        let bridge = AppState::find_bridge_script(Some(resource_dir.path()), data_dir.path()).unwrap();

        assert_eq!(bun, bundled_bun);
        assert_eq!(python, bundled_python.to_string_lossy().as_ref());
        assert_eq!(gbrain_entry, bundled_gbrain_entry);
        assert_eq!(bridge, bundled_bridge);

        let gbrain_home = data_dir.path().join("gbrain");
        AppState::write_gbrain_launcher_files(&gbrain_home, &bun, &gbrain_entry).unwrap();
        let run_sh = fs::read_to_string(gbrain_home.join("run.sh")).unwrap();
        let paths_json = fs::read_to_string(gbrain_home.join("paths.json")).unwrap();

        assert!(
            run_sh.contains(&resource_dir.path().display().to_string()),
            "release launcher should point at bundled resources, got:\n{}",
            run_sh
        );
        assert!(
            !run_sh.contains(env!("CARGO_MANIFEST_DIR")),
            "release launcher must not bake in dev checkout paths, got:\n{}",
            run_sh
        );
        assert!(
            paths_json.contains(&resource_dir.path().display().to_string()),
            "paths manifest should point at bundled resources, got:\n{}",
            paths_json
        );
        assert!(
            !paths_json.contains(env!("CARGO_MANIFEST_DIR")),
            "paths manifest must not bake in dev checkout paths, got:\n{}",
            paths_json
        );
    }

    #[test]
    fn packaged_resource_resolution_refuses_dev_fallback_when_bundle_is_incomplete() {
        let resource_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();

        fs::write(data_dir.path().join("memu_bridge.py"), "# stale data bridge\n").unwrap();

        assert!(
            AppState::find_bun_path(Some(resource_dir.path())).is_none(),
            "packaged release must not fall back to dev bunembed/bun"
        );
        assert!(
            AppState::find_gbrain_entry(Some(resource_dir.path())).is_none(),
            "packaged release must not fall back to dev gbrain-source"
        );
        assert!(
            AppState::find_bridge_script(Some(resource_dir.path()), data_dir.path()).is_none(),
            "packaged release must not fall back to stale data-dir memu_bridge.py"
        );
        assert!(
            AppState::find_python(Some(resource_dir.path())).is_none(),
            "packaged release must not fall back to dev or system Python"
        );
    }

    // ────────────────────────────────────────────────────────────────────
    // Bundle 15 — find_bun_path prefers system Bun in dev mode
    // ────────────────────────────────────────────────────────────────────
    //
    // The packaged-mode tests above already cover the "bundled wins, no
    // system fallback" case. These tests cover dev mode (resource_dir is
    // under CARGO_MANIFEST_DIR, or None), where macOS 26.4+ Gatekeeper
    // SIGKILLs the unsigned target/debug/bun copy and we need to dodge
    // it by preferring the user's installed Bun.
    //
    // Tests use `system_bun_candidates_for_home` directly so they don't
    // mutate the process's $HOME (which would race with parallel tests).

    #[test]
    fn system_bun_candidates_includes_dot_bun_first_when_home_set() {
        let home = std::ffi::OsString::from("/Users/test-user");
        let candidates = AppState::system_bun_candidates_for_home(Some(&home));
        assert_eq!(
            candidates.first().map(|p| p.as_path()),
            Some(std::path::Path::new("/Users/test-user/.bun/bin/bun")),
            "~/.bun/bin/bun must be the first candidate (bun.sh installer default)"
        );
        // Homebrew + standard prefixes are present too.
        assert!(
            candidates
                .iter()
                .any(|p| p == std::path::Path::new("/opt/homebrew/bin/bun")),
            "Homebrew (Apple Silicon) path missing"
        );
        assert!(
            candidates
                .iter()
                .any(|p| p == std::path::Path::new("/usr/local/bin/bun")),
            "Homebrew (Intel) / Linux standard prefix missing"
        );
    }

    #[test]
    fn system_bun_candidates_handles_no_home_gracefully() {
        let candidates = AppState::system_bun_candidates_for_home(None);
        // Without HOME we still return the Homebrew + standard prefixes.
        assert!(
            !candidates.is_empty(),
            "must still return system prefixes when HOME unset"
        );
        assert!(
            candidates
                .iter()
                .all(|p| !p.to_string_lossy().contains(".bun/bin")),
            ".bun/bin candidate must be omitted when HOME is unset"
        );
    }

    #[test]
    fn find_bun_path_in_dev_mode_prefers_working_system_bun_over_dev_bundled() {
        // Set up a fake "system" Bun under a tempdir's .bun/bin/bun and
        // a separate fake "dev bundled" Bun in a resource_dir under the
        // crate manifest dir. Then call find_bun_path with the dev-mode
        // resource_dir. Expect: the system Bun wins.
        //
        // The system check goes through system_bun_candidates(), which
        // hits the *real* HOME — so we can't use a tempdir for HOME
        // here without env mutation. Instead we assert the public
        // behavior contract: when the host's system Bun exists AND is
        // working (the common dev-machine case), find_bun_path in dev
        // mode returns a system path, not the bundled/dev path.
        //
        // CI runners without bun installed will return None or fall
        // through to the dev path; both are acceptable. So this test
        // only asserts the precondition + the negative ("did not pick
        // the broken-on-purpose dev bundled binary").
        let manifest_subdir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("__bundle15_test_resource_dir__");
        let _ = fs::create_dir_all(&manifest_subdir);
        // SIGKILL-on-launch fake (mimics unnotarized binary): exit 137.
        // first_working_bun's `--version` probe will reject it.
        let fake_bundled_bun = manifest_subdir.join("bun");
        write_executable(&fake_bundled_bun, "#!/usr/bin/env bash\nexit 137\n");

        // Sanity: the fake is under CARGO_MANIFEST_DIR, so
        // is_packaged_resource_dir returns false (dev mode).
        assert!(
            !AppState::is_packaged_resource_dir(&manifest_subdir),
            "test setup error: manifest subdir must be treated as dev"
        );

        let result = AppState::find_bun_path(Some(&manifest_subdir));

        // Clean up before we assert so a failure doesn't leak files.
        let _ = std::fs::remove_dir_all(&manifest_subdir);

        // We never want the broken bundled fake selected — that would
        // mean Bundle 15 regressed and dev again relies on the
        // SIGKILL-prone copy.
        if let Some(selected) = result.as_ref() {
            assert_ne!(
                selected, &fake_bundled_bun,
                "find_bun_path picked the broken dev-bundled fake: {}",
                selected.display()
            );
        }
        // We don't assert Some(...) — CI runners without Bun installed
        // legitimately return None here, and that's still better than
        // returning a binary that will SIGKILL on launch.
    }
}

#[cfg(test)]
mod hook_bus_tests {
    #[tokio::test]
    async fn shared_hook_bus_dispatch_observe_is_noop_without_subscribers() {
        use crate::agent::hook_bus::{HookBus, HookEvent};
        let bus = std::sync::Arc::new(HookBus::new());
        bus.dispatch_observe(&HookEvent::PostToolUse {
            task_id: "t".into(),
            tool_name: "read_file".into(),
            success: true,
            result_preview: "ok".into(),
        }).await;
        // no subscribers -> no panic, no side effect
    }

    /// Task 2: Verifies that the PolicySpecSubscriber (Allow-all default policy)
    /// can be registered on a bare HookBus before it is Arc-wrapped.
    /// Uses the register-logic approach (AppState::new requires a tauri::AppHandle
    /// which is not constructible in unit tests).
    #[test]
    fn default_policy_subscriber_registers_on_bus() {
        use crate::agent::hook_bus::HookBus;
        let mut bus = HookBus::new();
        bus.register(std::sync::Arc::new(
            crate::policy_eval::PolicySpecSubscriber::new(super::default_hook_policy()),
        ))
        .unwrap();
        assert_eq!(bus.subscriber_count(), 1);
    }
}
