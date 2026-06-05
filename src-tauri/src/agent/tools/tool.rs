use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use crate::agent::types::ToolDefinition;
use crate::safety::SafetyMode;

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub result: serde_json::Value,
    pub cost: Option<f64>,
    pub duration_ms: u64,
}

impl ToolOutput {
    pub fn new(result: serde_json::Value, duration_ms: u64) -> Self {
        Self { result, cost: None, duration_ms }
    }
    pub fn success(text: &str, duration_ms: u64) -> Self {
        Self { result: serde_json::json!({"ok": true, "content": text}), cost: None, duration_ms }
    }
    pub fn error(text: &str, duration_ms: u64) -> Self {
        Self { result: serde_json::json!({"ok": false, "error": text}), cost: None, duration_ms }
    }
}

/// Categorical label for a tool failure, exposed to the LLM as a
/// bracketed tag in the error message (e.g. `[NotFound] ...`).
///
/// Picking the right kind helps the LLM reason about retry vs.
/// alternative-approach. e.g. NotFound rarely benefits from retry but
/// suggests trying a different URL; Timeout often does benefit from a
/// retry; PermissionDenied means stop and ask the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolErrorKind {
    /// Input doesn't match schema / missing required field / malformed value.
    InvalidInput,
    /// Resource doesn't exist (HTTP 404, FS ENOENT, DB row missing).
    ResourceNotFound,
    /// Authorization or sandboxing rejection (HTTP 403, FS EACCES, SSRF).
    PermissionDenied,
    /// Operation took too long.
    Timeout,
    /// Network-level failure (DNS, connection refused, TLS error).
    NetworkError,
    /// Server-side error (HTTP 5xx, downstream service unhealthy).
    UpstreamError,
    /// HTTP 429 / API rate limit.
    RateLimited,
    /// Body exceeded buffer cap, file too large, etc.
    PayloadTooLarge,
    /// Body couldn't be parsed as expected format (JSON parse, malformed HTML).
    ParseError,
    /// Service / resource temporarily unavailable (DB locked, service starting).
    Unavailable,
    /// A required precondition for the operation does not hold (e.g. the file
    /// was modified externally since last read). The LLM should re-establish
    /// the precondition — typically by re-reading — before retrying. Used by
    /// EditTool's stale-file gate (spec §8.4 — hard reject, not a warning).
    PreconditionFailed,
    /// Catch-all when no other variant fits.
    Other,
}

impl ToolErrorKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidInput => "InvalidInput",
            Self::ResourceNotFound => "NotFound",
            Self::PermissionDenied => "PermissionDenied",
            Self::Timeout => "Timeout",
            Self::NetworkError => "NetworkError",
            Self::UpstreamError => "UpstreamError",
            Self::RateLimited => "RateLimited",
            Self::PayloadTooLarge => "PayloadTooLarge",
            Self::ParseError => "ParseError",
            Self::Unavailable => "Unavailable",
            Self::PreconditionFailed => "PreconditionFailed",
            Self::Other => "Other",
        }
    }
}

/// Tool error
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool execution failed: {0}")]
    Execution(String),
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
    #[error("Tool not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Structured error with a category and a user/LLM-friendly message.
    /// Display formats as `[Kind] message (cause)` so the LLM can pattern-match
    /// on the bracketed tag AND see the underlying cause.
    ///
    /// History: the original Display dropped `source_context` entirely. That
    /// silently swallowed the real reason behind every wrapped failure — e.g. a
    /// transient reqwest DNS/connect error surfaced to the model as a bare
    /// `[NetworkError] Failed to fetch <url>`, which both the agent and users
    /// repeatedly misread as a *timeout*. The captured cause now reaches the
    /// model so it (and we) can tell DNS/connect blips apart from real timeouts.
    #[error("{}", render_kinded(.kind, &.message, &.source_context))]
    Kinded {
        kind: ToolErrorKind,
        message: String,
        source_context: Option<String>,
    },
}

/// Render a `Kinded` error to its model-facing string: `[Kind] message`, with
/// the captured `source_context` appended as `(cause)` when it adds signal.
/// Suppressed when the cause is empty or already contained in `message` so we
/// don't emit `[NetworkError] Failed to fetch X (Failed to fetch X)`.
fn render_kinded(kind: &ToolErrorKind, message: &str, source: &Option<String>) -> String {
    match source.as_deref() {
        Some(cause) if !cause.is_empty() && !message.contains(cause) => {
            format!("[{}] {} ({})", kind.as_str(), message, cause)
        }
        _ => format!("[{}] {}", kind.as_str(), message),
    }
}

impl ToolError {
    pub fn kinded(kind: ToolErrorKind, message: impl Into<String>) -> Self {
        Self::Kinded { kind, message: message.into(), source_context: None }
    }

    pub fn kinded_with_source(
        kind: ToolErrorKind,
        message: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self::Kinded { kind, message: message.into(), source_context: Some(source.into()) }
    }
}

impl serde::Serialize for ToolError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Approval requirement for tool execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalRequirement {
    Never,
    UnlessAutoApproved,
    Always,
}

impl Default for ApprovalRequirement {
    fn default() -> Self { Self::Never }
}

/// Execution mode for a tool invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionMode {
    /// Tool call produced by an agent loop turn.
    AgentTurn,
    /// Direct/manual invocation outside a normal agent turn.
    Direct,
}

/// 工具并发性声明(Pi `executionMode` 子集)。ToolDispatcher 据此把工具分到
/// 串行内联 / 并行 JoinSet 两道。与上面的 `ToolExecutionMode`(调用点模式)是
/// 不同的轴,故另立枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolConcurrency {
    /// 串行(默认):有副作用 / 需审批 / 网络 IO 的工具。
    Sequential,
    /// 并行安全:纯只读工具,可进 JoinSet 批次。
    Parallel,
}

/// Structured context for a tool invocation.
///
/// PR-2 keeps this as adapter metadata. The canonical execution behavior still
/// flows through `Tool::execute(params)` until individual tools opt into richer
/// context-aware behavior in later PRs.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolExecutionContext {
    pub session_id: String,
    pub task_id: Option<String>,
    pub message_id: Option<String>,
    pub tool_call_id: String,
    pub workspace_root: Option<PathBuf>,
    pub execution_mode: ToolExecutionMode,
    pub safety_mode: Option<SafetyMode>,
    pub capability_profile_id: Option<String>,
}

impl ToolExecutionContext {
    pub fn agent_turn(
        session_id: impl Into<String>,
        tool_call_id: impl Into<String>,
        workspace_root: Option<PathBuf>,
        safety_mode: Option<SafetyMode>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            task_id: None,
            message_id: None,
            tool_call_id: tool_call_id.into(),
            workspace_root,
            execution_mode: ToolExecutionMode::AgentTurn,
            safety_mode,
            capability_profile_id: None,
        }
    }

    pub fn for_subcall(&self, tool_call_id: impl Into<String>) -> Self {
        let mut next = self.clone();
        next.tool_call_id = tool_call_id.into();
        next
    }

    pub fn resolve_candidate_path(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(root) = &self.workspace_root {
            root.join(path)
        } else {
            path.to_path_buf()
        }
    }
}

/// Tool trait — implement for each tool
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError>;

    /// 是否支持流式输出。默认 false —— 只有增量产出的工具(BashTool)override。
    /// dispatcher 用它决定是否为本次调用搭建合并节流 drain 任务。
    fn supports_streaming(&self) -> bool { false }

    /// 流式变体。默认忽略 sink、委托 `execute()`。
    /// 只有 `supports_streaming() == true` 的工具才 override 它。
    async fn execute_streaming(
        &self,
        params: serde_json::Value,
        _sink: crate::agent::tools::stream::ToolStreamSink,
    ) -> Result<ToolOutput, ToolError> {
        self.execute(params).await
    }

    fn estimated_cost(&self, _params: &serde_json::Value) -> Option<f64> { None }
    fn estimated_duration(&self, _params: &serde_json::Value) -> Option<Duration> { None }
    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement { ApprovalRequirement::default() }

    /// Return the argument keys that name filesystem paths. The dispatcher
    /// uses these to consult the SafetyManager's PathPolicy before invoking
    /// `execute`. Default impl returns empty — tools without path args (web,
    /// plan, exit_plan_mode, etc.) inherit this.
    fn path_args<'a>(&self, _arguments: &'a serde_json::Value) -> Vec<&'a str> {
        Vec::new()
    }

    /// If this tool **writes** to a file, return the path it will write.
    /// Drives the auto-preview popup: the dispatcher includes this string
    /// in the `chat:stream-tool-activity` event payload so the frontend
    /// can open the preview panel without having to maintain a hardcoded
    /// `Set<toolName>` of "write-ish" tools (the failure mode Proma's
    /// auto-preview suffers from — renaming `Write` → `write_v2` silently
    /// disables the trigger).
    ///
    /// Default returns `None` — non-mutating tools (search/grep/web/...)
    /// inherit this. WriteFile, Edit, and other mutating tools override
    /// to extract their path from `args`.
    fn preview_target_path(&self, _args: &serde_json::Value) -> Option<String> {
        None
    }

    /// 该工具是否并行安全。默认 `Sequential`(= 旧 PARALLEL_SAFE_TOOLS 白名单之外)。
    /// 只读工具 override 为 `Parallel`。
    fn concurrency(&self) -> ToolConcurrency { ToolConcurrency::Sequential }
}

pub async fn execute_tool_with_context(
    tool: &dyn Tool,
    params: serde_json::Value,
    _ctx: &ToolExecutionContext,
) -> Result<ToolOutput, ToolError> {
    tool.execute(params).await
}

/// 流式版本的 `execute_tool_with_context`。dispatcher 串行路径在工具
/// `supports_streaming()` 时调用它,把 sink 传进工具。
pub async fn execute_streaming_with_context(
    tool: &dyn Tool,
    params: serde_json::Value,
    _ctx: &ToolExecutionContext,
    sink: crate::agent::tools::stream::ToolStreamSink,
) -> Result<ToolOutput, ToolError> {
    tool.execute_streaming(params, sink).await
}

/// Tool registry
pub struct ToolRegistry {
    tools: std::collections::HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: std::collections::HashMap::new() }
    }

    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Box::new(tool));
    }

    /// Register a pre-boxed `Tool` instance. Used by `AgentApi.build_session_registry`
    /// where descriptor builders return `Box<dyn Tool>` (the concrete type is
    /// erased at registration time).
    pub fn register_boxed(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn list_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs: Vec<ToolDefinition> = self.tools.iter().map(|(name, tool)| {
            ToolDefinition {
                name: name.clone(),
                description: tool.description().to_string(),
                parameters: tool.parameters_schema(),
            }
        }).collect();
        // Deterministic order ensures Anthropic prompt cache breakpoint lands
        // on the same tool every iteration — random HashMap order breaks caching.
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        defs
    }

    pub fn len(&self) -> usize { self.tools.len() }
    pub fn is_empty(&self) -> bool { self.tools.is_empty() }
}

impl Default for ToolRegistry {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
#[path = "tool_tests.rs"]
mod tests;

#[cfg(test)]
mod stream_default_tests {
    use super::*;
    use crate::agent::tools::stream::ToolStreamSink;

    struct DummyTool;
    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str { "dummy" }
        fn description(&self) -> &str { "" }
        fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({}) }
        async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::new(serde_json::json!({"ok": true}), 0))
        }
    }

    #[tokio::test]
    async fn default_execute_streaming_delegates_to_execute() {
        let tool = DummyTool;
        assert!(!tool.supports_streaming());
        let out = tool.execute_streaming(serde_json::json!({}), ToolStreamSink::noop()).await.unwrap();
        assert_eq!(out.result["ok"], serde_json::json!(true));
    }
}

#[cfg(test)]
mod concurrency_tests {
    use super::*;
    use crate::agent::tools::builtin::file::ReadFileTool;
    use crate::agent::tools::builtin::get_file_skeleton::GetFileSkeletonTool;
    use crate::agent::tools::builtin::shell::BashTool;
    use std::path::PathBuf;

    #[test]
    fn read_only_tools_are_parallel() {
        let ws = PathBuf::from("/tmp");
        assert_eq!(ReadFileTool::new(ws.clone()).concurrency(), ToolConcurrency::Parallel);
        assert_eq!(GetFileSkeletonTool::new(ws.clone()).concurrency(), ToolConcurrency::Parallel);
    }

    #[test]
    fn other_tools_default_sequential() {
        let ws = PathBuf::from("/tmp");
        assert_eq!(BashTool::new(ws).concurrency(), ToolConcurrency::Sequential);
    }
}
