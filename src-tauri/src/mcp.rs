//! MCP (Model Context Protocol) client integration.
//!
//! Manages connections to MCP servers for extended tool capabilities.
//! Supports stdio (subprocess) and HTTP transports with JSON-RPC 2.0.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, RwLock, oneshot};
use tokio::task::JoinHandle;

// ─── JSON-RPC 2.0 Protocol Types ───────────────────────────────────────

pub const PROTOCOL_VERSION: &str = "2024-11-05";

// ─── PR-3 — auto-reconnect / health loop tunables ──────────────────────

/// How often the per-server health loop pings the MCP server. 30s is
/// chosen as the smallest interval that's clearly "background" rather
/// than "spammy"; users with multiple servers won't see noisy logs.
pub const HEALTH_PING_INTERVAL_SECS: u64 = 30;

/// First reconnect attempt fires this long after a ping failure.
/// Subsequent attempts double the delay up to `RECONNECT_MAX_DELAY_SECS`.
pub const RECONNECT_INITIAL_DELAY_SECS: u64 = 10;

/// Hard ceiling on the per-attempt reconnect wait. 5 minutes matches
/// the spirit of "don't hammer a dead server" without making recovery
/// feel hopeless if it comes back later.
pub const RECONNECT_MAX_DELAY_SECS: u64 = 300;

// ─── PR-4 — server notification routing ────────────────────────────────

/// Event surfaced by the stdio reader task whenever an MCP server
/// pushes a notification (JSON-RPC frame with `method` set, no `id`).
/// The manager-side consumer dispatches by method: today we only
/// special-case `notifications/tools/list_changed` (auto-refresh +
/// frontend event). Other methods log at debug for forward-compat —
/// future spec additions surface here without code changes.
#[derive(Debug, Clone)]
pub struct McpNotificationEvent {
    pub server_id: String,
    pub method: String,
    pub params: serde_json::Value,
}

/// Method string for the canonical "tools list changed" notification
/// defined by the MCP spec. uClaw declares
/// `capabilities.tools.listChanged = true` in `initialize` (line 56)
/// so well-behaved servers will fire this whenever they add/remove a
/// tool while connected.
pub const NOTIFY_TOOLS_LIST_CHANGED: &str = "notifications/tools/list_changed";

// ─── Sprint 2.2.5a — gbrain init timeout ───────────────────────────────

/// Hard ceiling on `gbrain init --pglite --yes` wall-clock duration.
/// PGLite's 63 migrations on a cold Apple Silicon disk normally finish
/// in 30-60s; 120s is the "something went very wrong" cliff after which
/// we give up rather than hang the entire app boot.
///
/// On timeout, the caller treats it as a regular init failure: app
/// continues to boot, gbrain seed step proceeds anyway (so the entry
/// appears in Integrations UI with an actionable error), and the user
/// can re-run `scripts/init-gbrain.sh` manually.
///
/// Lower would risk false positives on slow disks (Time Machine restore,
/// network home, encrypted disk with high CPU load). Higher delays the
/// "something's wrong" feedback for users with truly stuck inits.
pub const GBRAIN_INIT_TIMEOUT_SECS: u64 = 120;

// ─── Sprint 2.2.5b — init status for diagnostics ───────────────────────

/// Last-known outcome of the gbrain init step, persisted in `AppState`
/// for the duration of the app process. Read by `get_system_diagnostics`
/// and surfaced in the Settings → 系统 tab so users can see (and act on)
/// init failures instead of only finding them in logs.
///
/// Lifecycle: starts at `NotAttempted`. Stage 3 boot moves it to
/// `InProgress` before calling `ensure_bundled_gbrain_initialized`, then
/// to one of `Succeeded` / `SkippedAlreadyInitialized` / `Failed` based
/// on the result. Becomes `BundleMissing` if Stage 3 couldn't even find
/// the bun + gbrain entry paths (resource bundle absent / dev scripts
/// not run yet).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GbrainInitStatus {
    /// Stage 3 hasn't reached the gbrain init step yet, or the app
    /// is mid-boot.
    NotAttempted,
    /// Init currently running (sub-second window since the seed step
    /// is awaited inline, but exposed for completeness so UI doesn't
    /// flicker between NotAttempted and Succeeded).
    InProgress,
    /// `gbrain init` completed cleanly. `duration_ms` = wall-clock cost
    /// of the spawn; `at_ms` = unix-ms of completion.
    Succeeded { duration_ms: u64, at_ms: i64 },
    /// `PG_VERSION` was already present at boot; no spawn happened.
    /// Most steady-state boots land here. `at_ms` = when the probe ran.
    SkippedAlreadyInitialized { at_ms: i64 },
    /// `gbrain init` exited non-zero OR timed out OR spawn failed.
    /// `error` = short summary; `stderr_tail` = last 20 lines of stderr
    /// when available (None for spawn failures that never ran a child).
    /// `at_ms` = when the failure was observed.
    Failed {
        error: String,
        stderr_tail: Option<String>,
        at_ms: i64,
    },
    /// Stage 3 couldn't resolve `bun` and/or the gbrain entry path —
    /// resource bundle missing on fresh checkout, or dev's
    /// `setup-bun-runtime.sh` + `setup-gbrain-source.sh` not run yet.
    /// Init isn't even attempted in this state; gbrain MCP entry is
    /// skipped entirely.
    BundleMissing,
}

impl Default for GbrainInitStatus {
    fn default() -> Self {
        Self::NotAttempted
    }
}

// ─── PR-5 — env redaction + audit log ──────────────────────────────────

/// Replace any substring matching one of `env`'s values with
/// `[REDACTED]`. Used on every error message that goes to the UI / audit
/// log so a subprocess spawn failure can't leak `GITHUB_TOKEN=ghp_xxx`
/// in a screenshot or shared log.
///
/// We only redact values longer than 4 chars (avoids false positives on
/// boolean-ish env values like `1` or `true` that often appear in
/// general error strings). Empty values are skipped for the same
/// reason — `str::replace("", _)` infinite-loops on some allocators
/// and isn't useful anyway.
pub fn redact_env_values(s: &str, env: &HashMap<String, String>) -> String {
    let mut out = s.to_string();
    for v in env.values() {
        if v.len() >= 5 {
            out = out.replace(v, "[REDACTED]");
        }
    }
    out
}

fn diagnostic_error_summary(s: &str, env: &HashMap<String, String>) -> String {
    let redacted = redact_env_values(s, env);
    let lower = redacted.to_lowercase();
    let kind = if lower.contains("timed out waiting for pglite lock") {
        "pglite_lock_timeout"
    } else if lower.contains("no brain configured") || lower.contains("pg_version") {
        "pglite_not_ready"
    } else if lower.contains("permission denied") {
        "permission_denied"
    } else if lower.contains("gbrain_home") || lower.contains("pglite_data_dir") {
        "path_mismatch"
    } else if lower.contains("timeout waiting for response")
        || (lower.contains("gbrain cli") && lower.contains("timed out"))
    {
        "mcp_connect_timeout"
    } else if lower.contains("sigkill") || lower.contains("signal: 9") {
        "process_killed"
    } else if lower.contains("page_not_found") {
        "page_not_found"
    } else if lower.contains("failed to spawn") || lower.contains("no such file") {
        "launcher_missing_or_unusable"
    } else {
        "tool_call_failed"
    };

    let status = if lower.contains("signal: 9") {
        "signal: 9"
    } else if lower.contains("timed out") {
        "timed out"
    } else if lower.contains("exit status: 1") {
        "exit status: 1"
    } else if lower.contains("exit status: 2") {
        "exit status: 2"
    } else if lower.contains("exit status") {
        "exit status: nonzero"
    } else {
        ""
    };
    if status.is_empty() {
        format!("diagnostic_kind={kind}")
    } else {
        format!("diagnostic_kind={kind}; status={}", status.chars().take(160).collect::<String>())
    }
}

fn classify_gbrain_cli_failure(stderr: &str, status: &str) -> String {
    let lower = format!("{} {}", stderr, status).to_lowercase();
    if lower.contains("timed out waiting for pglite lock") {
        "pglite_lock_timeout"
    } else if lower.contains("no brain configured") || lower.contains("pg_version") {
        "pglite_not_ready"
    } else if lower.contains("permission denied") {
        "permission_denied"
    } else if lower.contains("gbrain_home") || lower.contains("pglite_data_dir") {
        "path_mismatch"
    } else if lower.contains("sigkill") || lower.contains("signal: 9") {
        "process_killed"
    } else if lower.contains("page_not_found") {
        "page_not_found"
    } else if lower.contains("failed to spawn") || lower.contains("no such file") {
        "launcher_missing_or_unusable"
    } else if lower.contains("cannot find module")
        || lower.contains("cannot find package")
        || lower.contains("error: cannot find")
    {
        // The bundled gbrain CLI is a Bun/TS project; this fires when its
        // node_modules aren't installed (e.g. `@electric-sql/pglite/vector`).
        "deps_missing"
    } else if lower.contains("timed out") {
        "timeout"
    } else {
        "unknown"
    }
    .to_string()
}

fn gbrain_cli_error_hint(kind: &str) -> &'static str {
    match kind {
        "page_not_found" => "Pick an existing slug from the suggestions or retry with fuzzy=true/include_deleted=true.",
        "process_killed" => "The gbrain CLI was killed by the OS. Retry with a smaller query/list size and check memory pressure if it repeats.",
        "timeout" => "The gbrain CLI timed out. Retry once, then restart gbrain if the problem repeats.",
        "pglite_lock_timeout" => "Stop stale gbrain processes and wait for the PGLite lock to clear, then retry.",
        "pglite_not_ready" => "Run gbrain init or restart the app so PGLite storage is ready.",
        "permission_denied" => "Fix permissions on the gbrain home directory or bundled launcher.",
        "path_mismatch" => "Refresh bundled gbrain configuration from System Diagnostics and restart gbrain.",
        "launcher_missing_or_unusable" => "Refresh bundled runtime paths and restart gbrain.",
        "deps_missing" => "gbrain's dependencies aren't installed (missing node module). Run scripts/setup-gbrain-source.sh — or `bun install` in the gbrain dir — then restart gbrain.",
        _ => "Open System Diagnostics for gbrain runtime details, then retry.",
    }
}

fn gbrain_cli_error_payload(tool: &str, kind: &str, status: &str, nearest_slugs: Vec<String>) -> String {
    serde_json::json!({
        "ok": false,
        "source": "gbrain",
        "tool": tool,
        "kind": kind,
        "status": status,
        "message": match kind {
            "page_not_found" => "gbrain page not found",
            "process_killed" => "gbrain process was killed",
            "timeout" => "gbrain CLI timed out",
            "pglite_lock_timeout" => "gbrain PGLite lock timed out",
            "pglite_not_ready" => "gbrain PGLite storage is not ready",
            "permission_denied" => "gbrain permission denied",
            "path_mismatch" => "gbrain runtime path mismatch",
            "launcher_missing_or_unusable" => "gbrain launcher missing or unusable",
            "deps_missing" => "gbrain dependencies not installed",
            _ => "gbrain CLI failed",
        },
        "hint": gbrain_cli_error_hint(kind),
        "nearest_slugs": nearest_slugs,
    })
    .to_string()
}

/// Kinds of events written to `mcp_audit`. Stored as the literal
/// string in the `event_kind` column; new variants are append-only so
/// historical rows stay parseable.
#[derive(Debug, Clone, Copy)]
pub enum McpAuditKind {
    ConnectAttempt,
    ConnectSucceeded,
    ConnectFailed,
    HealthFailed,
    Reconnected,
    Disconnect,
    Removed,
    ToolsChanged,
}

impl McpAuditKind {
    pub fn as_str(self) -> &'static str {
        match self {
            McpAuditKind::ConnectAttempt => "connect_attempt",
            McpAuditKind::ConnectSucceeded => "connect_succeeded",
            McpAuditKind::ConnectFailed => "connect_failed",
            McpAuditKind::HealthFailed => "health_failed",
            McpAuditKind::Reconnected => "reconnected",
            McpAuditKind::Disconnect => "disconnect",
            McpAuditKind::Removed => "removed",
            McpAuditKind::ToolsChanged => "tools_changed",
        }
    }
}

/// Single audit-log row as exposed to the frontend / list IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpAuditEntry {
    pub id: String,
    pub server_id: String,
    pub event_kind: String,
    pub message_redacted: String,
    pub created_at: i64,
}

/// Append one row to `mcp_audit`. Best-effort: a DB lock failure logs
/// + swallows. Caller passes a pre-redacted message (use
/// `redact_env_values` first).
pub fn append_audit_row(
    db: &Arc<std::sync::Mutex<rusqlite::Connection>>,
    server_id: &str,
    kind: McpAuditKind,
    message: &str,
) {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let conn = match db.lock() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("[mcp_audit] DB lock failed: {}", e);
            return;
        }
    };
    if let Err(e) = conn.execute(
        "INSERT INTO mcp_audit (id, server_id, event_kind, message_redacted, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, server_id, kind.as_str(), message, now],
    ) {
        tracing::warn!("[mcp_audit] INSERT failed: {}", e);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            method: method.into(),
            params,
        }
    }

    pub fn notification(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.into(),
            params,
        }
    }

    pub fn initialize(id: u64) -> Self {
        Self::new(
            id,
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "roots": { "listChanged": true }
                },
                "clientInfo": {
                    "name": "uclaw",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        )
    }

    pub fn initialized_notification() -> Self {
        Self::notification("notifications/initialized", None)
    }

    pub fn list_tools(id: u64) -> Self {
        Self::new(id, "tools/list", None)
    }

    pub fn call_tool(id: u64, name: &str, arguments: serde_json::Value) -> Self {
        Self::new(
            id,
            "tools/call",
            Some(serde_json::json!({
                "name": name,
                "arguments": arguments
            })),
        )
    }

    pub fn list_resources(id: u64) -> Self {
        Self::new(id, "resources/list", None)
    }

    pub fn read_resource(id: u64, uri: &str) -> Self {
        Self::new(
            id,
            "resources/read",
            Some(serde_json::json!({ "uri": uri })),
        )
    }

    pub fn ping(id: u64) -> Self {
        Self::new(id, "ping", None)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

// ─── MCP Protocol Result Types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    #[serde(default)]
    pub protocol_version: Option<String>,
    #[serde(default)]
    pub capabilities: ServerCapabilities,
    #[serde(default)]
    pub server_info: Option<ServerInfo>,
    #[serde(default)]
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(default)]
    pub tools: Option<serde_json::Value>,
    #[serde(default)]
    pub resources: Option<serde_json::Value>,
    #[serde(default)]
    pub prompts: Option<serde_json::Value>,
    #[serde(default)]
    pub logging: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListToolsResult {
    pub tools: Vec<McpRemoteTool>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRemoteTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_input_schema", alias = "input_schema")]
    pub input_schema: serde_json::Value,
}

fn default_input_schema() -> serde_json::Value {
    serde_json::json!({"type": "object", "properties": {}})
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolResult {
    pub content: Vec<ContentBlock>,
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ContentBlock {
    Text { text: String },
    Image { data: String, mime_type: String },
    Resource { resource: serde_json::Value },
}

// ─── MCP Server Status & Config ─────────────────────────────────────────

/// MCP server connection status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum McpServerStatus {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// Transport type for MCP server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    Stdio,
    Http,
}

impl Default for TransportType {
    fn default() -> Self {
        Self::Stdio
    }
}

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub transport_type: TransportType,
    /// Command to execute (stdio transport)
    #[serde(default)]
    pub command: String,
    /// Command arguments (stdio transport)
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables (stdio transport)
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// URL for HTTP transport
    #[serde(default)]
    pub url: Option<String>,
    pub enabled: bool,
    pub auto_approve: bool,
    /// When Some, only tools whose names appear in this list are registered
    /// into the agent ToolRegistry. `Some([])` intentionally exposes no raw
    /// tools. Others remain accessible to adapter code through the MCP manager
    /// but are hidden from the LLM — reducing tool definition tokens per call
    /// and preventing provider-routing bypasses. None = expose all tools.
    #[serde(default)]
    pub tool_allowlist: Option<Vec<String>>,
}

fn bundled_gbrain_tool_allowlist() -> Vec<String> {
    vec![
        "search".to_string(),
        "query".to_string(),
        "list_pages".to_string(),
        "think".to_string(),
        "get_page".to_string(),
        "put_page".to_string(),
    ]
}

pub fn playwright_mcp_tool_allowlist() -> Vec<String> {
    vec![
        "browser_snapshot".to_string(),
        "browser_navigate".to_string(),
        "browser_click".to_string(),
        "browser_type".to_string(),
        "browser_take_screenshot".to_string(),
        "browser_start_tracing".to_string(),
        "browser_stop_tracing".to_string(),
    ]
}

fn builtin_playwright_mcp_config() -> McpServerConfig {
    McpServerConfig {
        id: "playwright".to_string(),
        name: "Playwright MCP (built-in)".to_string(),
        description: "Official Playwright MCP server managed by uClaw Browser Automation."
            .to_string(),
        transport_type: TransportType::Stdio,
        command: "npx".to_string(),
        args: vec!["@playwright/mcp@latest".to_string()],
        env: HashMap::new(),
        url: None,
        enabled: true,
        auto_approve: false,
        tool_allowlist: Some(Vec::new()),
    }
}

fn bundled_gbrain_config(
    bun_path: &std::path::Path,
    entry_path: &std::path::Path,
    gbrain_home: &std::path::Path,
) -> McpServerConfig {
    let mut env = HashMap::new();
    env.insert(
        "GBRAIN_HOME".to_string(),
        gbrain_home.to_string_lossy().to_string(),
    );
    McpServerConfig {
        id: "gbrain".to_string(),
        name: "gbrain (bundled)".to_string(),
        description: "Local semantic-retrieval engine — wiki / entity-graph / dream-cycle. \
                     Bundled via Bun + gbrain source. PGLite brain at \
                     ~/.uclaw/gbrain/.gbrain/brain.pglite/."
            .to_string(),
        transport_type: TransportType::Stdio,
        command: bun_path.to_string_lossy().to_string(),
        args: vec![entry_path.to_string_lossy().to_string(), "serve".to_string()],
        env,
        url: None,
        enabled: true,
        auto_approve: true,
        tool_allowlist: Some(bundled_gbrain_tool_allowlist()),
    }
}

fn is_legacy_bundled_gbrain_script_wrapper(config: &McpServerConfig) -> bool {
    config.id == "gbrain"
        && config.transport_type == TransportType::Stdio
        && config.command.ends_with("/script")
        && config.args.iter().any(|arg| {
            arg.ends_with("gbrain/src/cli.ts") || arg.ends_with("gbrain-source/src/cli.ts")
        })
        && config.args.iter().any(|arg| arg == "serve")
}

/// MCP tool definition from a server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDef {
    pub server_id: String,
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// ─── Transport Trait ────────────────────────────────────────────────────

#[async_trait]
pub(crate) trait McpTransport: Send + Sync {
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, McpError>;
    async fn shutdown(&self) -> Result<(), McpError>;
}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("Transport error: {0}")]
    Transport(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Server error: {0}")]
    Server(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ─── Stdio Transport ────────────────────────────────────────────────────

struct StdioTransport {
    server_name: String,
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    reader_handle: Mutex<Option<JoinHandle<()>>>,
    stderr_handle: Mutex<Option<JoinHandle<()>>>,
    child: Arc<Mutex<tokio::process::Child>>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
}

impl StdioTransport {
    async fn spawn(
        name: impl Into<String>,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
        working_dir: Option<&Path>,
        // PR-4 — when `Some`, the stdout reader publishes JSON-RPC
        // notifications (frames with `method` but no `id`) onto this
        // sender keyed by the supplied `server_id`. `None` matches the
        // pre-PR-4 behaviour: notifications are logged + discarded.
        server_id: impl Into<String>,
        notification_tx: Option<tokio::sync::mpsc::UnboundedSender<McpNotificationEvent>>,
    ) -> Result<Self, McpError> {
        let server_name = name.into();
        let server_id = server_id.into();

        let mut cmd = tokio::process::Command::new(command);
        if let Some(working_dir) = working_dir {
            cmd.current_dir(working_dir);
        }
        cmd.args(args)
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            // Sprint 2.2 followon — kill the child when this StdioTransport
            // drops. Without this, a connect timeout or any failure path
            // that drops McpConnection leaves the child running, still
            // holding any external resource (most painfully, gbrain's
            // single-writer PGLite lock) so the next connect attempt's
            // freshly spawned child can't acquire the same lock and times
            // out with "Timed out waiting for PGLite lock." The original
            // 60s connect timeouts compounded this — each timeout left a
            // zombie that blocked the next connect. tokio defaults to
            // NOT killing children on drop (matches std behavior); we
            // opt in explicitly because we own the child's lifetime.
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| {
            McpError::Transport(format!(
                "[{}] Failed to spawn MCP server '{}': {}",
                server_name, command, e
            ))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            McpError::Transport(format!("[{}] Failed to capture stdin", server_name))
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            McpError::Transport(format!("[{}] Failed to capture stdout", server_name))
        })?;

        let stderr = child.stderr.take().ok_or_else(|| {
            McpError::Transport(format!("[{}] Failed to capture stderr", server_name))
        })?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(40)));

        // Spawn stdout reader
        let reader_pending = pending.clone();
        let reader_name = server_name.clone();
        let reader_server_id = server_id.clone();
        let reader_notify = notification_tx.clone();
        let reader_handle = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let value = match serde_json::from_str::<serde_json::Value>(&line) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::debug!("[{}] Failed to parse JSON-RPC: {}", reader_name, e);
                        continue;
                    }
                };

                // Check if it's a response (has result or error, no method)
                if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
                    // PR-4 — route via the notification channel when wired.
                    // Notifications have `method` and no `id`; the same
                    // frame shape technically encodes server-side
                    // requests (with `id`) but uClaw doesn't expose any
                    // `sampling/createMessage`-style handler today, so
                    // we treat both as fire-and-forget events.
                    tracing::debug!(
                        "[{}] Received server notification: {}",
                        reader_name,
                        method
                    );
                    if let Some(tx) = reader_notify.as_ref() {
                        let event = McpNotificationEvent {
                            server_id: reader_server_id.clone(),
                            method: method.to_string(),
                            params: value
                                .get("params")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null),
                        };
                        // send() returns Err only when every receiver
                        // is dropped — log + continue (the consumer
                        // task died for some reason, but the reader
                        // should keep up its end of the protocol).
                        if let Err(e) = tx.send(event) {
                            tracing::warn!(
                                "[{}] Failed to forward notification: {}",
                                reader_name,
                                e
                            );
                        }
                    }
                    continue;
                }

                let response: JsonRpcResponse = match serde_json::from_value(value) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::debug!("[{}] Failed to parse response: {}", reader_name, e);
                        continue;
                    }
                };

                if let Some(id) = response.id {
                    let mut map = reader_pending.lock().await;
                    if let Some(tx) = map.remove(&id) {
                        let _ = tx.send(response);
                    } else {
                        tracing::debug!("[{}] Response for unknown id {}", reader_name, id);
                    }
                }
            }
            tracing::debug!("[{}] JSON-RPC reader finished", reader_name);
        });

        // Spawn stderr reader
        let stderr_name = server_name.clone();
        let stderr_tail_reader = stderr_tail.clone();
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!("[{}] stderr: {}", stderr_name, line);
                let mut tail = stderr_tail_reader.lock().await;
                if tail.len() >= 40 {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        });

        Ok(Self {
            server_name,
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            reader_handle: Mutex::new(Some(reader_handle)),
            stderr_handle: Mutex::new(Some(stderr_handle)),
            child: Arc::new(Mutex::new(child)),
            stderr_tail,
        })
    }

    async fn stderr_tail_message(&self) -> Option<String> {
        let tail = self.stderr_tail.lock().await;
        if tail.is_empty() {
            None
        } else {
            Some(tail.iter().cloned().collect::<Vec<_>>().join("\n"))
        }
    }

    async fn error_with_stderr_tail(&self, prefix: String, timeout: bool) -> McpError {
        let msg = match self.stderr_tail_message().await {
            Some(tail) => format!("{}\nstderr tail:\n{}", prefix, tail),
            None => prefix,
        };
        if timeout {
            McpError::Timeout(msg)
        } else {
            McpError::Transport(msg)
        }
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, McpError> {
        let json = serde_json::to_string(request).map_err(|e| {
            McpError::Protocol(format!("Failed to serialize request: {}", e))
        })?;

        // For notifications (no id), just send and return empty response
        if request.id.is_none() {
            let mut writer = self.stdin.lock().await;
            writer.write_all(json.as_bytes()).await.map_err(|e| {
                McpError::Transport(format!("[{}] Write failed: {}", self.server_name, e))
            })?;
            writer.write_all(b"\n").await.map_err(|e| {
                McpError::Transport(format!("[{}] Write newline failed: {}", self.server_name, e))
            })?;
            writer.flush().await.map_err(|e| {
                McpError::Transport(format!("[{}] Flush failed: {}", self.server_name, e))
            })?;
            return Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: None,
                result: None,
                error: None,
            });
        }

        let id = request.id.unwrap();
        let (tx, rx) = oneshot::channel();

        {
            let mut map = self.pending.lock().await;
            map.insert(id, tx);
        }

        // Write request
        {
            let mut writer = self.stdin.lock().await;
            if let Err(e) = async {
                writer.write_all(json.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
                Ok::<_, std::io::Error>(())
            }.await {
                let mut map = self.pending.lock().await;
                map.remove(&id);
                return Err(self.error_with_stderr_tail(
                    format!("[{}] Write failed: {}", self.server_name, e),
                    false,
                )
                .await);
            }
        }

        // Wait for response with method-aware timeout.
        //
        // Sprint 2.2 followon: `tools/call` gets 5 minutes because a single
        // tool call can legitimately do heavy work (gbrain's put_page chunks +
        // embeds + writes — easily 30s on the first call, occasionally longer
        // for large content). All other methods (initialize, tools/list, ping,
        // notifications, etc.) keep the 60s default — they should never need
        // more than a few seconds in a healthy server.
        //
        // The 60s health ping interval upstream then can't collide with an
        // in-flight tool call within the same timeout window: tool call has
        // 5min headroom, ping has 60s, so ping fires + completes (or fails
        // quickly) without aborting the slower tool call.
        let timeout_secs = if request.method == "tools/call" { 300 } else { 60 };
        match tokio::time::timeout(Duration::from_secs(timeout_secs), rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                let mut map = self.pending.lock().await;
                map.remove(&id);
                Err(self.error_with_stderr_tail(
                    format!(
                        "[{}] Server closed connection before responding",
                        self.server_name
                    ),
                    false,
                )
                .await)
            }
            Err(_) => {
                let mut map = self.pending.lock().await;
                map.remove(&id);
                Err(self.error_with_stderr_tail(
                    format!(
                        "[{}] Timeout waiting for response to request {}",
                        self.server_name, id
                    ),
                    true,
                )
                .await)
            }
        }
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        {
            let mut child = self.child.lock().await;
            let _ = child.kill().await;
        }
        if let Some(handle) = self.reader_handle.lock().await.take() {
            handle.abort();
        }
        if let Some(handle) = self.stderr_handle.lock().await.take() {
            handle.abort();
        }
        {
            let mut pending = self.pending.lock().await;
            pending.clear();
        }
        tracing::debug!("[{}] Stdio transport shut down", self.server_name);
        Ok(())
    }
}

// ─── HTTP Transport ─────────────────────────────────────────────────────

struct HttpTransport {
    server_name: String,
    url: String,
    client: reqwest::Client,
}

impl HttpTransport {
    fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            server_name: name.into(),
            url: url.into(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, McpError> {
        let resp = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| {
                McpError::Transport(format!(
                    "[{}] HTTP request failed: {}",
                    self.server_name, e
                ))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(McpError::Transport(format!(
                "[{}] HTTP {} — {}",
                self.server_name, status, body
            )));
        }

        // For notifications, the server may return 202 with no body
        if request.id.is_none() {
            return Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: None,
                result: None,
                error: None,
            });
        }

        let response: JsonRpcResponse = resp.json().await.map_err(|e| {
            McpError::Protocol(format!(
                "[{}] Failed to parse HTTP response: {}",
                self.server_name, e
            ))
        })?;

        Ok(response)
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        tracing::debug!("[{}] HTTP transport shut down", self.server_name);
        Ok(())
    }
}

// ─── Bundled gbrain CLI Transport ──────────────────────────────────────

/// The bundled gbrain source currently runs on the embedded Bun runtime.
/// Its CLI one-shot commands are reliable, but the MCP SDK stdio server
/// can hang during the persistent `initialize` handshake under Bun pipes.
///
/// For uClaw's bundled local brain we keep the existing MCP-facing shape
/// (`mcp__gbrain__search`, etc.) while executing each tool call through
/// `bun <gbrain>/src/cli.ts <command> ...`. This makes the bridge
/// deterministic and avoids long-lived PGLite lock holders.
struct GbrainCliTransport {
    command: String,
    base_args: Vec<String>,
    env: HashMap<String, String>,
    /// Bundle 7-B — serialize CLI invocations.
    ///
    /// Every `call_cli` spawns a fresh `bun gbrain/src/cli.ts` subprocess
    /// (Bun runtime ~30–50 MB + PGLite init). When the agent runs
    /// multiple gbrain tools in parallel (e.g. `list_pages` + `search` +
    /// `get_page` in the same turn), each spawn lands at the same time
    /// and macOS's memory pressure killer issues SIGKILL — observable
    /// as `diagnostic_kind=process_killed; status=signal: 9` in the
    /// settings panel.
    ///
    /// A per-transport Mutex caps concurrency at 1 in-flight subprocess.
    /// Sequential is slightly slower under heavy parallel tool fan-out,
    /// but a single 45s tool call is still much faster than every call
    /// failing with SIGKILL and forcing the agent to retry.
    ///
    /// Note: this does NOT restore the persistent `gbrain serve` model
    /// (Bundle 7-B explicitly avoids re-introducing the PGLite-lock /
    /// serve-startup-timeout issues that motivated the original revert
    /// at 3e49bc5). It's the cheapest safe fix for the SIGKILL symptom.
    call_lock: Arc<tokio::sync::Mutex<()>>,
}

impl GbrainCliTransport {
    fn new(_server_name: &str, command: &str, args: &[String], env: &HashMap<String, String>) -> Self {
        let mut base_args = args.to_vec();
        if base_args.last().map(|s| s.as_str()) == Some("serve") {
            base_args.pop();
        }
        Self {
            command: command.to_string(),
            base_args,
            env: env.clone(),
            call_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn tools() -> Vec<McpRemoteTool> {
        vec![
            Self::tool(
                "search",
                "Keyword search using gbrain full-text search.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "limit": {"type": "number"},
                        "offset": {"type": "number"}
                    },
                    "required": ["query"]
                }),
            ),
            Self::tool(
                "query",
                "Hybrid semantic search across the local gbrain knowledge base.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "limit": {"type": "number"},
                        "offset": {"type": "number"},
                        "expand": {"type": "boolean"},
                        "detail": {"type": "string"},
                        "salience": {"type": "string"},
                        "recency": {"type": "string"},
                        "since": {"type": "string"},
                        "until": {"type": "string"},
                        "source_id": {"type": "string"}
                    }
                }),
            ),
            Self::tool(
                "list_pages",
                "List gbrain pages. Use this for 'what memories/knowledge do you have' and inventory questions instead of query('*').",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "type": {"type": "string"},
                        "tag": {"type": "string"},
                        "limit": {"type": "number"},
                        "updated_after": {"type": "string"},
                        "sort": {
                            "type": "string",
                            "enum": ["updated_desc", "updated_asc", "created_desc", "slug"]
                        },
                        "include_deleted": {"type": "boolean"}
                    }
                }),
            ),
            Self::tool(
                "think",
                "Multi-hop synthesis across pages, takes, and graph evidence.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "question": {"type": "string"},
                        "anchor": {"type": "string"},
                        "rounds": {"type": "number"},
                        "since": {"type": "string"},
                        "until": {"type": "string"}
                    },
                    "required": ["question"]
                }),
            ),
            Self::tool(
                "get_page",
                "Read a gbrain page by slug.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "slug": {"type": "string"},
                        "fuzzy": {"type": "boolean"},
                        "include_deleted": {"type": "boolean"}
                    },
                    "required": ["slug"]
                }),
            ),
            Self::tool(
                "put_page",
                "Write or update a gbrain page from markdown content.",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "slug": {"type": "string"},
                        "content": {"type": "string"}
                    },
                    "required": ["slug", "content"]
                }),
            ),
        ]
    }

    fn tool(name: &str, description: &str, input_schema: serde_json::Value) -> McpRemoteTool {
        McpRemoteTool {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
        }
    }

    async fn call_cli(&self, tool: &str, arguments: serde_json::Value) -> Result<String, McpError> {
        // Bundle 7-B — gate the whole CLI invocation behind the
        // per-transport mutex. Held until `output.status` is read,
        // which is also when the bun subprocess has fully exited
        // (kill_on_drop guarantees cleanup on early return / panic).
        // The PGLite lock cleanup inside the critical section then has
        // race-free semantics: at most one cleanup + spawn at a time.
        let _call_guard = self.call_lock.lock().await;

        cleanup_stale_pglite_lock(&self.env);

        let mut argv = self.base_args.clone();
        let mut requested_slug: Option<String> = None;
        match tool {
            "search" => {
                let query = required_string(&arguments, "query")?;
                argv.push("search".to_string());
                argv.push(query);
                push_number_flag(&mut argv, &arguments, "limit", "--limit");
                push_number_flag(&mut argv, &arguments, "offset", "--offset");
            }
            "query" => {
                let query = optional_string(&arguments, "query").unwrap_or_default();
                argv.push("query".to_string());
                argv.push(query);
                push_number_flag(&mut argv, &arguments, "limit", "--limit");
                push_number_flag(&mut argv, &arguments, "offset", "--offset");
                push_bool_flag(&mut argv, &arguments, "expand", "--expand");
                push_string_flag(&mut argv, &arguments, "detail", "--detail");
                push_string_flag(&mut argv, &arguments, "salience", "--salience");
                push_string_flag(&mut argv, &arguments, "recency", "--recency");
                push_string_flag(&mut argv, &arguments, "since", "--since");
                push_string_flag(&mut argv, &arguments, "until", "--until");
                push_string_flag(&mut argv, &arguments, "source_id", "--source-id");
            }
            "list_pages" => {
                argv.push("list".to_string());
                push_string_flag(&mut argv, &arguments, "type", "--type");
                push_string_flag(&mut argv, &arguments, "tag", "--tag");
                push_number_flag(&mut argv, &arguments, "limit", "--limit");
                push_string_flag(&mut argv, &arguments, "updated_after", "--updated-after");
                push_string_flag(&mut argv, &arguments, "sort", "--sort");
                push_bool_flag(&mut argv, &arguments, "include_deleted", "--include-deleted");
            }
            "think" => {
                let question = required_string(&arguments, "question")?;
                argv.push("think".to_string());
                argv.push(question);
                push_string_flag(&mut argv, &arguments, "anchor", "--anchor");
                push_number_flag(&mut argv, &arguments, "rounds", "--rounds");
                push_string_flag(&mut argv, &arguments, "since", "--since");
                push_string_flag(&mut argv, &arguments, "until", "--until");
            }
            "get_page" => {
                let slug = required_string(&arguments, "slug")?;
                requested_slug = Some(slug.clone());
                argv.push("get".to_string());
                argv.push(slug);
                push_bool_flag(&mut argv, &arguments, "fuzzy", "--fuzzy");
                push_bool_flag(&mut argv, &arguments, "include_deleted", "--include-deleted");
            }
            "put_page" => {
                let slug = required_string(&arguments, "slug")?;
                let content = required_string(&arguments, "content")?;
                argv.push("put".to_string());
                argv.push(slug);
                argv.push("--content".to_string());
                argv.push(content);
            }
            "get_backlinks" => {
                let slug = required_string(&arguments, "slug")?;
                argv.push("backlinks".to_string());
                argv.push(slug);
            }
            "traverse_graph" => {
                let slug = required_string(&arguments, "slug")?;
                argv.push("graph".to_string());
                argv.push(slug);
                push_number_flag(&mut argv, &arguments, "depth", "--depth");
                push_string_flag(&mut argv, &arguments, "direction", "--direction");
            }
            "get_links" => {
                let slug = required_string(&arguments, "slug")?;
                argv.push("graph".to_string());
                argv.push(slug);
                argv.push("--depth".to_string());
                argv.push("1".to_string());
            }
            "get_versions" => {
                let slug = required_string(&arguments, "slug")?;
                argv.push("history".to_string());
                argv.push(slug);
            }
            "get_stats" => {
                argv.push("stats".to_string());
            }
            "find_orphans" => {
                argv.push("orphans".to_string());
                argv.push("--json".to_string());
            }
            "revert_version" => {
                let slug = required_string(&arguments, "slug")?;
                let vid = arguments
                    .get("version_id")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| McpError::Server("revert_version: version_id (number) required".into()))?;
                argv.push("revert".to_string());
                argv.push(slug);
                argv.push(vid.to_string());
            }
            other => {
                return Err(McpError::Server(format!(
                    "gbrain CLI transport does not support tool '{}'",
                    other
                )));
            }
        }

        // Bundle 14-B — observability around the bun spawn. The
        // SIGKILL-in-4ms symptom doesn't fit OOM (system idle, 64GB
        // free) so we need to see what env + argv are actually being
        // launched. Logged at DEBUG to keep production trace clean,
        // but easy to surface with RUST_LOG=uclaw_core::mcp=debug.
        let spawn_start = std::time::Instant::now();
        tracing::debug!(
            tool,
            command = %self.command,
            argv = ?argv,
            env_keys = ?self.env.keys().collect::<Vec<_>>(),
            "[gbrain-cli] spawning bun subprocess"
        );

        let mut cmd = tokio::process::Command::new(&self.command);
        cmd.args(&argv)
            .envs(&self.env)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let output = tokio::time::timeout(Duration::from_secs(45), cmd.output())
            .await
            .map_err(|_| McpError::Server(gbrain_cli_error_payload(tool, "timeout", "timed out", Vec::new())))?
            .map_err(|e| McpError::Io(e))?;

        // Bundle 14-B — log spawn timing + exit details on the way out.
        // For a SIGKILL'd process: status.code()=None, status.to_string()
        // is "signal: N (NAME)". Stderr is captured even if process never
        // wrote anything (empty string in that case).
        let elapsed_ms = spawn_start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !output.status.success() {
            // Capture the stderr TAIL even for crash cases — for SIGKILL
            // we typically get nothing, but bun sometimes prints a
            // single line about quarantine / signing / library load
            // failures before being killed.
            tracing::warn!(
                tool,
                elapsed_ms,
                status = %output.status,
                stdout_len = stdout.len(),
                stderr_len = stderr.len(),
                stderr_tail = %stderr.chars().rev().take(500).collect::<String>().chars().rev().collect::<String>(),
                "[gbrain-cli] subprocess exited non-zero"
            );
            let mut suggestions = Vec::new();
            if tool == "get_page" && stderr.contains("page_not_found") {
                if let Some(slug) = requested_slug.as_deref() {
                    suggestions = self.suggest_page_slugs(slug).await;
                }
            }
            return Err(McpError::Server(gbrain_cli_error_payload(
                tool,
                &classify_gbrain_cli_failure(&stderr, &output.status.to_string()),
                &output.status.to_string(),
                suggestions,
            )));
        }
        if stdout.is_empty() && !stderr.is_empty() {
            return Ok(stderr);
        }
        crate::gbrain::cli_format::to_mcp_json(tool, &arguments, &stdout)
    }

    async fn suggest_page_slugs(&self, missing_slug: &str) -> Vec<String> {
        // Bundle 7-B — no separate lock here: `suggest_page_slugs` is
        // only called from inside `call_cli`, which already holds the
        // per-transport mutex. Taking it again here would deadlock the
        // same task. Externally-callable variants would need their own
        // entry point that acquires the lock.

        let mut argv = self.base_args.clone();
        argv.push("list".to_string());
        argv.push("--limit".to_string());
        argv.push("200".to_string());

        let mut cmd = tokio::process::Command::new(&self.command);
        cmd.args(&argv)
            .envs(&self.env)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);

        let Ok(Ok(output)) = tokio::time::timeout(Duration::from_secs(10), cmd.output()).await else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut candidates: Vec<(usize, String)> = stdout
            .lines()
            .filter_map(|line| line.split('\t').next())
            .filter(|slug| !slug.trim().is_empty())
            .map(|slug| (slug_distance(missing_slug, slug), slug.to_string()))
            .collect();
        candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        candidates
            .into_iter()
            .take(3)
            .map(|(_, slug)| slug)
            .collect()
    }

}

#[async_trait]
impl McpTransport for GbrainCliTransport {
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, McpError> {
        match request.method.as_str() {
            "initialize" => Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(serde_json::json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "gbrain-cli", "version": env!("CARGO_PKG_VERSION")}
                })),
                error: None,
            }),
            "notifications/initialized" => Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(serde_json::Value::Null),
                error: None,
            }),
            "tools/list" => Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(serde_json::json!({ "tools": Self::tools() })),
                error: None,
            }),
            "tools/call" => {
                let params = request.params.as_ref().ok_or_else(|| {
                    McpError::Protocol("tools/call missing params".into())
                })?;
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::Protocol("tools/call missing name".into()))?;
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                let text = self.call_cli(name, args).await?;
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: Some(serde_json::json!({
                        "content": [{"type": "text", "text": text}],
                        "isError": false
                    })),
                    error: None,
                })
            }
            "ping" => Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(serde_json::json!({})),
                error: None,
            }),
            other => Err(McpError::Protocol(format!(
                "gbrain CLI transport does not implement method '{}'",
                other
            ))),
        }
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        Ok(())
    }
}

fn required_string(args: &serde_json::Value, key: &str) -> Result<String, McpError> {
    optional_string(args, key)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| McpError::Protocol(format!("missing required argument '{}'", key)))
}

fn slug_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];

    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = prev[j] + usize::from(ca != cb);
            let insertion = curr[j] + 1;
            let deletion = prev[j + 1] + 1;
            curr[j + 1] = substitution.min(insertion).min(deletion);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b.len()]
}

fn optional_string(args: &serde_json::Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn push_string_flag(argv: &mut Vec<String>, args: &serde_json::Value, key: &str, flag: &str) {
    if let Some(value) = optional_string(args, key).filter(|s| !s.is_empty()) {
        argv.push(flag.to_string());
        argv.push(value);
    }
}

fn push_number_flag(argv: &mut Vec<String>, args: &serde_json::Value, key: &str, flag: &str) {
    if let Some(value) = args.get(key).and_then(|v| v.as_f64()) {
        argv.push(flag.to_string());
        argv.push(if value.fract() == 0.0 {
            format!("{}", value as i64)
        } else {
            value.to_string()
        });
    }
}

fn push_bool_flag(argv: &mut Vec<String>, args: &serde_json::Value, key: &str, flag: &str) {
    if args.get(key).and_then(|v| v.as_bool()) == Some(true) {
        argv.push(flag.to_string());
    }
}

fn pid_is_alive(pid: i64) -> bool {
    if pid <= 0 {
        return false;
    }
    std::process::Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("pid=")
        .output()
        .map(|out| out.status.success() && !String::from_utf8_lossy(&out.stdout).trim().is_empty())
        .unwrap_or(false)
}

fn is_bundled_gbrain(config: &McpServerConfig) -> bool {
    config.id == "gbrain"
        && config.transport_type == TransportType::Stdio
        && config.args.last().map(|s| s.as_str()) == Some("serve")
        && config.args.iter().any(|arg| {
            arg.ends_with("gbrain/src/cli.ts") || arg.ends_with("gbrain-source/src/cli.ts")
        })
}

/// 清掉 gbrain PGLite 的崩溃残留单写锁(锁文件里的 PID 已不存活时删除)。
/// 在 spawn 持久 `gbrain serve` 前调用,避免上次 serve 崩溃留下的锁让新 serve
/// 卡在 "Timed out waiting for PGLite lock"。
fn cleanup_stale_pglite_lock(env: &HashMap<String, String>) {
    let Some(home) = env.get("GBRAIN_HOME") else { return; };
    let lock_dir = std::path::Path::new(home)
        .join(".gbrain")
        .join("brain.pglite")
        .join(".gbrain-lock");
    let lock_file = lock_dir.join("lock");
    let Ok(raw) = std::fs::read_to_string(&lock_file) else { return; };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else { return; };
    let Some(pid) = v.get("pid").and_then(|p| p.as_i64()) else { return; };
    if !pid_is_alive(pid) {
        if let Err(e) = std::fs::remove_dir_all(&lock_dir) {
            tracing::warn!(lock = %lock_dir.display(), error = %e, "Failed to remove stale gbrain PGLite lock");
        } else {
            tracing::warn!(pid, lock = %lock_dir.display(), "Removed stale gbrain PGLite lock for dead process");
        }
    }
}

// ─── MCP Client (per-server connection) ─────────────────────────────────

struct McpConnection {
    transport: Arc<dyn McpTransport>,
    next_id: AtomicU64,
    initialized: bool,
    tools: Vec<McpRemoteTool>,
    server_info: Option<ServerInfo>,
}

impl McpConnection {
    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    async fn initialize(&mut self) -> Result<InitializeResult, McpError> {
        let id = self.next_id();
        let request = JsonRpcRequest::initialize(id);
        let response = self.transport.send(&request).await?;

        if let Some(error) = &response.error {
            return Err(McpError::Server(format!(
                "Initialize failed: {}", error
            )));
        }

        let init_result: InitializeResult = response
            .result
            .ok_or_else(|| McpError::Protocol("No result in initialize response".into()))
            .and_then(|r| {
                serde_json::from_value(r).map_err(|e| {
                    McpError::Protocol(format!("Invalid initialize result: {}", e))
                })
            })?;

        self.server_info = init_result.server_info.clone();

        // Send initialized notification
        let notification = JsonRpcRequest::initialized_notification();
        if let Err(e) = self.transport.send(&notification).await {
            tracing::debug!("Failed to send initialized notification: {}", e);
        }

        self.initialized = true;
        Ok(init_result)
    }

    async fn discover_tools(&mut self) -> Result<Vec<McpRemoteTool>, McpError> {
        let id = self.next_id();
        let request = JsonRpcRequest::list_tools(id);
        let response = self.transport.send(&request).await?;

        if let Some(error) = &response.error {
            return Err(McpError::Server(format!("tools/list failed: {}", error)));
        }

        let result: ListToolsResult = response
            .result
            .ok_or_else(|| McpError::Protocol("No result in tools/list response".into()))
            .and_then(|r| {
                serde_json::from_value(r).map_err(|e| {
                    McpError::Protocol(format!("Invalid tools/list result: {}", e))
                })
            })?;

        self.tools = result.tools.clone();
        Ok(result.tools)
    }

    async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        let id = self.next_id();
        let request = JsonRpcRequest::call_tool(id, tool_name, arguments);
        let response = self.transport.send(&request).await?;

        if let Some(error) = &response.error {
            return Err(McpError::Server(format!("tools/call failed: {}", error)));
        }

        let result: CallToolResult = response
            .result
            .ok_or_else(|| McpError::Protocol("No result in tools/call response".into()))
            .and_then(|r| {
                serde_json::from_value(r).map_err(|e| {
                    McpError::Protocol(format!("Invalid tools/call result: {}", e))
                })
            })?;

        Ok(result)
    }

    async fn list_resources(&self) -> Result<serde_json::Value, McpError> {
        let id = self.next_id();
        let request = JsonRpcRequest::list_resources(id);
        let response = self.transport.send(&request).await?;

        if let Some(error) = &response.error {
            return Err(McpError::Server(format!("resources/list failed: {}", error)));
        }

        Ok(response.result.unwrap_or(serde_json::Value::Null))
    }

    async fn read_resource(&self, uri: &str) -> Result<serde_json::Value, McpError> {
        let id = self.next_id();
        let request = JsonRpcRequest::read_resource(id, uri);
        let response = self.transport.send(&request).await?;

        if let Some(error) = &response.error {
            return Err(McpError::Server(format!("resources/read failed: {}", error)));
        }

        Ok(response.result.unwrap_or(serde_json::Value::Null))
    }

    async fn ping(&self) -> Result<(), McpError> {
        let id = self.next_id();
        let request = JsonRpcRequest::ping(id);
        let response = self.transport.send(&request).await?;

        if let Some(error) = &response.error {
            return Err(McpError::Server(format!("ping failed: {}", error)));
        }

        Ok(())
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        self.transport.shutdown().await
    }
}

// ─── MCP Server Runtime State ───────────────────────────────────────────

/// MCP server runtime state
pub struct McpServerState {
    pub config: McpServerConfig,
    pub status: McpServerStatus,
    pub tools: Vec<McpToolDef>,
    pub error: Option<String>,
    connection: Option<McpConnection>,
}

// ─── MCP Tool Proxy (exposes MCP tools as Tool trait) ───────────────────

/// Prefix applied to every agent-facing MCP tool name. Lets the rest of
/// uClaw — SafetyManager, telemetry, the prompt manifest — distinguish
/// MCP-sourced tool calls from builtins at a glance via the tool name
/// alone (no need to consult a separate registry).
pub const MCP_TOOL_PREFIX: &str = "mcp__";

/// Build the agent-facing tool name for an MCP-proxied tool. The
/// format `mcp__{server_id}__{tool_name}` matches the convention used
/// by Cline / Roo / Claude Desktop and is what users will recognize.
pub fn prefixed_tool_name(server_id: &str, tool_name: &str) -> String {
    format!("{}{}__{}", MCP_TOOL_PREFIX, server_id, tool_name)
}

/// Inverse of `prefixed_tool_name`. Returns `Some((server_id, tool_name))`
/// when `name` matches the expected `mcp__SERVER__TOOL` shape, `None`
/// otherwise (so callers can fast-path non-MCP tool names without
/// expensive checks). The split is on the first `__` AFTER the prefix
/// so server ids containing single underscores are preserved.
pub fn parse_mcp_tool_name(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix(MCP_TOOL_PREFIX)?;
    let idx = rest.find("__")?;
    let (server_id, tail) = rest.split_at(idx);
    let tool_name = &tail[2..]; // strip the "__" separator
    if server_id.is_empty() || tool_name.is_empty() {
        return None;
    }
    Some((server_id, tool_name))
}

#[derive(Clone)]
pub struct McpToolProxy {
    /// Source server id — used to route the JSON-RPC call back through
    /// the right transport, and (with `tool_name`) to identify which
    /// MCP server a proxied call originated from when auditing.
    server_id: String,
    /// Raw MCP tool name as the server reported it (e.g. "create_issue").
    /// Used in the JSON-RPC `tools/call` request — must NOT include the
    /// uClaw-side `mcp__{server_id}__` prefix or the server won't know
    /// what to invoke.
    tool_name: String,
    /// Agent-facing tool name = `mcp__{server_id}__{tool_name}`. This is
    /// what `name()` returns to `ToolRegistry`, what shows up in the LLM
    /// tool manifest, and what `SafetyManager` keys on. The prefix
    /// guarantees uniqueness across servers (two MCP servers can ship
    /// identically-named tools without colliding).
    prefixed_name: String,
    description: String,
    input_schema: serde_json::Value,
    manager: SharedMcpManager,
    /// Snapshotted from `McpServerConfig.auto_approve` at proxy
    /// construction time. Drives `requires_approval` so SafetyManager
    /// can grant `Never` (no approval prompt) for tools sourced from
    /// servers the user marked as trusted in the Integrations UI.
    /// Snapshot is OK because changing the flag triggers a manager
    /// refresh which rebuilds proxies for the next agent turn.
    auto_approve: bool,
}

#[async_trait]
impl crate::agent::tools::tool::Tool for McpToolProxy {
    fn name(&self) -> &str {
        &self.prefixed_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.input_schema.clone()
    }

    /// Soft-honor the server's `auto_approve` flag. `Never` lets
    /// SafetyManager short-circuit straight to AutoApprove regardless
    /// of the active SafetyMode (see safety/mod.rs:221-224).
    /// `UnlessAutoApproved` keeps the normal Supervised-mode gating
    /// in place: the user can still allow specific tools via the
    /// auto-approved whitelist, but unknown calls require confirmation.
    fn requires_approval(
        &self,
        _params: &serde_json::Value,
    ) -> crate::agent::tools::tool::ApprovalRequirement {
        if self.auto_approve {
            crate::agent::tools::tool::ApprovalRequirement::Never
        } else {
            crate::agent::tools::tool::ApprovalRequirement::UnlessAutoApproved
        }
    }

    async fn execute(
        &self,
        params: serde_json::Value,
    ) -> Result<crate::agent::tools::tool::ToolOutput, crate::agent::tools::tool::ToolError> {
        let start = std::time::Instant::now();

        // Acquire read lock only to get the transport handle, then release immediately
        let (transport, req_id) = {
            let mgr = self.manager.read().await;
            mgr.get_transport(&self.server_id).map_err(|e| {
                crate::agent::tools::tool::ToolError::Execution(e.to_string())
            })?
        };
        // Lock is now released — execute the network call without holding it
        tracing::debug!("Calling MCP tool '{}' on server '{}' (lock-free)", self.tool_name, self.server_id);
        let request = JsonRpcRequest::call_tool(req_id, &self.tool_name, params);
        let response = transport.send(&request).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        let result = match response {
            Ok(resp) => {
                if let Some(error) = &resp.error {
                    Err(McpError::Server(format!("tools/call failed: {}", error)))
                } else {
                    resp.result
                        .ok_or_else(|| McpError::Protocol("No result in tools/call response".into()))
                        .and_then(|r| {
                            serde_json::from_value::<CallToolResult>(r).map_err(|e| {
                                McpError::Protocol(format!("Invalid tools/call result: {}", e))
                            })
                        })
                }
            }
            Err(e) => Err(e),
        };

        match result {
            Ok(call_result) => {
                let text = call_result
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                if call_result.is_error {
                    {
                        let mut mgr = self.manager.write().await;
                        if let Some(state) = mgr.servers.get_mut(&self.server_id) {
                            McpManager::set_error_for_state(state, Some(text.clone()));
                        }
                    }
                    Ok(crate::agent::tools::tool::ToolOutput::error(&text, duration_ms))
                } else {
                    {
                        let mut mgr = self.manager.write().await;
                        if let Some(state) = mgr.servers.get_mut(&self.server_id) {
                            McpManager::set_error_for_state(state, None);
                        }
                    }
                    Ok(crate::agent::tools::tool::ToolOutput::success(&text, duration_ms))
                }
            }
            Err(e) => {
                let error = e.to_string();
                {
                    let mut mgr = self.manager.write().await;
                    if let Some(state) = mgr.servers.get_mut(&self.server_id) {
                        McpManager::set_error_for_state(state, Some(error.clone()));
                    }
                }
                Ok(crate::agent::tools::tool::ToolOutput::error(&error, duration_ms))
            }
        }
    }
}

impl McpToolProxy {
    /// Construct a proxy for a plugin-declared tool.
    ///
    /// * `plugin_id`   — the plugin's manifest `id`; used as the MCP server id
    ///   so the call is routed through the right transport.
    /// * `tool_name`   — raw (un-prefixed) tool name as declared in the plugin
    ///   manifest's `contributes.tools` list.
    /// * `mcp_manager` — the shared MCP manager handle; cloned into the proxy so
    ///   it can acquire a read lock at call time.
    ///
    /// The `prefixed_name` is computed via `prefixed_tool_name` (same convention
    /// as the existing MCP server tool registration path).  `input_schema` starts
    /// as an empty object — the actual schema is provided by the subprocess at
    /// connect time; this is sufficient for boot-time descriptor registration.
    /// `auto_approve` defaults to `false` (requires approval unless the user
    /// marks the server trusted in the Integrations UI).
    pub fn for_plugin(
        plugin_id: String,
        tool_name: String,
        mcp_manager: SharedMcpManager,
    ) -> Self {
        let prefixed = prefixed_tool_name(&plugin_id, &tool_name);
        Self {
            server_id: plugin_id.clone(),
            tool_name: tool_name.clone(),
            prefixed_name: prefixed,
            description: format!("Plugin tool {tool_name} (server {plugin_id})"),
            input_schema: serde_json::json!({}),
            manager: mcp_manager,
            auto_approve: false,
        }
    }
}

// ─── MCP Manager ────────────────────────────────────────────────────────

/// MCP client manager
pub struct McpManager {
    servers: HashMap<String, McpServerState>,
    config_path: std::path::PathBuf,
    runtime_working_dirs: HashMap<String, PathBuf>,
    /// PR-3 — per-server health/reconnect task handles. Keyed by server
    /// id. Inserted by `start_health_loop`, aborted + removed by
    /// `stop_health_loop` (called on disconnect/remove). The handle is
    /// `pub(crate)` only — outside callers can't poke at the loops.
    health_tasks: HashMap<String, JoinHandle<()>>,
    /// PR-4 — shared sender pushed into every stdio transport at
    /// `connect_server` time. `None` until `set_notification_tx` is
    /// called (main.rs wires it once at boot). When None the reader
    /// tasks fall back to log-and-discard behaviour.
    notification_tx: Option<tokio::sync::mpsc::UnboundedSender<McpNotificationEvent>>,
    /// PR-5 — main app DB handle for writing `mcp_audit` rows. `None`
    /// until `set_db_handle` is called at boot. When None, audit writes
    /// silently no-op so unit tests don't need DB setup.
    db: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>,
}

impl McpManager {
    pub fn new(data_dir: &std::path::Path) -> Self {
        let config_path = data_dir.join("mcp_servers.json");
        let mut manager = Self {
            servers: HashMap::new(),
            config_path,
            runtime_working_dirs: HashMap::new(),
            health_tasks: HashMap::new(),
            notification_tx: None,
            db: None,
        };
        manager.load_config();
        manager
    }

    /// PR-4 — install the channel sender that every stdio transport
    /// will forward notifications onto. Called once at app boot from
    /// `main.rs`; the matching receiver lives in a tokio task that
    /// dispatches by method (today only `tools/list_changed` is
    /// special-cased).
    pub fn set_notification_tx(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<McpNotificationEvent>,
    ) {
        self.notification_tx = Some(tx);
    }

    /// PR-5 — install the app DB handle so lifecycle events get
    /// persisted to the `mcp_audit` table. Called once at boot.
    pub fn set_db_handle(
        &mut self,
        db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    ) {
        self.db = Some(db);
    }

    /// Set a process cwd override for a server at runtime without persisting it
    /// into `mcp_servers.json`. Built-in servers whose roots follow the active
    /// app session/workspace use this so changing workspace does not leave a
    /// stale absolute path in user configuration.
    pub fn set_runtime_working_dir(&mut self, id: &str, working_dir: Option<PathBuf>) {
        if let Some(working_dir) = working_dir {
            self.runtime_working_dirs.insert(id.to_string(), working_dir);
        } else {
            self.runtime_working_dirs.remove(id);
        }
    }

    pub fn runtime_working_dir(&self, id: &str) -> Option<PathBuf> {
        self.runtime_working_dirs.get(id).cloned()
    }

    /// PR-5 — helper that pairs `redact_env_values` with `append_audit_row`.
    /// Looks up the server's env (if known) for redaction; if the
    /// server doesn't exist (e.g. the audit row is for `Removed`) the
    /// message goes in verbatim. No-op when `db` isn't installed.
    fn record_audit(&self, server_id: &str, kind: McpAuditKind, message: &str) {
        let redacted = self
            .servers
            .get(server_id)
            .map(|s| redact_env_values(message, &s.config.env))
            .unwrap_or_else(|| message.to_string());
        if let Some(db) = self.db.as_ref() {
            append_audit_row(db, server_id, kind, &redacted);
        }
    }

    // ── Config Persistence ──────────────────────────────────────────

    fn load_config(&mut self) {
        if let Ok(content) = std::fs::read_to_string(&self.config_path) {
            if let Ok(servers) = serde_json::from_str::<Vec<McpServerConfig>>(&content) {
                for config in servers {
                    self.servers.insert(
                        config.id.clone(),
                        McpServerState {
                            config,
                            status: McpServerStatus::Disconnected,
                            tools: Vec::new(),
                            error: None,
                            connection: None,
                        },
                    );
                }
            }
        }
    }

    fn save_config(&self) {
        let configs: Vec<&McpServerConfig> =
            self.servers.values().map(|s| &s.config).collect();
        if let Ok(json) = serde_json::to_string_pretty(&configs) {
            let _ = std::fs::write(&self.config_path, json);
        }
    }

    // ── Server CRUD ─────────────────────────────────────────────────

    pub fn add_server(&mut self, config: McpServerConfig) -> Result<(), String> {
        if self.servers.contains_key(&config.id) {
            return Err(format!("Server {} already exists", config.id));
        }
        self.servers.insert(
            config.id.clone(),
            McpServerState {
                config,
                status: McpServerStatus::Disconnected,
                tools: Vec::new(),
                error: None,
                connection: None,
            },
        );
        self.save_config();
        Ok(())
    }

    /// gbrain Sprint 2.1 — seed the bundled gbrain stdio MCP entry if
    /// no entry with id="gbrain" already exists. Called once at boot
    /// from main.rs's Stage 3.
    ///
    /// Idempotent + non-destructive:
    /// - If the entry exists (regardless of `enabled`), do nothing.
    ///   That way users who explicitly disable / remove gbrain don't
    ///   get it re-added on every restart.
    /// - The entry is auto_approve=true because it's the bundled
    ///   service we ship + sign — same trust level as the local
    ///   user's filesystem (which builtin tools already get).
    ///
    /// Inputs:
    /// - `bun_path`: absolute path to `bunembed/bun` (resource or dev)
    /// - `entry_path`: absolute path to gbrain's CLI entry (resource
    ///   or dev `src/cli.ts`). Spawned via `bun <entry> serve`.
    /// - `gbrain_home`: writable directory that becomes `$GBRAIN_HOME`.
    ///   gbrain reads its config from `$GBRAIN_HOME/.gbrain/config.json`
    ///   (created by `ensure_bundled_gbrain_initialized`) and stores
    ///   PGLite data under `$GBRAIN_HOME/.gbrain/brain.pglite/`.
    ///   Caller MUST have invoked `ensure_bundled_gbrain_initialized`
    ///   first — without an initialized brain, gbrain serve exits
    ///   immediately on every connect attempt.
    ///
    /// Returns `Ok(true)` if seeded or a legacy bundled entry was
    /// migrated, `Ok(false)` if a non-legacy entry already existed
    /// (no-op). Errors propagate from `add_server`.
    pub fn seed_bundled_gbrain(
        &mut self,
        bun_path: &std::path::Path,
        entry_path: &std::path::Path,
        gbrain_home: &std::path::Path,
    ) -> Result<bool, String> {
        if let Some(state) = self.servers.get_mut("gbrain") {
            if is_legacy_bundled_gbrain_script_wrapper(&state.config) {
                let enabled = state.config.enabled;
                let auto_approve = state.config.auto_approve;
                let mut config = bundled_gbrain_config(bun_path, entry_path, gbrain_home);
                config.enabled = enabled;
                config.auto_approve = auto_approve;
                state.config = config;
                self.save_config();
                tracing::warn!(
                    bun = %bun_path.display(),
                    entry = %entry_path.display(),
                    gbrain_home = %gbrain_home.display(),
                    "gbrain Sprint 2.1: migrated legacy /usr/bin/script MCP wrapper to direct Bun stdio"
                );
                return Ok(true);
            }
            if is_bundled_gbrain(&state.config) {
                let enabled = state.config.enabled;
                let auto_approve = state.config.auto_approve;
                let mut desired_config = bundled_gbrain_config(bun_path, entry_path, gbrain_home);
                desired_config.enabled = enabled;
                desired_config.auto_approve = auto_approve;

                if state.config.command != desired_config.command
                    || state.config.args != desired_config.args
                    || state.config.env != desired_config.env
                    || state.config.tool_allowlist != desired_config.tool_allowlist
                {
                    state.config = desired_config;
                    self.save_config();
                    tracing::info!(
                        bun = %bun_path.display(),
                        entry = %entry_path.display(),
                        gbrain_home = %gbrain_home.display(),
                        "seed_bundled_gbrain: refreshed bundled gbrain command/env/tool allowlist"
                    );
                    return Ok(true);
                }
            }
            tracing::debug!(
                "seed_bundled_gbrain: 'gbrain' entry already in config (keeping user state)"
            );
            return Ok(false);
        }
        let config = bundled_gbrain_config(bun_path, entry_path, gbrain_home);
        self.add_server(config)?;
        tracing::info!(
            bun = %bun_path.display(),
            entry = %entry_path.display(),
            gbrain_home = %gbrain_home.display(),
            "gbrain Sprint 2.1: seeded bundled MCP entry"
        );
        Ok(true)
    }

    pub fn seed_builtin_playwright_mcp(&mut self) -> Result<bool, String> {
        let config = builtin_playwright_mcp_config();
        if let Some(state) = self.servers.get_mut("playwright") {
            let enabled = state.config.enabled;
            let auto_approve = state.config.auto_approve;
            let was_current = state.config.command == config.command
                && state.config.args == config.args
                && state.config.env == config.env
                && state.config.tool_allowlist == config.tool_allowlist
                && state.config.transport_type == config.transport_type
                && state.config.url == config.url
                && state.config.name == config.name
                && state.config.description == config.description;
            let mut refreshed = config;
            refreshed.enabled = enabled;
            refreshed.auto_approve = auto_approve;
            state.config = refreshed;
            self.save_config();
            return Ok(!was_current);
        }

        self.add_server(config)?;
        tracing::info!("Seeded built-in Playwright MCP server");
        Ok(true)
    }

    pub fn set_playwright_mcp_raw_tools_exposed(
        &mut self,
        exposed: bool,
    ) -> Result<bool, String> {
        if !self.servers.contains_key("playwright") {
            self.seed_builtin_playwright_mcp()?;
        }
        let Some(state) = self.servers.get_mut("playwright") else {
            return Err("Playwright MCP server is not configured".to_string());
        };
        let desired = if exposed {
            Some(playwright_mcp_tool_allowlist())
        } else {
            Some(Vec::new())
        };
        if state.config.tool_allowlist == desired {
            return Ok(false);
        }
        state.config.tool_allowlist = desired;
        self.save_config();
        Ok(true)
    }

    pub fn remove_server(&mut self, id: &str) -> Option<McpServerConfig> {
        // PR-3 — abort any health loop for this server first so a
        // delayed reconnect attempt can't recreate the connection
        // after removal.
        self.stop_health_loop(id);
        // PR-5 — audit BEFORE the state is dropped so we have access
        // to the env for redaction (record_audit looks up the server).
        self.record_audit(id, McpAuditKind::Removed, "Server removed by user");
        let state = self.servers.remove(id)?;
        self.save_config();
        Some(state.config)
    }

    pub fn update_server(&mut self, id: &str, config: McpServerConfig) -> Result<(), String> {
        if !self.servers.contains_key(id) {
            return Err(format!("Server {} not found", id));
        }
        if let Some(state) = self.servers.get_mut(id) {
            state.config = config;
        }
        self.save_config();
        Ok(())
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if let Some(state) = self.servers.get_mut(id) {
            state.config.enabled = enabled;
            self.save_config();
            return true;
        }
        false
    }

    pub fn set_auto_approve(&mut self, id: &str, auto_approve: bool) -> bool {
        if let Some(state) = self.servers.get_mut(id) {
            state.config.auto_approve = auto_approve;
            self.save_config();
            return true;
        }
        false
    }

    // ── Status & Queries ────────────────────────────────────────────

    pub fn set_status(&mut self, id: &str, status: McpServerStatus) {
        if let Some(state) = self.servers.get_mut(id) {
            state.status = status;
        }
    }

    pub fn set_tools(&mut self, id: &str, tools: Vec<McpToolDef>) {
        if let Some(state) = self.servers.get_mut(id) {
            state.tools = tools;
        }
    }

    /// Set or clear the error message for a server. PR-5: redact env
    /// values from the message before storing so a screenshot of the
    /// detail drawer can't leak `GITHUB_TOKEN=ghp_xxxx`. Clearing
    /// (passing `None`) is unchanged.
    pub fn set_error(&mut self, id: &str, error: Option<String>) {
        if let Some(state) = self.servers.get_mut(id) {
            let redacted = error.map(|e| diagnostic_error_summary(&e, &state.config.env));
            let is_err = redacted.is_some();
            state.error = redacted;
            if is_err {
                state.status = McpServerStatus::Error;
            }
        }
    }

    fn set_error_for_state(state: &mut McpServerState, error: Option<String>) {
        let redacted = error.map(|e| diagnostic_error_summary(&e, &state.config.env));
        let is_err = redacted.is_some();
        state.error = redacted;
        if is_err {
            state.status = McpServerStatus::Error;
        }
    }

    pub fn enabled_servers(&self) -> Vec<&McpServerConfig> {
        self.servers
            .values()
            .filter(|s| s.config.enabled)
            .map(|s| &s.config)
            .collect()
    }

    pub fn all_servers(&self) -> Vec<&McpServerConfig> {
        self.servers.values().map(|s| &s.config).collect()
    }

    pub fn all_tools(&self) -> Vec<McpToolDef> {
        self.servers
            .values()
            .filter(|s| s.status == McpServerStatus::Connected)
            .flat_map(|s| s.tools.clone())
            .collect()
    }

    pub fn status(&self, id: &str) -> Option<McpServerStatus> {
        self.servers.get(id).map(|s| s.status.clone())
    }

    pub fn server_error(&self, id: &str) -> Option<String> {
        self.servers.get(id).and_then(|s| s.error.clone())
    }

    /// Return the number of discovered tools for a server, or None if the
    /// server ID is not registered.
    pub fn server_tool_count(&self, id: &str) -> Option<usize> {
        self.servers.get(id).map(|s| s.tools.len())
    }

    pub fn server_config(&self, id: &str) -> Option<McpServerConfig> {
        self.servers.get(id).map(|s| s.config.clone())
    }

    #[cfg(test)]
    pub fn test_set_server_tools(
        &mut self,
        id: &str,
        status: McpServerStatus,
        tools: Vec<McpToolDef>,
    ) {
        if let Some(state) = self.servers.get_mut(id) {
            state.status = status;
            state.tools = tools;
        }
    }

    /// Get detailed status for all servers (for IPC)
    pub fn all_server_statuses(&self) -> Vec<(String, McpServerStatus, Option<String>)> {
        self.servers
            .values()
            .map(|s| (s.config.id.clone(), s.status.clone(), s.error.clone()))
            .collect()
    }

    // ── Connection Lifecycle ────────────────────────────────────────

    /// Disconnect from an MCP server. Also aborts the health loop
    /// (PR-3) so a pending reconnect can't fight a user-initiated
    /// disconnect. Caller is expected to call `start_health_loop`
    /// again after the next successful connect.
    pub async fn disconnect_server(&mut self, id: &str) -> Result<(), McpError> {
        self.stop_health_loop(id);
        if let Some(state) = self.servers.get_mut(id) {
            if let Some(conn) = state.connection.take() {
                conn.shutdown().await?;
            }
            state.status = McpServerStatus::Disconnected;
            state.tools.clear();
            state.error = None;
            tracing::info!("Disconnected from MCP server '{}'", state.config.name);
        }
        // PR-5 — outside the `if let` so we still audit the call even
        // when the server isn't in the map (e.g. removed mid-flight).
        self.record_audit(id, McpAuditKind::Disconnect, "Disconnected");
        Ok(())
    }

    // ── PR-3 — health loop management ───────────────────────────────

    /// Spawn (or replace) the per-server health/reconnect background
    /// task. Idempotent: if a loop is already running for this id it's
    /// aborted before the new one starts. Caller passes the shared
    /// manager arc so the spawned task can re-acquire the lock for
    /// ping + reconnect without holding a borrow across the spawn.
    pub fn start_health_loop(&mut self, mgr: SharedMcpManager, id: &str) {
        if let Some(h) = self.health_tasks.remove(id) {
            h.abort();
        }
        let id_owned = id.to_string();
        let handle = tokio::spawn(async move {
            Self::run_health_loop(mgr, id_owned).await;
        });
        self.health_tasks.insert(id.to_string(), handle);
    }

    /// Abort the loop for `id` if any. Caller invokes this from
    /// `disconnect_server` / `remove_server`.
    pub fn stop_health_loop(&mut self, id: &str) {
        if let Some(h) = self.health_tasks.remove(id) {
            h.abort();
            tracing::debug!("[{}] health loop aborted", id);
        }
    }

    /// The actual loop body. Lives outside the impl block conceptually
    /// (it doesn't take &self) so the spawn closure isn't required to
    /// hold a borrow back into the manager.
    ///
    /// Two phases per iteration:
    /// 1. Ping immediately. On success, reset the backoff and wait for
    ///    the next regular health interval.
    /// 2. On failure, flip the server's status to Error with a
    ///    descriptive message, sleep `min(INITIAL * 2^attempt, MAX)`,
    ///    then call `reconnect_server`. Success resets attempt; failure
    ///    bumps attempt and loops back to phase 2's sleep.
    ///
    /// The loop is cancellation-aware: `tokio::spawn`-ed tasks die when
    /// the JoinHandle is aborted, which is what `stop_health_loop`
    /// does. No explicit shutdown signal needed.
    async fn run_health_loop(mgr: SharedMcpManager, id: String) {
        let mut attempt: u32 = 0;
        loop {
            let current_status = {
                let m = mgr.read().await;
                m.status(&id)
            };
            if matches!(current_status, Some(McpServerStatus::Connecting)) {
                tokio::time::sleep(Duration::from_secs(RECONNECT_INITIAL_DELAY_SECS)).await;
                continue;
            }

            // Ping under a read lock — short critical section.
            let ping_result = {
                let m = mgr.read().await;
                m.ping_server(&id).await
            };

            match ping_result {
                Ok(()) => {
                    // Healthy: reset attempt counter and continue the
                    // outer loop. If the server was in Error from a
                    // previous failure-then-recovery cycle, the
                    // reconnect path below already cleared it.
                    attempt = 0;
                    tokio::time::sleep(Duration::from_secs(HEALTH_PING_INTERVAL_SECS)).await;
                    continue;
                }
                Err(e) => {
                    tracing::warn!(
                        "[{}] health ping failed: {} (attempt {})",
                        id,
                        e,
                        attempt + 1
                    );
                    // Compute backoff *before* messaging so the UI
                    // shows "next attempt in 80s" rather than the
                    // misleading current attempt's delay.
                    let delay = std::cmp::min(
                        RECONNECT_INITIAL_DELAY_SECS
                            .saturating_mul(2u64.saturating_pow(attempt)),
                        RECONNECT_MAX_DELAY_SECS,
                    );
                    {
                        let msg = format!(
                            "Health check failed: {} — reconnecting in {}s (attempt {})",
                            e,
                            delay,
                            attempt + 1
                        );
                        let mut m = mgr.write().await;
                        m.set_error(&id, Some(msg.clone()));
                        // PR-5 — also persist to the audit table so the
                        // user can review history across restarts.
                        m.record_audit(&id, McpAuditKind::HealthFailed, &msg);
                    }
                    tokio::time::sleep(Duration::from_secs(delay)).await;

                    let reconnect_result = reconnect_server_shared(&mgr, &id).await;
                    match reconnect_result {
                        Ok(()) => {
                            tracing::info!(
                                "[{}] reconnect succeeded after {} attempt(s)",
                                id,
                                attempt + 1
                            );
                            attempt = 0;
                            tokio::time::sleep(Duration::from_secs(
                                HEALTH_PING_INTERVAL_SECS,
                            ))
                            .await;
                        }
                        Err(rc_err) => {
                            tracing::warn!(
                                "[{}] reconnect attempt {} failed: {}",
                                id,
                                attempt + 1,
                                rc_err
                            );
                            attempt = attempt.saturating_add(1);
                        }
                    }
                }
            }
        }
    }

    /// Snapshot the IDs of enabled servers. Cheap; takes only a `&self`
    /// borrow so callers can release the lock before doing async work.
    pub fn list_enabled_ids(&self) -> Vec<String> {
        self.servers
            .values()
            .filter(|s| s.config.enabled)
            .map(|s| s.config.id.clone())
            .collect()
    }

    /// Disconnect all servers. Also aborts every health loop (PR-3) —
    /// `disconnect_server` does it per id but iterating that way is
    /// `O(n)` ops on the health_tasks map; doing it in one drain is
    /// cleaner and matches the "we're shutting down" intent.
    pub async fn disconnect_all(&mut self) {
        for (_id, h) in self.health_tasks.drain() {
            h.abort();
        }
        let ids: Vec<String> = self.servers.keys().cloned().collect();
        for id in ids {
            self.disconnect_server(&id).await.ok();
        }
    }

    /// Health check (ping) a connected server
    pub async fn ping_server(&self, id: &str) -> Result<(), McpError> {
        let state = self.servers.get(id).ok_or_else(|| {
            McpError::Server(format!("Server {} not found", id))
        })?;
        let conn = state.connection.as_ref().ok_or_else(|| {
            McpError::Server(format!("Server {} is not connected", id))
        })?;
        conn.ping().await
    }

    /// Refresh tools for a connected server
    pub async fn refresh_tools(&mut self, id: &str) -> Result<Vec<McpToolDef>, McpError> {
        let remote_tools = {
            let state = self.servers.get_mut(id).ok_or_else(|| {
                McpError::Server(format!("Server {} not found", id))
            })?;
            let conn = state.connection.as_mut().ok_or_else(|| {
                McpError::Server(format!("Server {} is not connected", id))
            })?;
            conn.discover_tools().await?
        };

        let tool_defs: Vec<McpToolDef> = remote_tools
            .iter()
            .map(|t| McpToolDef {
                server_id: id.to_string(),
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.input_schema.clone(),
            })
            .collect();

        if let Some(state) = self.servers.get_mut(id) {
            state.tools = tool_defs.clone();
        }

        Ok(tool_defs)
    }

    // ── Tool Proxying ───────────────────────────────────────────────

    /// Get a cloneable transport handle and a next-id generator for a connected server.
    /// Used by McpToolProxy to call tools without holding the manager lock.
    pub(crate) fn get_transport(
        &self,
        server_id: &str,
    ) -> Result<(Arc<dyn McpTransport>, u64), McpError> {
        let state = self.servers.get(server_id).ok_or_else(|| {
            McpError::Server(format!("Server {} not found", server_id))
        })?;
        let conn = state.connection.as_ref().ok_or_else(|| {
            McpError::Server(format!("Server {} is not connected", server_id))
        })?;
        let id = conn.next_id();
        Ok((conn.transport.clone(), id))
    }

    /// Call a tool on a connected MCP server
    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        tracing::debug!("Calling MCP tool '{}' on server '{}'", tool_name, server_id);
        let state = self.servers.get(server_id).ok_or_else(|| {
            McpError::Server(format!("Server {} not found", server_id))
        })?;
        let conn = state.connection.as_ref().ok_or_else(|| {
            McpError::Server(format!("Server {} is not connected", server_id))
        })?;

        conn.call_tool(tool_name, arguments).await
    }

    /// List resources from a connected MCP server
    pub async fn list_resources(&self, server_id: &str) -> Result<serde_json::Value, McpError> {
        let state = self.servers.get(server_id).ok_or_else(|| {
            McpError::Server(format!("Server {} not found", server_id))
        })?;
        let conn = state.connection.as_ref().ok_or_else(|| {
            McpError::Server(format!("Server {} is not connected", server_id))
        })?;
        conn.list_resources().await
    }

    /// Read a resource from a connected MCP server
    pub async fn read_resource(
        &self,
        server_id: &str,
        uri: &str,
    ) -> Result<serde_json::Value, McpError> {
        let state = self.servers.get(server_id).ok_or_else(|| {
            McpError::Server(format!("Server {} not found", server_id))
        })?;
        let conn = state.connection.as_ref().ok_or_else(|| {
            McpError::Server(format!("Server {} is not connected", server_id))
        })?;
        conn.read_resource(uri).await
    }

    /// Create McpToolProxy instances for every tool exposed by the
    /// currently-connected servers. These wrap the MCP transport in the
    /// agent's `Tool` trait so the dispatcher can call MCP tools the
    /// same way it calls builtins.
    ///
    /// `locked` is the already-acquired read guard on the manager — by
    /// taking it explicitly we (a) avoid re-locking inside this method
    /// and (b) make the snapshot semantics obvious at the call site.
    /// `manager` is the shared handle each proxy keeps so it can run
    /// the actual `tools/call` JSON-RPC without re-borrowing.
    ///
    /// Names are emitted in the prefixed form
    /// `mcp__{server_id}__{tool_name}` so they're unambiguous in the
    /// LLM's tool manifest and in SafetyManager logs. `auto_approve` is
    /// snapshotted per-proxy so `Tool::requires_approval` short-circuits
    /// approval prompts for trusted servers.
    pub fn create_tool_proxies(
        manager: &SharedMcpManager,
        locked: &McpManager,
    ) -> Vec<McpToolProxy> {
        let server_meta: HashMap<String, (bool, Option<Vec<String>>)> = locked
            .all_servers()
            .iter()
            .map(|c| (c.id.clone(), (c.auto_approve, c.tool_allowlist.clone())))
            .collect();
        locked
            .all_tools()
            .into_iter()
            .filter(|tool| {
                // Apply per-server tool_allowlist: when set, only whitelisted
                // tool names are registered into the agent ToolRegistry.
                // Some([]) deliberately hides every raw tool while adapter
                // code can still call through McpManager.
                match server_meta.get(&tool.server_id) {
                    Some((_, Some(allowlist))) => allowlist.contains(&tool.name),
                    _ => true,
                }
            })
            .map(|tool| {
                let auto_approve = server_meta
                    .get(&tool.server_id)
                    .map(|(a, _)| *a)
                    .unwrap_or(false);
                let prefixed_name = prefixed_tool_name(&tool.server_id, &tool.name);
                McpToolProxy {
                    server_id: tool.server_id.clone(),
                    tool_name: tool.name.clone(),
                    prefixed_name,
                    description: tool.description.clone(),
                    input_schema: tool.parameters.clone(),
                    manager: manager.clone(),
                    auto_approve,
                }
            })
            .collect()
    }

}

/// Connect to an MCP server without holding the manager's write lock
/// across the slow network IO. Three phases:
///   1. Prepare (short write lock): clone config, snapshot
///      `notification_tx`, mark `Connecting`, audit `ConnectAttempt`.
///   2. IO (no lock held): spawn transport, run `initialize` and
///      `discover_tools`.
///   3. Commit (short write lock): install the connection + tool defs +
///      `Connected` status, OR mark `Error` and audit appropriately.
///
/// This is the lock-discipline fix for the "first message after app
/// launch hangs for 60s" bug — the bundled gbrain server's initialize
/// can take up to a full 60s to time out, and prior to this refactor
/// the manager-wide write lock was held the entire time, blocking
/// `send_agent_message`'s `mcp_manager.read().await`.
///
/// Note on `discover_tools` mutation semantics: `McpConnection::discover_tools`
/// both populates `conn.tools` in-place (via `self.tools = result.tools.clone()`)
/// AND returns `Result<Vec<McpRemoteTool>>`. The commit phase reads `conn.tools`
/// directly since the in-place mutation is already done before we re-acquire the lock.
pub(crate) async fn connect_server_shared(
    shared: &SharedMcpManager,
    id: &str,
) -> Result<(), McpError> {
    // ── Phase 1: prepare ────────────────────────────────────────────
    let (config, notification_tx, runtime_working_dir) = {
        let mut guard = shared.write().await;
        let state = guard.servers.get(id).ok_or_else(|| {
            McpError::Server(format!("Server {} not found", id))
        })?;
        if matches!(state.status, McpServerStatus::Connecting) {
            return Err(McpError::Server(format!(
                "Server {} is already connecting",
                id
            )));
        }
        let config = state.config.clone();
        let notification_tx = guard.notification_tx.clone();
        let runtime_working_dir = guard.runtime_working_dir(id);
        if let Some(state) = guard.servers.get_mut(id) {
            state.status = McpServerStatus::Connecting;
            state.error = None;
        }
        guard.record_audit(
            id,
            McpAuditKind::ConnectAttempt,
            &format!("Connecting to {}", config.name),
        );
        (config, notification_tx, runtime_working_dir)
    };

    tracing::info!(
        working_dir = runtime_working_dir.as_ref().map(|path| path.display().to_string()),
        "Connecting to MCP server '{}' ({})",
        config.name,
        id
    );

    // ── Phase 2: IO (no lock held) ──────────────────────────────────
    let io_result: Result<McpConnection, McpError> = async {
        let transport: Arc<dyn McpTransport> = match config.transport_type {
            TransportType::Stdio => {
                if is_bundled_gbrain(&config) {
                    tracing::warn!(
                        server_id = %id,
                        "Using bundled gbrain CLI-backed MCP transport instead of Bun stdio"
                    );
                    Arc::new(GbrainCliTransport::new(
                        &config.name,
                        &config.command,
                        &config.args,
                        &config.env,
                    ))
                } else {
                    let t = StdioTransport::spawn(
                        &config.name,
                        &config.command,
                        &config.args,
                        &config.env,
                        runtime_working_dir.as_deref(),
                        id,
                        notification_tx.clone(),
                    )
                    .await?;
                    Arc::new(t)
                }
            }
            TransportType::Http => {
                let url = config.url.clone().unwrap_or_default();
                if url.is_empty() {
                    return Err(McpError::Server(
                        "HTTP transport requires a URL".into(),
                    ));
                }
                Arc::new(HttpTransport::new(&config.name, &url))
            }
        };

        let mut conn = McpConnection {
            transport,
            next_id: AtomicU64::new(1),
            initialized: false,
            tools: Vec::new(),
            server_info: None,
        };

        // initialize is the expensive call (up to ~60s on a hung
        // stdio server). Critically, no lock is held here.
        let init_result = conn.initialize().await?;
        tracing::info!(
            "MCP server '{}' initialized (protocol: {:?}, server: {:?})",
            config.name,
            init_result.protocol_version,
            init_result.server_info.as_ref().map(|s| &s.name),
        );

        // discover_tools failure is non-fatal — the server may simply
        // not implement tools/list. We still keep the connection.
        // discover_tools also populates conn.tools in-place, so the
        // commit phase reads conn.tools directly.
        if let Err(e) = conn.discover_tools().await {
            tracing::warn!(
                "MCP server '{}' tools/list failed: {}",
                config.name,
                e
            );
        }

        Ok(conn)
    }
    .await;

    // ── Phase 3: commit ─────────────────────────────────────────────
    let mut guard = shared.write().await;
    match io_result {
        Ok(conn) => {
            // discover_tools populates conn.tools in-place (conn.tools =
            // result.tools.clone() inside McpConnection::discover_tools).
            // Mirror those into ServerState.tools as McpToolDef entries.
            let tool_defs: Vec<McpToolDef> = conn
                .tools
                .iter()
                .map(|t| McpToolDef {
                    server_id: id.to_string(),
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.input_schema.clone(),
                })
                .collect();
            let tool_count = tool_defs.len();
            tracing::info!(
                "MCP server '{}' has {} tool(s): [{}]",
                config.name,
                tool_count,
                tool_defs.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")
            );
            if let Some(state) = guard.servers.get_mut(id) {
                state.status = McpServerStatus::Connected;
                state.error = None;
                state.tools = tool_defs;
                state.connection = Some(conn);
            }
            guard.record_audit(
                id,
                McpAuditKind::ConnectSucceeded,
                &format!("Connected ({} tool(s) discovered)", tool_count),
            );
            Ok(())
        }
        Err(e) => {
            tracing::error!(
                "MCP server '{}' connect failed: {}",
                config.name,
                e
            );
            guard.record_audit(id, McpAuditKind::ConnectFailed, &e.to_string());
            if let Some(state) = guard.servers.get_mut(id) {
                state.status = McpServerStatus::Error;
                state.error = Some(e.to_string());
            }
            Err(e)
        }
    }
}

/// Internal reconnect for the health loop. Mirrors `restart_server`'s
/// disconnect+connect shape but without aborting the health loop
/// (we *are* the health loop).
///
/// Lock note: the `conn.shutdown().await` call is made while holding the
/// write lock. For Stdio transports `shutdown()` only closes stdin and drops
/// the child handle — it does not wait for the child to exit — so this is a
/// very short non-IO critical section in practice. HTTP/SSE transports drop
/// the reqwest connection handle. Neither involves 60s network IO, so
/// holding the write lock across `shutdown()` is acceptable here.
pub(crate) async fn reconnect_server_shared(
    shared: &SharedMcpManager,
    id: &str,
) -> Result<(), McpError> {
    {
        let mut guard = shared.write().await;
        if let Some(state) = guard.servers.get_mut(id) {
            if let Some(conn) = state.connection.take() {
                let _ = conn.shutdown().await;
            }
            state.status = McpServerStatus::Disconnected;
        }
    }
    connect_server_shared(shared, id).await
}

/// Restart a server connection. User-triggered (Tauri command) and
/// distinct from the health loop's internal `reconnect_server_shared`
/// in that it ALSO aborts the health loop so the loop's pending
/// reconnect can't fight the user.
pub async fn restart_server_shared(
    shared: &SharedMcpManager,
    id: &str,
) -> Result<(), McpError> {
    {
        let mut guard = shared.write().await;
        guard.stop_health_loop(id);
        if let Some(state) = guard.servers.get_mut(id) {
            if let Some(conn) = state.connection.take() {
                let _ = conn.shutdown().await;
            }
            state.status = McpServerStatus::Disconnected;
            state.tools.clear();
            state.error = None;
        }
        guard.record_audit(id, McpAuditKind::Disconnect, "Disconnected (restart)");
    }
    connect_server_shared(shared, id).await?;
    {
        let mut guard = shared.write().await;
        guard.start_health_loop(shared.clone(), id);
    }
    Ok(())
}

/// Connect all enabled servers. Each server's connect runs through
/// `connect_server_shared`, which releases the write lock during the
/// slow initialize/discover IO. Sequential iteration is fine because
/// the slow part no longer blocks readers — parallelizing would only
/// help startup wall time, not user-perceived latency.
pub async fn connect_all_enabled(shared: &SharedMcpManager) {
    let ids: Vec<String> = {
        let guard = shared.read().await;
        guard.list_enabled_ids()
    };
    for id in ids {
        if let Err(e) = connect_server_shared(shared, &id).await {
            tracing::error!("Failed to connect MCP server '{}': {}", id, e);
        }
    }
}

/// gbrain Sprint 2.1 init-fix — probe whether `<gbrain_home>/.gbrain/brain.pglite/`
/// has been initialized by `gbrain init --pglite`. The presence of
/// `PG_VERSION` is the canonical Postgres-data-dir initialization marker
/// (PGLite writes it as part of `initdb`).
///
/// Pure — no I/O beyond `Path::exists`. Used by
/// `ensure_bundled_gbrain_initialized` to decide whether to spawn `gbrain
/// init` or skip (idempotent). Safe to call repeatedly.
pub fn is_brain_initialized(gbrain_home: &std::path::Path) -> bool {
    gbrain_home
        .join(".gbrain")
        .join("brain.pglite")
        .join("PG_VERSION")
        .exists()
}

/// gbrain Sprint 2.1 init-fix — run `bun <cli.ts> init --pglite --yes` against
/// `gbrain_home` if the brain isn't already initialized. First call
/// cold-starts PGLite + runs ~63 migrations (~30-60s on Apple Silicon);
/// subsequent calls short-circuit via `is_brain_initialized` and return
/// `Ok(false)` in O(1).
///
/// Sprint 2.2.5a: wrapped in `tokio::time::timeout(GBRAIN_INIT_TIMEOUT_SECS)`
/// so a corrupted bun binary or stuck PGLite migration can't hang the
/// entire app boot. On timeout the child is dropped (tokio kills it on
/// drop because `kill_on_drop(true)` is set below); caller sees the same
/// `Err(...)` shape as any other init failure and falls through to the
/// seed-anyway path.
///
/// Returns:
/// - `Ok(true)`  — freshly initialized
/// - `Ok(false)` — already initialized, no work done
/// - `Err(msg)`  — spawn failed, timed out, OR `gbrain init` exited
///   non-zero. Caller MUST NOT proceed to seed the MCP entry,
///   otherwise gbrain will spawn and immediately exit with "No brain
///   configured" on every connect.
///
/// `GBRAIN_HOME` is the only env var threaded through. `gbrain init`
/// writes `<gbrain_home>/.gbrain/config.json` itself with the correct
/// `database_path` — callers MUST NOT pre-write that file (the v0.35
/// init path uses its own layout, not whatever the caller passes).
pub async fn ensure_bundled_gbrain_initialized(
    bun_path: &std::path::Path,
    entry_path: &std::path::Path,
    gbrain_home: &std::path::Path,
) -> Result<bool, String> {
    if is_brain_initialized(gbrain_home) {
        tracing::debug!(
            gbrain_home = %gbrain_home.display(),
            "ensure_bundled_gbrain_initialized: brain already initialized"
        );
        return Ok(false);
    }
    if let Err(e) = std::fs::create_dir_all(gbrain_home) {
        return Err(format!(
            "create gbrain_home {}: {}",
            gbrain_home.display(),
            e
        ));
    }
    tracing::info!(
        bun = %bun_path.display(),
        entry = %entry_path.display(),
        gbrain_home = %gbrain_home.display(),
        timeout_secs = GBRAIN_INIT_TIMEOUT_SECS,
        "gbrain Sprint 2.1 init-fix: running 'gbrain init --pglite --yes' (first launch, may take 30-60s)"
    );
    // Sprint 2.2.5a — tokio::process::Command + timeout. kill_on_drop
    // ensures the bun child is reaped even if we drop the future
    // (timeout fires, task cancelled, etc).
    let mut cmd = tokio::process::Command::new(bun_path);
    cmd.arg(entry_path)
        .arg("init")
        .arg("--pglite")
        .arg("--yes")
        .env("GBRAIN_HOME", gbrain_home)
        .kill_on_drop(true);
    let timeout = Duration::from_secs(GBRAIN_INIT_TIMEOUT_SECS);
    let output = match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(format!("spawn 'bun gbrain init': {}", e)),
        Err(_elapsed) => {
            return Err(format!(
                "'gbrain init' timed out after {}s — bun binary may be \
                 corrupted or PGLite migration stuck. Re-run \
                 scripts/init-gbrain.sh manually or remove ~/.uclaw/gbrain/ \
                 to retry from scratch.",
                GBRAIN_INIT_TIMEOUT_SECS
            ));
        }
    };
    if !output.status.success() {
        let stderr_tail: String = String::from_utf8_lossy(&output.stderr)
            .lines()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!(
            "'gbrain init' exited {:?}\nstderr (last 20 lines):\n{}",
            output.status.code(),
            stderr_tail
        ));
    }
    // Defense in depth: verify the marker really landed. Catches the case
    // where gbrain init exits 0 but writes to an unexpected path (bug
    // surface we are explicitly fixing in this PR).
    if !is_brain_initialized(gbrain_home) {
        return Err(format!(
            "'gbrain init' exited 0 but {} did not appear — \
             gbrain may have written to a different GBRAIN_HOME",
            gbrain_home.join(".gbrain/brain.pglite/PG_VERSION").display()
        ));
    }
    tracing::info!(
        gbrain_home = %gbrain_home.display(),
        "gbrain Sprint 2.1 init-fix: brain initialized successfully"
    );
    Ok(true)
}

/// Shared MCP manager for Tauri state
pub type SharedMcpManager = Arc<RwLock<McpManager>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_deps_missing_from_module_not_found() {
        // The bundled gbrain CLI crashing on a missing node module → deps_missing,
        // with a hint pointing at the setup script.
        let kind = classify_gbrain_cli_failure(
            "Cannot find module '@electric-sql/pglite/vector' from '.../gbrain/src/core/pglite-engine.ts'",
            "exit status: 1",
        );
        assert_eq!(kind, "deps_missing");
        assert!(gbrain_cli_error_hint(&kind).contains("setup-gbrain-source.sh"));
    }

    fn cfg(id: &str, transport: TransportType) -> McpServerConfig {
        McpServerConfig {
            id: id.into(),
            name: format!("srv-{id}"),
            description: String::new(),
            transport_type: transport,
            command: "npx".into(),
            args: vec!["-y".into()],
            env: HashMap::new(),
            url: None,
            enabled: true,
            auto_approve: false,
            tool_allowlist: None,
        }
    }

    #[test]
    fn add_server_preserves_transport_type_and_url() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = McpManager::new(dir.path());
        let mut http = cfg("a", TransportType::Http);
        http.url = Some("https://example.com/mcp".into());
        mgr.add_server(http).unwrap();
        let stored = mgr.all_servers().into_iter().find(|c| c.id == "a").unwrap();
        assert_eq!(stored.transport_type, TransportType::Http);
        assert_eq!(stored.url.as_deref(), Some("https://example.com/mcp"));
    }

    #[test]
    fn diagnostic_error_summary_classifies_without_user_content() {
        let mut env = HashMap::new();
        env.insert("GBRAIN_HOME".to_string(), "/Users/alice/.uclaw/gbrain".to_string());
        let summary = diagnostic_error_summary(
            "[gbrain] gbrain CLI 'list_pages' timed out while searching private customer notes /Users/alice/.uclaw/gbrain",
            &env,
        );
        assert!(summary.contains("diagnostic_kind=mcp_connect_timeout"));
        assert!(summary.contains("timed out"));
        assert!(!summary.contains("private customer notes"));
        assert!(!summary.contains("/Users/alice"));
    }

    #[test]
    fn gbrain_cli_error_payload_is_structured_for_recovery_ui() {
        let payload = gbrain_cli_error_payload(
            "get_page",
            "page_not_found",
            "exit status: 1",
            vec!["knowledge/openai-gpt5".to_string()],
        );
        let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["source"], "gbrain");
        assert_eq!(value["tool"], "get_page");
        assert_eq!(value["kind"], "page_not_found");
        assert_eq!(value["nearest_slugs"][0], "knowledge/openai-gpt5");
        assert!(value["hint"].as_str().unwrap().contains("suggestions"));
    }

    #[test]
    fn update_server_rewrites_config_and_persists_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut mgr = McpManager::new(dir.path());
            mgr.add_server(cfg("b", TransportType::Stdio)).unwrap();
            let mut updated = cfg("b", TransportType::Http);
            updated.url = Some("https://example.com/b".into());
            updated.auto_approve = true;
            mgr.update_server("b", updated).unwrap();
        }
        // Re-open from disk — confirms save_config persisted the update.
        let mgr2 = McpManager::new(dir.path());
        let stored = mgr2.all_servers().into_iter().find(|c| c.id == "b").unwrap();
        assert_eq!(stored.transport_type, TransportType::Http);
        assert_eq!(stored.url.as_deref(), Some("https://example.com/b"));
        assert!(stored.auto_approve);
    }

    #[test]
    fn update_server_missing_id_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = McpManager::new(dir.path());
        let err = mgr
            .update_server("nope", cfg("nope", TransportType::Stdio))
            .unwrap_err();
        assert!(err.contains("not found"));
    }

    #[test]
    fn seed_bundled_gbrain_migrates_legacy_script_wrapper() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut mgr = McpManager::new(dir.path());
            let mut legacy = cfg("gbrain", TransportType::Stdio);
            legacy.name = "gbrain (bundled)".into();
            legacy.description =
                "Wrapped via macOS BSD `script` to defeat bun stdout buffering".into();
            legacy.command = "/usr/bin/script".into();
            legacy.args = vec![
                "-q".into(),
                "/dev/null".into(),
                "/tmp/uclaw/bun".into(),
                "/tmp/uclaw/gbrain/src/cli.ts".into(),
                "serve".into(),
            ];
            legacy.env.insert("GBRAIN_HOME".into(), "/old/home".into());
            legacy.enabled = false;
            legacy.auto_approve = false;
            mgr.add_server(legacy).unwrap();

            let changed = mgr
                .seed_bundled_gbrain(
                    std::path::Path::new("/new/bun"),
                    std::path::Path::new("/new/gbrain/src/cli.ts"),
                    std::path::Path::new("/new/home"),
                )
                .unwrap();
            assert!(changed);
        }

        let mgr = McpManager::new(dir.path());
        let stored = mgr
            .all_servers()
            .into_iter()
            .find(|config| config.id == "gbrain")
            .unwrap();
        assert_eq!(stored.command, "/new/bun");
        assert_eq!(
            stored.args,
            vec!["/new/gbrain/src/cli.ts".to_string(), "serve".to_string()]
        );
        assert_eq!(stored.env.get("GBRAIN_HOME").map(String::as_str), Some("/new/home"));
        assert!(!stored.enabled);
        assert!(!stored.auto_approve);
        assert_eq!(
            stored.tool_allowlist.as_deref(),
            Some(bundled_gbrain_tool_allowlist().as_slice())
        );
    }

    #[test]
    fn seed_bundled_gbrain_refreshes_stale_bundled_paths() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut mgr = McpManager::new(dir.path());
            let mut stale = bundled_gbrain_config(
                std::path::Path::new("/old/dev/target/debug/bun"),
                std::path::Path::new("/old/dev/target/debug/gbrain/src/cli.ts"),
                std::path::Path::new("/old/home"),
            );
            stale.enabled = false;
            stale.auto_approve = false;
            mgr.add_server(stale).unwrap();

            let changed = mgr
                .seed_bundled_gbrain(
                    std::path::Path::new("/Applications/uClaw.app/Contents/Resources/bun"),
                    std::path::Path::new(
                        "/Applications/uClaw.app/Contents/Resources/gbrain/src/cli.ts",
                    ),
                    std::path::Path::new("/Users/test/.uclaw/gbrain"),
                )
                .unwrap();
            assert!(changed);
        }

        let mgr = McpManager::new(dir.path());
        let stored = mgr
            .all_servers()
            .into_iter()
            .find(|config| config.id == "gbrain")
            .unwrap();
        assert_eq!(
            stored.command,
            "/Applications/uClaw.app/Contents/Resources/bun"
        );
        assert_eq!(
            stored.args,
            vec![
                "/Applications/uClaw.app/Contents/Resources/gbrain/src/cli.ts".to_string(),
                "serve".to_string(),
            ]
        );
        assert_eq!(
            stored.env.get("GBRAIN_HOME").map(String::as_str),
            Some("/Users/test/.uclaw/gbrain")
        );
        assert!(!stored.enabled);
        assert!(!stored.auto_approve);
    }

    #[test]
    fn slug_distance_ranks_one_character_slug_typo_close() {
        assert!(
            slug_distance("aknowledge/openai-gpt5", "knowledge/openai-gpt5")
                < slug_distance("aknowledge/openai-gpt5", "ai-models/gpt-5")
        );
    }

    #[test]
    fn seed_bundled_gbrain_preserves_non_legacy_existing_entry() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut mgr = McpManager::new(dir.path());
            let mut custom = cfg("gbrain", TransportType::Stdio);
            custom.command = "/custom/gbrain".into();
            custom.args = vec!["serve".into()];
            mgr.add_server(custom).unwrap();

            let changed = mgr
                .seed_bundled_gbrain(
                    std::path::Path::new("/new/bun"),
                    std::path::Path::new("/new/gbrain/src/cli.ts"),
                    std::path::Path::new("/new/home"),
                )
                .unwrap();
            assert!(!changed);
        }

        let mgr = McpManager::new(dir.path());
        let stored = mgr
            .all_servers()
            .into_iter()
            .find(|config| config.id == "gbrain")
            .unwrap();
        assert_eq!(stored.command, "/custom/gbrain");
        assert_eq!(stored.args, vec!["serve".to_string()]);
    }

    #[test]
    fn seed_builtin_playwright_mcp_adds_official_npx_server() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut mgr = McpManager::new(dir.path());

        let seeded = mgr.seed_builtin_playwright_mcp().expect("seed");
        assert!(seeded);

        let cfg = mgr.server_config("playwright").expect("config");
        assert_eq!(cfg.command, "npx");
        assert_eq!(cfg.args, vec!["@playwright/mcp@latest".to_string()]);
        assert_eq!(cfg.transport_type, TransportType::Stdio);
        assert!(cfg.enabled);
        assert!(!cfg.auto_approve);
        assert_eq!(cfg.tool_allowlist.as_deref(), Some(&[] as &[String]));
    }

    #[test]
    fn seed_builtin_playwright_mcp_refreshes_managed_entry() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut mgr = McpManager::new(dir.path());
        mgr.seed_builtin_playwright_mcp().expect("seed");

        let refreshed = mgr.seed_builtin_playwright_mcp().expect("refresh");
        assert!(!refreshed);

        let cfg = mgr.server_config("playwright").expect("config");
        assert_eq!(cfg.command, "npx");
        assert_eq!(cfg.args, vec!["@playwright/mcp@latest".to_string()]);
        assert_eq!(cfg.tool_allowlist.as_deref(), Some(&[] as &[String]));
    }

    #[test]
    fn seed_builtin_playwright_mcp_preserves_user_enabled_and_approval_state() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut mgr = McpManager::new(dir.path());
        let mut stale = cfg("playwright", TransportType::Stdio);
        stale.command = "npx".to_string();
        stale.args = vec!["@playwright/mcp@old".to_string()];
        stale.enabled = false;
        stale.auto_approve = true;
        mgr.add_server(stale).expect("add stale");

        let refreshed = mgr.seed_builtin_playwright_mcp().expect("refresh");
        assert!(refreshed);

        let cfg = mgr.server_config("playwright").expect("config");
        assert_eq!(cfg.args, vec!["@playwright/mcp@latest".to_string()]);
        assert!(!cfg.enabled);
        assert!(cfg.auto_approve);
    }

    #[test]
    fn playwright_mcp_raw_tool_exposure_toggles_allowlisted_tools() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut mgr = McpManager::new(dir.path());

        assert!(mgr
            .set_playwright_mcp_raw_tools_exposed(true)
            .expect("expose"));
        let cfg = mgr.server_config("playwright").expect("config");
        assert_eq!(cfg.tool_allowlist, Some(playwright_mcp_tool_allowlist()));

        assert!(mgr
            .set_playwright_mcp_raw_tools_exposed(false)
            .expect("hide"));
        let cfg = mgr.server_config("playwright").expect("config");
        assert_eq!(cfg.tool_allowlist.as_deref(), Some(&[] as &[String]));
    }

    #[test]
    fn runtime_working_dir_is_process_local_and_not_persisted() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut mgr = McpManager::new(dir.path());
        mgr.seed_builtin_playwright_mcp().expect("seed");

        let workspace = std::path::PathBuf::from("/tmp/uclaw-active-workspace");
        mgr.set_runtime_working_dir("playwright", Some(workspace.clone()));

        assert_eq!(mgr.runtime_working_dir("playwright"), Some(workspace));
        let cfg = mgr.server_config("playwright").expect("config");
        assert_eq!(cfg.command, "npx");
        assert_eq!(cfg.args, vec!["@playwright/mcp@latest".to_string()]);
    }

    // ─── PR-1 — prefix helpers + auto_approve plumbing ──────────────

    #[test]
    fn prefixed_tool_name_format_matches_convention() {
        // The mcp__{server}__{tool} shape is the Cline / Roo / Claude
        // Desktop convention; consumers (SafetyManager, UI badges,
        // telemetry) rely on it to recognize MCP-sourced calls without
        // a separate registry lookup.
        let n = prefixed_tool_name("github", "create_issue");
        assert_eq!(n, "mcp__github__create_issue");
    }

    #[test]
    fn parse_mcp_tool_name_round_trips() {
        let name = prefixed_tool_name("github", "create_issue");
        let parsed = parse_mcp_tool_name(&name).unwrap();
        assert_eq!(parsed.0, "github");
        assert_eq!(parsed.1, "create_issue");
    }

    #[test]
    fn parse_mcp_tool_name_handles_underscore_in_server_id() {
        // Server ids commonly contain single underscores ("my_team_search").
        // The split must be on the FIRST "__" (double-underscore)
        // boundary, not any single underscore, or those ids round-trip
        // wrong.
        let name = "mcp__my_team_search__do_thing";
        let parsed = parse_mcp_tool_name(name).unwrap();
        assert_eq!(parsed.0, "my_team_search");
        assert_eq!(parsed.1, "do_thing");
    }

    #[test]
    fn parse_mcp_tool_name_rejects_non_mcp_names() {
        // Builtins (read_file, edit, plan_update, …) must fast-path
        // through the parser as `None` so SafetyManager doesn't waste
        // cycles searching for a non-existent MCP server.
        assert!(parse_mcp_tool_name("read_file").is_none());
        assert!(parse_mcp_tool_name("mcp__").is_none()); // no server / tool
        assert!(parse_mcp_tool_name("mcp__github__").is_none()); // empty tool
        assert!(parse_mcp_tool_name("mcp____tool").is_none()); // empty server
    }

    #[test]
    fn create_tool_proxies_emits_prefixed_names_and_honors_auto_approve() {
        // Build a manager with two configured servers, one auto-approved
        // and one not, both with a single discovered tool. Verify the
        // returned proxies carry the right prefix and the auto_approve
        // flag is snapshotted onto each.
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = McpManager::new(dir.path());

        let mut trusted = cfg("trusted", TransportType::Stdio);
        trusted.auto_approve = true;
        mgr.add_server(trusted).unwrap();

        let untrusted = cfg("untrusted", TransportType::Stdio);
        mgr.add_server(untrusted).unwrap();

        // Simulate the post-connect state: tools discovered, status =
        // Connected. We bypass the actual transport here — all_tools()
        // filters on `status == Connected` so we have to set it.
        for id in ["trusted", "untrusted"] {
            if let Some(state) = mgr.servers.get_mut(id) {
                state.status = McpServerStatus::Connected;
                state.tools.push(McpToolDef {
                    server_id: id.to_string(),
                    name: "do_thing".to_string(),
                    description: format!("thing on {id}"),
                    parameters: serde_json::json!({}),
                });
            }
        }

        let shared: SharedMcpManager = Arc::new(RwLock::new(mgr));
        // Re-acquire a borrow for create_tool_proxies — we can't both
        // pass shared+locked in one statement so split the borrow.
        let proxies = {
            let locked = shared.try_read().unwrap();
            McpManager::create_tool_proxies(&shared, &*locked)
        };

        let names: Vec<&str> = proxies.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"mcp__trusted__do_thing"));
        assert!(names.contains(&"mcp__untrusted__do_thing"));

        let trusted_proxy = proxies.iter().find(|p| p.server_id == "trusted").unwrap();
        let untrusted_proxy = proxies.iter().find(|p| p.server_id == "untrusted").unwrap();
        assert!(trusted_proxy.auto_approve);
        assert!(!untrusted_proxy.auto_approve);

        use crate::agent::tools::tool::{ApprovalRequirement, Tool};
        let v = serde_json::json!({});
        assert_eq!(
            trusted_proxy.requires_approval(&v),
            ApprovalRequirement::Never
        );
        assert_eq!(
            untrusted_proxy.requires_approval(&v),
            ApprovalRequirement::UnlessAutoApproved
        );
    }

    #[test]
    fn create_tool_proxies_hides_server_when_allowlist_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = McpManager::new(dir.path());

        let mut playwright = cfg("playwright", TransportType::Stdio);
        playwright.tool_allowlist = Some(Vec::new());
        mgr.add_server(playwright).unwrap();

        if let Some(state) = mgr.servers.get_mut("playwright") {
            state.status = McpServerStatus::Connected;
            state.tools.push(McpToolDef {
                server_id: "playwright".to_string(),
                name: "browser_take_screenshot".to_string(),
                description: "screenshot".to_string(),
                parameters: serde_json::json!({}),
            });
        }

        let shared: SharedMcpManager = Arc::new(RwLock::new(mgr));
        let proxies = {
            let locked = shared.try_read().unwrap();
            McpManager::create_tool_proxies(&shared, &*locked)
        };

        assert!(proxies.is_empty());
    }

    #[test]
    fn server_tool_count_returns_none_for_missing_server() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = McpManager::new(tmp.path());
        assert_eq!(mgr.server_tool_count("gbrain"), None);
    }
}

#[cfg(test)]
mod gbrain_init_tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn is_brain_initialized_returns_false_for_empty_gbrain_home() {
        let dir = tempdir().unwrap();
        assert!(!is_brain_initialized(dir.path()));
    }

    #[test]
    fn is_brain_initialized_returns_false_when_brain_dir_missing_pg_version() {
        let dir = tempdir().unwrap();
        // .gbrain/brain.pglite/ exists but no PG_VERSION inside
        fs::create_dir_all(dir.path().join(".gbrain/brain.pglite")).unwrap();
        assert!(!is_brain_initialized(dir.path()));
    }

    #[test]
    fn is_brain_initialized_returns_true_when_pg_version_present() {
        let dir = tempdir().unwrap();
        let brain = dir.path().join(".gbrain/brain.pglite");
        fs::create_dir_all(&brain).unwrap();
        fs::write(brain.join("PG_VERSION"), "17\n").unwrap();
        assert!(is_brain_initialized(dir.path()));
    }

    #[tokio::test]
    async fn ensure_bundled_gbrain_short_circuits_when_already_initialized() {
        // Idempotency contract: when PG_VERSION already exists, the spawner
        // MUST return Ok(false) without invoking bun. We prove this by passing
        // bun/cli paths that don't exist on disk — if the function tried to
        // spawn, we'd get Err(spawn failed). Instead we get Ok(false).
        let dir = tempfile::tempdir().unwrap();
        let brain = dir.path().join(".gbrain").join("brain.pglite");
        std::fs::create_dir_all(&brain).unwrap();
        std::fs::write(brain.join("PG_VERSION"), "17\n").unwrap();

        let result = ensure_bundled_gbrain_initialized(
            std::path::Path::new("/nonexistent/bun"),
            std::path::Path::new("/nonexistent/cli.ts"),
            dir.path(),
        )
        .await;

        assert_eq!(result, Ok(false), "warm-path probe should short-circuit before spawn");
    }

    #[tokio::test]
    async fn ensure_bundled_gbrain_returns_err_when_bun_not_executable() {
        // Sprint 2.2.5a — when PG_VERSION is missing AND bun_path is bogus,
        // the function must return Err quickly (spawn fails before any
        // timer). This is the "graceful degradation" contract used by
        // main.rs Stage 3: a bogus bun shouldn't hang boot, just produce
        // an Err the caller logs + falls through.
        let dir = tempfile::tempdir().unwrap();
        // gbrain_home exists but PG_VERSION does not — forces the spawn path
        let result = ensure_bundled_gbrain_initialized(
            std::path::Path::new("/nonexistent/bun"),
            std::path::Path::new("/nonexistent/cli.ts"),
            dir.path(),
        )
        .await;
        assert!(result.is_err(), "bogus bun should yield Err");
        let msg = result.err().unwrap();
        // Should mention spawn failure, not timeout — the spawn fails
        // immediately, never reaching the timeout branch.
        assert!(
            msg.contains("spawn") || msg.contains("No such file"),
            "expected spawn-failure msg, got: {}",
            msg
        );
    }

    /// Sprint 2.2.5a — timeout sanity check. Spawns `sleep 300` (or
    /// equivalent) as the bun binary and a tiny override timeout to keep
    /// the test fast. Verifies we get an Err mentioning "timed out"
    /// rather than hanging for the full GBRAIN_INIT_TIMEOUT_SECS (120s).
    ///
    /// We can't reuse `ensure_bundled_gbrain_initialized` directly because
    /// the timeout const is baked at compile time. So we duplicate the
    /// relevant lines in a "probe" closure to exercise the same shape
    /// with a 1s timeout. If that core pattern is broken, the real
    /// function is too.
    #[tokio::test]
    async fn timeout_pattern_kills_hung_process() {
        // Skip if /bin/sleep doesn't exist (Windows CI, very minimal env)
        if !std::path::Path::new("/bin/sleep").exists() {
            eprintln!("Skipping timeout_pattern test — /bin/sleep not found");
            return;
        }
        let mut cmd = tokio::process::Command::new("/bin/sleep");
        cmd.arg("60").kill_on_drop(true);
        let result = tokio::time::timeout(Duration::from_millis(200), cmd.output()).await;
        assert!(result.is_err(), "timeout must fire on hung process");
        // The Elapsed error is what we want — process is killed on drop.
    }
}

#[cfg(test)]
mod pglite_lock_cleanup_tests {
    use super::*;
    use std::collections::HashMap;

    fn env_with_home(home: &std::path::Path) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("GBRAIN_HOME".to_string(), home.to_string_lossy().to_string());
        m
    }

    fn write_lock(home: &std::path::Path, pid: i64) -> std::path::PathBuf {
        let dir = home.join(".gbrain").join("brain.pglite").join(".gbrain-lock");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("lock"), format!("{{\"pid\": {pid}}}")).unwrap();
        dir
    }

    #[test]
    fn no_lock_file_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        cleanup_stale_pglite_lock(&env_with_home(tmp.path())); // must not panic
    }

    #[test]
    fn dead_pid_lock_is_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = write_lock(tmp.path(), 2_000_000_000); // implausibly-high pid → not alive
        cleanup_stale_pglite_lock(&env_with_home(tmp.path()));
        assert!(!dir.exists(), "dead-pid lock dir should be removed");
    }

    #[test]
    fn live_pid_lock_is_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = write_lock(tmp.path(), std::process::id() as i64); // current process → alive
        cleanup_stale_pglite_lock(&env_with_home(tmp.path()));
        assert!(dir.exists(), "live-pid lock dir should be kept");
    }
}
