use crate::error::Error;
use serde::{Deserialize, Serialize};
pub use uclaw_message_types::{
    estimate_message_tokens, estimate_tokens, ChatMessage, ContentBlock, MessageRole,
};
pub use uclaw_tool_types::{ToolCall, ToolDefinition};

// ─── Thread State Machine ─────────────────────────────────────────────

/// Tracks the lifecycle state of an agent thread.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ThreadState {
    /// Idle — waiting for user input.
    Idle,
    /// Actively processing an iteration.
    Processing,
    /// Paused — a tool requires user approval before continuing.
    AwaitingApproval {
        tool_name: String,
        tool_id: String,
        arguments: serde_json::Value,
    },
    /// The loop completed normally.
    Completed,
    /// The loop was interrupted by a signal.
    Interrupted,
    /// The loop terminated with a failure.
    Failed { error: String },
}

impl ThreadState {
    /// Returns true if the state allows a transition to `Processing`.
    pub fn can_start_processing(&self) -> bool {
        matches!(self, ThreadState::Idle)
    }

    /// Returns true if the thread is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ThreadState::Completed | ThreadState::Interrupted | ThreadState::Failed { .. }
        )
    }
}

// ─── Reasoning Context ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ReasoningContext {
    pub messages: Vec<ChatMessage>,
    pub system_prompt: String,
    pub force_text: bool,
    /// Current thread state.
    pub thread_state: ThreadState,
    /// Cumulative token usage tracked across iterations.
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
    /// Number of mutating tool calls (write_file / edit / apply_patch /
    /// side-effecting bash) executed since the last `plan_update done:true`.
    /// Reset to 0 when `plan_update done:true` succeeds. Anti-fake-progress
    /// guard: a `plan_update done:true` that arrives with this counter at 0
    /// is intercepted in the dispatcher and replaced with a soft error so
    /// the model can't game plan-aware termination.
    pub mutations_since_last_plan_done: usize,
    /// How many times in this loop we've already intercepted a
    /// `plan_update done:true` with zero mutation evidence. Bounded escape
    /// hatch — after `MAX_MUTATION_CHALLENGES` we let the call through with
    /// a logged warning (so genuine corner cases like "I did the work in a
    /// way the heuristic doesn't recognize" don't loop forever).
    pub mutation_challenges_issued: usize,
    /// How many consecutive text responses ended with finish_reason=length.
    /// Resets to 0 on any successful tool call. Used to escalate from a
    /// generic "call write_file" nudge to a chunked-writing strategy after
    /// repeated truncations (the file is simply too large for one shot).
    pub consecutive_length_truncations: usize,
    /// Partial code block being accumulated across finish_reason=length
    /// responses. Stores (language_tag, content_so_far). When the model
    /// continues a truncated response, we prepend this fence+content before
    /// running code-block rescue, so a block split across multiple responses
    /// can still be rescued as a complete write_file call.
    pub partial_code_buffer: Option<(String, String)>,
    /// How many consecutive times the plan-guard nudge fired without the
    /// model making a tool call in response. Resets to 0 on any tool call.
    /// After MAX_PLAN_GUARD_NUDGES, the guard gives up and returns the
    /// response as-is to avoid infinite text-only loops.
    pub consecutive_plan_guard_nudges: usize,
    /// R-6 cancellation surface — optional cancellation token observed at
    /// every stage boundary in `run_agentic_loop` AND mid-flight inside
    /// `stream_completion` / `ToolDispatcher::dispatch` (Slice 1a, M1-T2e
    /// completed 2026-05-28). When set + cancelled, the loop returns
    /// `LoopOutcome::Cancelled` at the nearest checkpoint (after `call_llm`
    /// completes, after `execute_tool_calls` completes, at iteration top).
    /// Mid-flight aborts work without any `LoopDelegate` trait signature
    /// change because both `call_llm` and `execute_tool_calls` already take
    /// `&mut ReasoningContext`, which carries this token.
    ///
    /// Tokens aren't persisted; they're per-run state injected by the
    /// `RegularTask` / `run_with_rollout` wrapper at the start of each
    /// turn. (ReasoningContext doesn't derive Serialize/Deserialize, so
    /// no serde annotation needed.)
    pub cancellation_token: Option<tokio_util::sync::CancellationToken>,
    /// Pi Sprint 1 — per-session file ops accumulator. Updated by
    /// `execute_tool_calls` after every successful file-touching tool
    /// call. Merged into `StructuredFold.file_ops` at compaction time
    /// (agentic_loop.rs::soft_compress_context) so the agent never
    /// forgets which files it touched across compression cycles.
    pub file_ops: crate::agent::file_ops::SessionFileOps,
    /// 迭代式压缩状态(Pi Sprint 2):跨轮次累积上一份 fold。
    pub compaction_state: crate::agent::compaction::CompactionState,
}

impl ReasoningContext {
    pub fn new(system_prompt: String) -> Self {
        Self {
            messages: Vec::new(),
            system_prompt,
            force_text: false,
            thread_state: ThreadState::Idle,
            total_input_tokens: 0,
            total_output_tokens: 0,
            mutations_since_last_plan_done: 0,
            mutation_challenges_issued: 0,
            consecutive_length_truncations: 0,
            partial_code_buffer: None,
            consecutive_plan_guard_nudges: 0,
            cancellation_token: None,
            file_ops: crate::agent::file_ops::SessionFileOps::default(),
            compaction_state: crate::agent::compaction::CompactionState::default(),
        }
    }

    /// Builder — install a cancellation token observed at every stage
    /// boundary in `run_agentic_loop`. See the field docstring for
    /// semantics. Added M1-T2d.
    pub fn with_cancellation(mut self, token: tokio_util::sync::CancellationToken) -> Self {
        self.cancellation_token = Some(token);
        self
    }

    /// True when the installed token has been fired. False if no token
    /// is installed or the token hasn't been cancelled yet.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation_token
            .as_ref()
            .is_some_and(|t| t.is_cancelled())
    }

    /// Token count estimate using CJK-aware per-character weighting.
    /// Uses the same `estimate_tokens()` function that the dispatcher's
    /// `emit_context_stats` uses, replacing the previous `len()/4` heuristic
    /// which severely underestimated CJK text (4-5x undercount).
    /// (P1 fix: 2026-05-16)
    ///
    /// Skips messages marked as `compacted` so logically removed messages
    /// don't inflate the budget calculation. (P1 logical-marking: 2026-05-16)
    pub fn estimate_token_count(&self) -> usize {
        let system_tokens = estimate_tokens(&self.system_prompt) as usize;
        let msg_tokens: usize = self.messages.iter()
            .filter(|m| !m.compacted)
            .map(|m| {
            m.content.iter().map(|b| match b {
                ContentBlock::Text { text } => estimate_tokens(text) as usize,
                ContentBlock::Thinking { thinking, .. } => estimate_tokens(thinking) as usize,
                ContentBlock::ToolUse { name, input, .. } => {
                    estimate_tokens(name) as usize
                        + estimate_tokens(&input.to_string()) as usize
                        + 20
                }
                ContentBlock::ToolResult { content, .. } => {
                    estimate_tokens(content) as usize + 10
                }
            }).sum::<usize>()
        }).sum();
        system_tokens + msg_tokens
    }
}

// ─── Token Usage / Response Metadata ───────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
    /// Reasoning / "thinking" tokens — Claude extended thinking, o1, etc.
    /// Added in M1-T6; existing providers that don't emit this set 0 and
    /// downstream consumers (cost calc, rollout) treat 0 as absent.
    #[serde(default)]
    pub reasoning_output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMetadata {
    pub model: String,
    pub finish_reason: Option<String>,
    pub usage: Option<TokenUsage>,
}

// ─── Context & Cost Stats ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextStats {
    /// Session this event belongs to. Required so multi-session UIs can
    /// route the payload to the right streamingStates entry.
    pub conversation_id: String,
    pub model_context_length: u32,
    pub system_prompt_tokens: u32,
    pub mcp_prompts_tokens: u32,
    pub skills_tokens: u32,
    pub messages_tokens: u32,
    pub tool_use_tokens: u32,
    pub compact_buffer_tokens: u32,
    pub free_tokens: i32,
    /// Cumulative input tokens from API usage across all iterations.
    pub cumulative_input_tokens: Option<u32>,
    /// Cumulative output tokens from API usage across all iterations.
    pub cumulative_output_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnCostInfo {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
    pub cost_usd: String,
}

// ─── Cost Calculation ─────────────────────────────────────────────────

/// Calculate USD cost based on model pricing (per 1M tokens).
/// CNY per USD, used to convert providers that bill in 元 (DeepSeek) into the USD
/// the cost badge shows. An estimate — the frontend caption multiplies the USD
/// back by the SAME rate to surface the native ¥ amount, so keep them in lock-step
/// (`ui` `MessageMetaBar` `CNY_PER_USD`). Exact billing is on the provider invoice.
pub const CNY_PER_USD: f64 = 7.15;

/// Per-1M-token USD prices for a model. `input_hit` is the cache-HIT input rate —
/// DeepSeek caches input at a steep discount (flash ¥0.02 vs ¥1 = 50× cheaper), so
/// ignoring it badly over-bills the big, reused system prompt of a coding agent.
struct ModelPricing {
    input_miss: f64,
    input_hit: f64,
    output: f64,
}

/// USD/1M-token pricing per model. DeepSeek rates are its published ¥/M
/// (api-docs.deepseek.com, 2026-05) ÷ [`CNY_PER_USD`]; others are public USD rates
/// with cache-read modeled as the provider's discount (Anthropic 0.1×, OpenAI 0.5×).
fn model_pricing(model: &str) -> ModelPricing {
    let cny = |yuan: f64| yuan / CNY_PER_USD;
    match model {
        // DeepSeek — ¥/M: v4-flash 1 / 0.02 / 2 ; v4-pro 12 / 0.1 / 24 (regular rate).
        m if m.contains("deepseek-v4-flash") || m.contains("deepseek-chat") => {
            ModelPricing { input_miss: cny(1.0), input_hit: cny(0.02), output: cny(2.0) }
        }
        m if m.contains("deepseek-v4-pro") || m.contains("deepseek-reasoner") => {
            ModelPricing { input_miss: cny(12.0), input_hit: cny(0.1), output: cny(24.0) }
        }
        m if m.contains("deepseek") => {
            ModelPricing { input_miss: cny(1.0), input_hit: cny(0.02), output: cny(2.0) }
        }
        // Anthropic / OpenAI / Qwen — public USD/M; cache-hit ≈ the provider discount.
        m if m.contains("claude-3-5-sonnet") || m.contains("claude-4") || m.contains("claude-sonnet-4") => {
            ModelPricing { input_miss: 3.0, input_hit: 0.3, output: 15.0 }
        }
        m if m.contains("claude-3-5-haiku") || m.contains("claude-haiku") => {
            ModelPricing { input_miss: 0.8, input_hit: 0.08, output: 4.0 }
        }
        m if m.contains("gpt-4o-mini") => ModelPricing { input_miss: 0.15, input_hit: 0.075, output: 0.6 },
        m if m.contains("gpt-4o") => ModelPricing { input_miss: 2.5, input_hit: 1.25, output: 10.0 },
        m if m.contains("gpt-4") => ModelPricing { input_miss: 2.5, input_hit: 1.25, output: 10.0 },
        m if m.contains("qwen") => ModelPricing { input_miss: 0.5, input_hit: 0.05, output: 2.0 },
        _ => ModelPricing { input_miss: 1.0, input_hit: 0.1, output: 3.0 },
    }
}

/// USD cost, treating all input as cache-miss (no cache info). Back-compat wrapper
/// over [`calculate_cost_cached`] for the legacy dispatcher path.
pub fn calculate_cost(model: &str, input_tokens: u32, output_tokens: u32) -> f64 {
    calculate_cost_cached(model, input_tokens, output_tokens, 0)
}

/// Cache-aware USD cost: the cached portion of `input_tokens` (`cache_read_tokens`)
/// bills at the cache-hit rate, the remainder at cache-miss.
pub fn calculate_cost_cached(
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cache_read_tokens: u32,
) -> f64 {
    let p = model_pricing(model);
    let cached = cache_read_tokens.min(input_tokens) as f64;
    let uncached = input_tokens as f64 - cached;
    (uncached * p.input_miss + cached * p.input_hit + output_tokens as f64 * p.output) / 1_000_000.0
}

pub fn format_cost(cost: f64) -> String {
    if cost < 0.01 {
        format!("${:.4}", cost)
    } else {
        format!("${:.2}", cost)
    }
}

/// Get model context window length.
///
/// Note: Anthropic's 1M context beta is auto-enabled in the provider layer
/// for Sonnet 4+ / Opus 4.6+ (see llm::providers::anthropic::supports_1m_context).
/// When that beta is on, the actual usable window is 1M, not 200K — keep
/// these two functions in lock-step.
pub fn get_model_context_length(model: &str) -> u32 {
    let m = model.to_lowercase();
    // 1M context window — Anthropic models that support the beta. Matches
    // supports_1m_context() in the anthropic provider.
    if m.contains("sonnet-4") || m.contains("sonnet4")
        || m.contains("opus-4-6") || m.contains("opus-4-7")
        || m.contains("opus-4-8") || m.contains("opus-4-9")
        || m.contains("opus-5")
    {
        return 1_000_000;
    }
    // DeepSeek V4 family ships a 1M context window (api-docs.deepseek.com pricing).
    // Must precede the generic `deepseek` arm below, which keeps older V3 / chat /
    // reasoner models at their 128K window.
    if m.contains("deepseek-v4") || m.contains("deepseek_v4") || m.contains("deepseekv4") {
        return 1_000_000;
    }
    if m.contains("claude") { 200_000 }
    else if m.contains("gpt-4o") || m.contains("gpt-4") || m.contains("deepseek") { 128_000 }
    else if m.contains("qwen") { 131_072 }
    else { 200_000 }
}

// ─── LLM Response Outputs ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum RespondOutput {
    Text { text: String, thinking: Option<String>, thinking_signature: Option<String>, metadata: ResponseMetadata },
    ToolCalls { tool_calls: Vec<ToolCall>, text: Option<String>, thinking: Option<String>, thinking_signature: Option<String>, metadata: ResponseMetadata },
}

/// Streaming delta
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamDelta {
    TextDelta { text: String },
    ThinkingDelta { thinking: String },
    SignatureDelta { signature: String },
    ToolCallDelta { id: String, name: Option<String>, input_json: Option<String> },
    Done { finish_reason: Option<String>, usage: Option<TokenUsage> },
}

// ─── Loop Signals & Outcomes ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum LoopSignal {
    Continue,
    Stop,
    Cancel,
    InjectMessage { content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoopOutcome {
    Response {
        text: String,
        usage: Option<TokenUsage>,
        truncated: bool,
        /// M1-backlog #3 — the provider's model identifier that produced
        /// this response. `None` for non-LLM-response paths (tool result
        /// terminators, escalation paths). Surfaced into the `ModelTurn`
        /// rollout event so cost / usage analyses can attribute by model.
        #[serde(default)]
        model: Option<String>,
    },
    ToolResult { results: Vec<String> },
    Stopped,
    Cancelled { partial_code: Option<String> },
    MaxIterations,
    Failure { error: String },
    NeedApproval { tool_name: String, tool_call_id: String, parameters: serde_json::Value },
}

#[derive(Debug)]
pub enum TextAction {
    Return(LoopOutcome),
    Continue,
    /// Like Continue, but agentic_loop will append the assistant turn and inject
    /// this nudge as a user message before the next iteration. The dispatcher must
    /// NOT push the assistant message itself — the loop owns that responsibility.
    ContinueWithNudge(String),
    /// The text response contained complete code block(s) that were rescued into
    /// synthetic write_file ToolCalls. The loop should append the assistant turn
    /// (text + synthetic tool_use blocks), execute the calls through the normal
    /// tool path (including safety approval), then continue.
    RescueWithToolCalls(Vec<ToolCall>),
}

// ─── Loop Delegate Trait ───────────────────────────────────────────────

#[async_trait::async_trait]
pub trait LoopDelegate: Send + Sync {
    async fn check_signals(&self) -> LoopSignal;
    async fn before_llm_call(&self, reason_ctx: &mut ReasoningContext, iteration: usize) -> Option<LoopOutcome>;
    async fn call_llm(
        &self,
        reason_ctx: &mut ReasoningContext,
        snapshot: &crate::agent::turn::TurnSnapshot,
        iteration: usize,
    ) -> Result<RespondOutput, Error>;
    async fn handle_text_response(&self, text: &str, metadata: ResponseMetadata, reason_ctx: &mut ReasoningContext) -> TextAction;
    async fn execute_tool_calls(&self, tool_calls: Vec<ToolCall>, reason_ctx: &mut ReasoningContext) -> Result<Option<LoopOutcome>, Error>;
    async fn on_tool_intent_nudge(&self, _text: &str, _ctx: &mut ReasoningContext) {}
    async fn on_usage(&self, _usage: &TokenUsage, _reason_ctx: &ReasoningContext, _snapshot: &crate::agent::turn::TurnSnapshot) {}
    async fn after_iteration(&self, _iteration: usize) {}
    /// Generate a semantic summary of the given messages for context compression.
    /// Called during soft_compress_context to produce an L1 archive summary.
    /// Returns `None` to fall back to the template-based placeholder summary.
    async fn summarize_for_compression(&self, _messages: &[ChatMessage]) -> Option<String> {
        None
    }
    /// Generate a StructuredFold summary of the given messages for context compression.
    /// Called during soft_compress_context to produce an L1 archive summary.
    async fn summarize_to_fold(&self, _messages: &[ChatMessage]) -> Option<super::compact::StructuredFold> {
        None
    }
    /// 增量更新一份 fold(prior fold + 仅新消息)。默认实现委托到全量
    /// `summarize_to_fold`(忽略 prior),以便未覆盖的 delegate 仍可用。
    async fn update_fold_incremental(
        &self,
        prior_fold: &super::compact::StructuredFold,
        new_messages: &[ChatMessage],
    ) -> Option<super::compact::StructuredFold> {
        let _ = prior_fold;
        self.summarize_to_fold(new_messages).await
    }

    /// 为即将开始的一轮创建不可变快照。默认从 reason_ctx 取(测试 delegate 用);
    /// ChatDelegate override 为真实组装(model + effective_system_prompt + tools)。
    async fn create_turn_snapshot(
        &self,
        reason_ctx: &ReasoningContext,
        turn_index: u32,
    ) -> crate::agent::turn::TurnSnapshot {
        crate::agent::turn::TurnSnapshot {
            turn_index,
            model: String::new(),
            system_prompt: std::sync::Arc::new(reason_ctx.system_prompt.clone()),
            dynamic_context: String::new(),
            tools: std::sync::Arc::new(Vec::new()),
            force_text: reason_ctx.force_text,
        }
    }

    /// 轮边界钩子:返回对下一轮的补丁。默认 None(item ② 无生产者)。
    async fn prepare_next_turn(
        &self,
        _reason_ctx: &ReasoningContext,
        _turn_index: u32,
    ) -> Option<crate::agent::turn::NextTurnPatch> {
        None
    }

    /// 自然停止点钩子,item ③(双队列)填充。默认空。
    async fn get_steering_messages(&self) -> Vec<ChatMessage> {
        Vec::new()
    }
    async fn get_follow_up_messages(&self) -> Vec<ChatMessage> {
        Vec::new()
    }
    /// Persist an injected steering/follow-up user message into agent_messages
    /// (so reloads stay continuous). Default no-op (for test mocks).
    async fn persist_user_message(&self, _msg: &ChatMessage) {}
}

// ─── Agentic Loop Config ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AgenticLoopConfig {
    /// Maximum loop iterations before giving up.
    pub max_iterations: usize,
    /// Whether to detect and nudge tool intent without tool calls.
    pub enable_tool_intent_nudge: bool,
    /// Maximum consecutive nudge attempts before stopping.
    pub max_tool_intent_nudges: usize,
    /// Truncations before forcing text-only mode.
    pub max_truncations: usize,
    /// Total token budget for the conversation.
    pub token_budget: usize,
    /// Fraction of token_budget that triggers context compression (0.0–1.0).
    pub compression_threshold: f32,
    /// Fraction of token_budget that triggers hard truncation (0.0–1.0).
    pub hard_truncation_threshold: f32,
    /// Number of recent turns to keep during compression.
    pub compression_keep_turns: usize,
    /// Model context window length (0 = no cap, use raw budget).
    /// Set by dispatcher from get_model_context_length().
    pub model_context_length: u32,
}

impl Default for AgenticLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 50,
            enable_tool_intent_nudge: true,
            max_tool_intent_nudges: 4,
            max_truncations: 3,
            // 提升上下文预算：200K tokens 覆盖更长的对话历史
            token_budget: 200_000,
            // 仅当达到 90% 时才触发压缩（原 85%），给 agent 更多上下文空间
            compression_threshold: 0.90,
            // 仅当达到 98% 时才硬截断
            hard_truncation_threshold: 0.98,
            // 保留最近 20 轮而非 10 轮，压缩后仍保留足够上下文
            compression_keep_turns: 20,
            // 0 = no cap, will be set to actual model context window by dispatcher
            model_context_length: 0,
        }
    }
}

impl AgenticLoopConfig {
    /// 基于模型名称自动规划上下文预算。
    ///
    /// 使用 [`crate::agent::context::plan_context_for_model`] 计算
    /// 模型感知的 token 预算、压缩阈值和保留轮次数。
    ///
    /// 此方法替代了手动设置 `model_context_length` 的模式：
    /// ```ignore
    /// // 旧方式（静态默认值 + 手动设置窗口大小）
    /// let mut config = AgenticLoopConfig::default();
    /// config.model_context_length = get_model_context_length(&model);
    ///
    /// // 新方式（模型感知自动规划）
    /// let config = AgenticLoopConfig::from_model(&model);
    /// ```
    pub fn from_model(model: &str) -> Self {
        let plan = crate::agent::context::plan_context_for_model(model);
        Self {
            max_iterations: 50,
            enable_tool_intent_nudge: true,
            max_tool_intent_nudges: 4,
            max_truncations: 3,
            token_budget: plan.token_budget,
            compression_threshold: plan.compression_threshold,
            hard_truncation_threshold: plan.hard_truncation_threshold,
            compression_keep_turns: plan.compression_keep_turns,
            model_context_length: plan.model_context_length,
        }
    }
}

// ─── Constants ─────────────────────────────────────────────────────────

pub const TOOL_INTENT_NUDGE: &str = "You described an action but did not call any tool. \
    Call the tool NOW — do not return text describing what you will do. \
    Use write_file to write code, edit to modify an existing file, or bash to run a command.";

/// Detect LLM signalling tool intent without actually calling a tool
// ─── Reflection Types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionDetail {
    pub assistant_message_id: String,
    pub status: String, // "queued", "running", "completed", "failed"
    pub outcome: Option<String>, // "updated", "created", "no_op"
    pub summary: Option<String>,
    pub detail: Option<String>,
    pub run_started_at: Option<String>,
    pub run_completed_at: Option<String>,
    pub tool_calls: Vec<ReflectionToolCall>,
    pub messages: Vec<ReflectionMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionToolCall {
    pub id: String,
    pub created_at: String,
    pub name: String,
    pub status: String,
    pub parameters: Option<String>,
    pub result_preview: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionMessage {
    pub id: String,
    pub content: String,
    pub created_at: String,
}

// ─── Anti-fake-progress: mutation classification ───────────────────────

/// Maximum number of times we'll soft-block a `plan_update done:true` call
/// in a single loop run before letting it through with a warning. Higher
/// = stricter; lower = more permissive for genuine edge cases. 2 means
/// the model gets one challenge then must either provide evidence in
/// `note` or call a real mutating tool.
pub const MAX_MUTATION_CHALLENGES: usize = 2;

/// How many times the plan-guard nudge may fire consecutively without a tool
/// call response before the guard gives up and lets the response through.
/// This prevents infinite text-only loops when the model ignores nudges.
pub const MAX_PLAN_GUARD_NUDGES: usize = 2;

/// Did this tool call cause a mutation to the workspace / outside world?
/// Used by the anti-fake-progress guard to decide whether a subsequent
/// `plan_update done:true` is plausibly justified by real work.
///
/// Conservative on purpose: pure read tools (`read_file`, `glob`,
/// `grep`, `ls`-only bash) deliberately do NOT count. The LLM can't game
/// the guard by spamming reads.
pub fn is_mutating_tool(name: &str, args: &serde_json::Value) -> bool {
    match name {
        // Direct write tools — always count.
        "write_file" | "edit" | "apply_patch" | "create_file" | "delete_file"
        | "move_file" | "rename_file" => true,
        // Bash is mutating only if the command contains a side-effect marker.
        "bash" => {
            let cmd = args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            bash_command_is_mutating(cmd)
        }
        // Anything else is treated as non-mutating. Add explicit cases here
        // (e.g. mcp tools that write) when known.
        _ => false,
    }
}

/// Heuristic: does this bash command produce a side effect in the workspace?
/// Patterns chosen for high precision (low false-positive) — `ls`, `cat`,
/// `git status`, `pwd`, etc. correctly classify as non-mutating.
pub fn bash_command_is_mutating(cmd: &str) -> bool {
    // Whitespace-stripped lowercase command for substring checks. We look
    // at the raw too because some markers (`>`) are punctuation.
    let lower = cmd.to_lowercase();

    // Output redirection — almost always a write.
    if cmd.contains('>') {
        return true;
    }

    // Specific mutating commands. Match with leading/trailing space to
    // avoid false positives like "rmdir" matching "rm" via prefix.
    const MUTATING_CMDS: &[&str] = &[
        "mkdir ", "touch ", "cp ", "mv ", "rm ", "rmdir ",
        "tee ", "sed -i", "patch ",
        "git add", "git commit", "git rm", "git mv", "git checkout -b",
        "git merge", "git push", "git pull", "git rebase",
        "npm install", "npm i ", "npm uninstall", "npm run build",
        "yarn add", "yarn install", "pnpm install", "pnpm add",
        "cargo new", "cargo init", "cargo add", "cargo build",
        "pip install", "pip uninstall",
        "make install", "make build",
        "chmod ", "chown ",
        "echo ", // echo without > is benign but still counts as activity
    ];
    if MUTATING_CMDS.iter().any(|m| lower.contains(m)) {
        return true;
    }

    false
}

/// Synthetic tool result body returned when we intercept a
/// `plan_update done:true` that has no mutation evidence. The model
/// sees this in the tool_result slot and (per baseline #5) is expected
/// to either actually do the work or supply explicit evidence.
pub const FAKE_PROGRESS_CHALLENGE: &str =
    "Blocked: this `plan_update done:true` call has no supporting evidence. \
     Since the previous `plan_update done:true` (or the start of this turn), \
     ZERO mutating tool calls have run (write_file / edit / apply_patch / \
     bash with > / mkdir / touch / git commit / npm install / etc.).\n\n\
     If the step actually requires writing or modifying code, call the \
     appropriate tool now (write_file / edit / bash) to do the real work, \
     then call plan_update again.\n\n\
     If the step was genuinely completed via means this guard doesn't \
     recognize (e.g. you executed work in a previous turn the user has now \
     resumed), call plan_update again with the `note` field set to a \
     concrete description of WHAT was done and WHERE (file paths, command \
     output, etc.) — that will satisfy the challenge.";

#[cfg(test)]
mod cost_tests {
    use super::{calculate_cost, calculate_cost_cached};

    #[test]
    fn deepseek_flash_cache_hit_is_far_cheaper_than_miss() {
        // ¥1/M miss, ¥0.02/M hit (50× cheaper), ¥2/M output (÷ CNY_PER_USD → USD).
        let miss = calculate_cost_cached("deepseek-v4-flash", 100_000, 1_000, 0);
        let hit = calculate_cost_cached("deepseek-v4-flash", 100_000, 1_000, 90_000);
        assert!(hit < miss / 2.0, "cache hit {hit} should be ≪ all-miss {miss}");
        assert!(miss > 0.0 && miss < 0.05, "miss {miss} out of sane range");
    }

    #[test]
    fn calculate_cost_equals_zero_cache_variant() {
        let a = calculate_cost("deepseek-v4-flash", 10_000, 500);
        let b = calculate_cost_cached("deepseek-v4-flash", 10_000, 500, 0);
        assert!((a - b).abs() < 1e-12);
    }

    #[test]
    fn pro_is_much_more_expensive_than_flash() {
        let flash = calculate_cost("deepseek-v4-flash", 10_000, 1_000);
        let pro = calculate_cost("deepseek-v4-pro", 10_000, 1_000);
        assert!(pro > flash * 5.0, "pro {pro} should be ≫ flash {flash}");
    }
}

#[cfg(test)]
mod mutation_tracking_tests {
    use super::{bash_command_is_mutating, is_mutating_tool};
    use serde_json::json;

    #[test]
    fn write_tools_are_mutating() {
        assert!(is_mutating_tool("write_file", &json!({"path": "a.txt"})));
        assert!(is_mutating_tool("edit", &json!({})));
        assert!(is_mutating_tool("apply_patch", &json!({})));
    }

    #[test]
    fn read_tools_are_not_mutating() {
        assert!(!is_mutating_tool("read_file", &json!({})));
        assert!(!is_mutating_tool("glob", &json!({})));
        assert!(!is_mutating_tool("grep", &json!({})));
        assert!(!is_mutating_tool("plan_write", &json!({})));
        assert!(!is_mutating_tool("plan_update", &json!({})));
    }

    #[test]
    fn bash_ls_pwd_status_are_not_mutating() {
        // Regression: 泡泡龙 session — `ls paopao/` was the only bash
        // call before plan_update done:true, must NOT count as mutation.
        assert!(!bash_command_is_mutating("ls -la paopao/"));
        assert!(!bash_command_is_mutating("pwd"));
        assert!(!bash_command_is_mutating("git status"));
        assert!(!bash_command_is_mutating("git log --oneline -5"));
        assert!(!bash_command_is_mutating("cat README.md"));
        assert!(!bash_command_is_mutating("find . -name '*.html'"));
    }

    #[test]
    fn bash_with_redirect_is_mutating() {
        assert!(bash_command_is_mutating("echo 'hello' > a.html"));
        assert!(bash_command_is_mutating("cat a >> b"));
    }

    #[test]
    fn bash_side_effect_commands_are_mutating() {
        assert!(bash_command_is_mutating("mkdir paopao"));
        assert!(bash_command_is_mutating("touch a.html"));
        assert!(bash_command_is_mutating("cp a b"));
        assert!(bash_command_is_mutating("git add ."));
        assert!(bash_command_is_mutating("git commit -m 'x'"));
        assert!(bash_command_is_mutating("npm install"));
    }

    #[test]
    fn rm_does_not_match_rmdir_via_prefix() {
        // Both should still count as mutating, but for different reasons.
        assert!(bash_command_is_mutating("rm a.txt"));
        assert!(bash_command_is_mutating("rmdir foo"));
        // And the "rm " marker must require trailing space to avoid
        // matching e.g. "harmonize" — sanity check:
        assert!(!bash_command_is_mutating("harmonize the system"));
    }
}

/// Heuristic: did the LLM say "I'm about to use a tool" without actually
/// calling one? If yes, the loop nudges it to actually invoke the tool
/// instead of terminating on text. Covers English + Chinese phrasings.
///
/// Return true when the user message looks purely conversational — a short
/// status/greeting question with no file, code, or task keywords.
///
/// Used to guard the tool_intent_nudge: when the FIRST user message in a
/// turn is purely conversational, nudging the model to "call a tool NOW"
/// converts a harmless "让我看看" preamble into spurious glob/ls/date calls.
/// Examples that should return true: "你在干啥", "你好", "现在几点", "你叫什么名字"
pub fn is_purely_conversational(user_msg: &str) -> bool {
    let trimmed = user_msg.trim();
    // Must be short — real tasks are rarely under 20 chars but greetings are
    if trimmed.len() > 60 {
        return false;
    }
    // Must not contain task-signalling keywords
    let task_keywords = [
        // English
        "file", "code", "write", "read", "edit", "run", "build", "fix", "create",
        "delete", "search", "find", "install", "update", "deploy", "test", "debug",
        // Chinese task words
        "文件", "代码", "写", "读", "编辑", "运行", "构建", "修复", "创建",
        "删除", "搜索", "查找", "安装", "更新", "部署", "测试", "调试",
        "实现", "添加", "生成", "执行", "调用", "配置", "设置",
    ];
    if task_keywords.iter().any(|k| trimmed.contains(k)) {
        return false;
    }
    // Must contain at least one conversational signal
    let conversational_signals = [
        // Status questions
        "在干啥", "在干嘛", "在做什么", "你好", "hi", "hello", "几点", "什么时间",
        "你是谁", "你叫什么", "你能做什么", "你能干什么", "有什么可以",
        // Generic short queries that are clearly conversational
        "怎么了", "有什么事", "还好吗", "how are you", "what are you doing",
        "what's up", "whats up",
    ];
    conversational_signals.iter().any(|s| trimmed.to_lowercase().contains(s))
        || trimmed.ends_with('?') && trimmed.chars().count() < 20
}

/// The 600-char cap (was 200) is to filter out long completion summaries —
/// "intent" phrases in long replies are usually post-hoc descriptions, not
/// pre-action announcements. 600 chars accommodates short Chinese paragraph
/// responses that talk about next-step intent.
pub fn llm_signals_tool_intent(text: &str) -> bool {
    if text.len() >= 600 {
        return false;
    }
    let lower = text.to_lowercase();
    // English action-intent phrases.
    let en_patterns = [
        "let me search", "i'll search", "i'll look", "i will search", "let me check",
        "i'll find", "let me find", "i'll read", "let me read",
        "i'll grep", "let me grep", "i'll fetch", "let me fetch",
        "let me run", "i'll run", "let me open", "i'll open",
        "let me list", "i'll list", "let me write", "i'll write",
        "let me edit", "i'll edit", "next, i'll", "now i'll",
        "let me continue", "i'll continue", "let me update", "i'll update",
    ];
    if en_patterns.iter().any(|p| lower.contains(p)) {
        return true;
    }
    // Chinese action-intent phrases (matched on the original text, not lowered —
    // CJK isn't affected by lowercasing).
    let zh_patterns = [
        "接下来", "下一步", "我来", "让我", "我现在", "现在我",
        "我将", "我要", "马上", "继续", "我来更新", "我来编辑",
        "我来写", "我来调用", "我来读", "下面我", "接着我",
        // "宣告了就不做"型短语 — 模型常说"现在开始写XX"然后直接结束 turn。
        // 这些必须触发 nudge，否则 plan_update done:true 配合空文本会让
        // loop 误判为完成。参考: 泡泡龙 session 中 "现在开始写完整的泡泡龙游戏 HTML 文件！"
        "现在开始", "开始写", "开始编写", "开始构建", "开始创建",
        "开始实现", "开始制作", "开始添加", "开始修改", "开始整合",
        "我来创建", "我来实现", "我来构建", "我来制作", "我来添加",
    ];
    zh_patterns.iter().any(|p| text.contains(p))
}

#[cfg(test)]
mod tool_intent_tests {
    use super::llm_signals_tool_intent;

    #[test]
    fn english_intent_phrases_match() {
        assert!(llm_signals_tool_intent("Let me read the config first."));
        assert!(llm_signals_tool_intent("I'll search for that pattern."));
        assert!(llm_signals_tool_intent("Now I'll update the imports."));
    }

    #[test]
    fn chinese_intent_phrases_match() {
        assert!(llm_signals_tool_intent("接下来我来编辑这个文件"));
        assert!(llm_signals_tool_intent("好的，让我先读一下源码"));
        assert!(llm_signals_tool_intent("现在我要调用 grep 找一下"));
        assert!(llm_signals_tool_intent("继续修改剩余的部分"));
    }

    #[test]
    fn long_responses_dont_trigger() {
        let long = format!("我来更新{}", "这是一段很长的描述文本，".repeat(50));
        assert!(long.len() >= 600);
        // Long replies are usually completion summaries, not pre-action — skip nudge.
        assert!(!llm_signals_tool_intent(&long));
    }

    #[test]
    fn announce_then_stop_phrases_match() {
        // Regression: 泡泡龙 session — agent said these and then returned
        // without ever calling write_file. These must trigger the nudge.
        assert!(llm_signals_tool_intent("现在开始写完整的泡泡龙游戏 HTML 文件！"));
        assert!(llm_signals_tool_intent("现在开始构建 **泡泡龙游戏**！"));
        assert!(llm_signals_tool_intent("我来创建主入口文件"));
        assert!(llm_signals_tool_intent("开始编写核心引擎"));
    }

    #[test]
    fn unrelated_text_does_not_match() {
        assert!(!llm_signals_tool_intent("The function returned successfully."));
        assert!(!llm_signals_tool_intent("代码已经写完了，测试通过。"));
    }
}

#[cfg(test)]
mod conversational_guard_tests {
    use super::is_purely_conversational;

    #[test]
    fn status_questions_are_conversational() {
        // The exact phrase that triggered the bug (2026-05-18 incident)
        assert!(is_purely_conversational("你在干啥"));
        assert!(is_purely_conversational("你在干嘛"));
        assert!(is_purely_conversational("你在做什么"));
        assert!(is_purely_conversational("what are you doing"));
        assert!(is_purely_conversational("what's up"));
    }

    #[test]
    fn greetings_are_conversational() {
        assert!(is_purely_conversational("你好"));
        assert!(is_purely_conversational("hi"));
        assert!(is_purely_conversational("hello"));
    }

    #[test]
    fn time_questions_are_conversational() {
        assert!(is_purely_conversational("现在几点"));
        assert!(is_purely_conversational("几点了?"));
    }

    #[test]
    fn task_requests_are_not_conversational() {
        // Task keywords must prevent the guard from firing
        assert!(!is_purely_conversational("帮我写一个函数"));
        assert!(!is_purely_conversational("查找 main.rs 里的问题"));
        assert!(!is_purely_conversational("创建一个新文件"));
        assert!(!is_purely_conversational("run the tests"));
        assert!(!is_purely_conversational("fix the bug in file.rs"));
    }

    #[test]
    fn long_messages_are_not_conversational() {
        // Anything over 60 chars is assumed to have task intent
        let long = "你好，我想让你帮我分析一下这段代码的性能问题，看看哪里可以优化";
        assert!(long.len() > 60);
        assert!(!is_purely_conversational(long));
    }

    #[test]
    fn task_questions_without_keywords_are_not_conversational() {
        // Should not match — no conversational signal keyword
        assert!(!is_purely_conversational("这是什么意思"));
        assert!(!is_purely_conversational("为什么会这样"));
    }

    #[test]
    fn deepseek_v4_gets_1m_context_window() {
        use super::get_model_context_length;
        // V4 family → 1M (api-docs.deepseek.com pricing).
        assert_eq!(get_model_context_length("deepseek-v4-flash"), 1_000_000);
        assert_eq!(get_model_context_length("deepseek-v4"), 1_000_000);
        assert_eq!(get_model_context_length("DeepSeek-V4-Chat"), 1_000_000);
        // Older deepseek stays at 128K; the V4 arm must not widen them.
        assert_eq!(get_model_context_length("deepseek-chat"), 128_000);
        assert_eq!(get_model_context_length("deepseek-reasoner"), 128_000);
        assert_eq!(get_model_context_length("deepseek-v3"), 128_000);
    }
}
