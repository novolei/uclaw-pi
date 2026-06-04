//! [R4 cross-runtime bridge — IO tools] The **tokio-resolver** variant of the
//! wrapped-tool pattern.
//!
//! Interaction tools (`ExitPlanTool` in [`crate::tool_factory`]) resolve from the
//! **frontend** via the R3 [`crate::approval::ApprovalRegistry`]. IO tools
//! (MCP / browser / skill) instead resolve from a **tokio executor**: the wrapped
//! tool's `execute()` (asupersync) requests execution over a [`ToolRequestSink`],
//! uClaw's tokio side runs the real tool, and the result returns as
//! `EngineCmd::ToolResult` → resolves the [`ToolResultRegistry`].
//!
//! Engine-side here is fully testable. The tokio executor (a uClaw
//! [`ToolRequestSink`] impl that dispatches to `mcp.rs` / browser / skills and
//! replies via `EngineCmd::ToolResult`) is the integration seam.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use async_trait::async_trait;
use asupersync::channel::oneshot;
use asupersync::time::{timeout, wall_now};
use pi::agent_cx::AgentCx;
use pi::error::Result as PiResult;
use pi::model::{ContentBlock, TextContent};
use pi::tools::{Tool, ToolEffects, ToolOutput, ToolUpdate};
use serde_json::Value;

/// One uClaw IO tool the executor can run, as pi tool metadata. The factory turns
/// each spec into a [`BridgedIoTool`]. Keeping the list on the uClaw-provided sink
/// keeps the engine ignorant of uClaw's specific tools.
#[derive(Clone, Debug)]
pub struct IoToolSpec {
    pub name: String,
    pub label: String,
    pub description: String,
    pub parameters: Value,
    /// Per-tool override for the result-wait timeout. `None` ⇒ the registry's
    /// default (~5 min). Interaction tools that block on a human (`ask_user`) set
    /// a generous window so a slow answer isn't dropped: the default would
    /// fail-close the tool while the banner is still open, then silently drop the
    /// user's late answer (and leave an orphan tool_call — see #93).
    pub result_timeout: Option<Duration>,
}

/// Engine → tokio: ask uClaw to execute a wrapped tool. uClaw provides the impl
/// (a channel to a tokio executor that runs MCP/browser/skill and replies via
/// `EngineCmd::ToolResult`). Abstracted like `EventSink` so the engine stays
/// runtime-agnostic and testable.
pub trait ToolRequestSink: Send + Sync + 'static {
    /// The IO tools this executor can run (name + schema). The factory wraps each
    /// as a `pi::sdk::Tool`. May be empty (no IO tools wired yet).
    fn io_tool_specs(&self) -> Vec<IoToolSpec>;

    /// Dispatch `tool_name`(`input`) for `request_id`. Non-blocking: the result
    /// arrives later as `EngineCmd::ToolResult { request_id, .. }`. `conversation_id`
    /// is the owning session (when known) so the executor can attribute per-conv
    /// side-effects (e.g. uClaw's `agent:skill-recalled` UI event) to the right
    /// session instead of a placeholder; `None` keeps the executor's default.
    fn request(&self, conversation_id: Option<&str>, request_id: &str, tool_name: &str, input: &Value);
}

type PendingMap = Arc<StdMutex<HashMap<String, oneshot::Sender<ToolOutput>>>>;

/// Build a plain-text [`ToolOutput`] (renderers read the flattened text).
fn text_output(text: impl Into<String>, is_error: bool) -> ToolOutput {
    ToolOutput {
        content: vec![ContentBlock::Text(TextContent {
            text: text.into(),
            text_signature: None,
        })],
        details: None,
        is_error,
    }
}

/// Build a [`ToolOutput`] from plain text + error flag — used by the engine to
/// resolve `EngineCmd::ToolResult` into the value the awaiting tool returns.
#[must_use]
pub fn tool_output_text(text: impl Into<String>, is_error: bool) -> ToolOutput {
    text_output(text, is_error)
}

/// RAII: drop the pending slot on every exit path (resolve / cancel / timeout).
struct PendingGuard {
    pending: PendingMap,
    key: String,
}
impl Drop for PendingGuard {
    fn drop(&mut self) {
        if let Ok(mut g) = self.pending.lock() {
            g.remove(&self.key);
        }
    }
}

/// Resolves IO-tool executions: register a oneshot keyed by request id, await the
/// tokio executor's [`ToolOutput`] with a timeout (fail-closed to an error output
/// — a tool never silently hangs). Mirrors the R3 approval machinery for the
/// `ToolOutput` response type. Cheap to clone.
#[derive(Clone)]
pub struct ToolResultRegistry {
    pending: PendingMap,
    counter: Arc<AtomicU64>,
    timeout: Duration,
    cx: AgentCx,
}

impl ToolResultRegistry {
    #[must_use]
    pub fn new(cx: AgentCx, timeout: Duration) -> Self {
        Self {
            pending: Arc::new(StdMutex::new(HashMap::new())),
            counter: Arc::new(AtomicU64::new(0)),
            timeout,
            cx,
        }
    }

    /// A fresh, unique request id for an IO-tool execution.
    #[must_use]
    pub fn next_request_id(&self) -> String {
        format!("tool-{}", self.counter.fetch_add(1, Ordering::SeqCst))
    }

    /// Resolve a pending execution (from the engine loop on `EngineCmd::ToolResult`).
    /// Returns `true` iff a waiter was found.
    pub fn respond(&self, request_id: &str, output: ToolOutput) -> bool {
        let tx = self
            .pending
            .lock()
            .ok()
            .and_then(|mut g| g.remove(request_id));
        match tx {
            Some(tx) => tx.send(self.cx.cx(), output).is_ok(),
            None => false,
        }
    }

    /// Number of in-flight executions (tests / diagnostics).
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// Synchronously register `request_id` and return its ticket. Call this
    /// **before** dispatching to the executor so a fast reply always lands.
    #[must_use]
    pub fn register(&self, request_id: String) -> ResultTicket {
        self.register_with_timeout(request_id, None)
    }

    /// Like [`register`](Self::register) but with a per-tool timeout override
    /// (`None` ⇒ the registry default). Used for interaction tools (`ask_user`)
    /// that legitimately block on a human for minutes.
    #[must_use]
    pub fn register_with_timeout(
        &self,
        request_id: String,
        timeout_override: Option<Duration>,
    ) -> ResultTicket {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut g) = self.pending.lock() {
            g.insert(request_id.clone(), tx);
        }
        ResultTicket {
            rx,
            _guard: PendingGuard {
                pending: Arc::clone(&self.pending),
                key: request_id,
            },
            timeout: timeout_override.unwrap_or(self.timeout),
        }
    }
}

/// A registered IO-tool execution. Hold it across the dispatch, then `.await` it.
pub struct ResultTicket {
    rx: oneshot::Receiver<ToolOutput>,
    _guard: PendingGuard,
    timeout: Duration,
}

impl ResultTicket {
    /// Await the executor's result with the registry timeout (fail-closed to an
    /// error output — an IO tool never silently hangs).
    pub async fn await_result(mut self) -> ToolOutput {
        let cx = AgentCx::for_current_or_request();
        match timeout(wall_now(), self.timeout, Box::pin(self.rx.recv(cx.cx()))).await {
            Ok(Ok(output)) => output,
            Ok(Err(_)) => text_output("tool result channel closed", true),
            Err(_) => text_output("tool execution timed out", true),
        }
    }
}

/// A uClaw IO tool (MCP / browser / skill) wrapped as a `pi::sdk::Tool`. Its
/// `execute()` (asupersync) registers, asks uClaw's tokio executor to run the
/// real tool, and awaits the [`ToolOutput`].
pub struct BridgedIoTool {
    name: String,
    label: String,
    description: String,
    parameters: Value,
    registry: ToolResultRegistry,
    sink: Arc<dyn ToolRequestSink>,
    /// Owning conversation, captured when this session's registry is built (the
    /// registry is per-session ⇒ per-conv), so `request()` can attribute per-conv
    /// side-effects to the right session. `None` = the executor's default.
    conversation_id: Option<String>,
    /// Per-tool result-wait timeout (from `IoToolSpec`); `None` ⇒ registry
    /// default. Interaction tools (`ask_user`) set a generous window.
    result_timeout: Option<Duration>,
}

impl BridgedIoTool {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
        registry: ToolResultRegistry,
        sink: Arc<dyn ToolRequestSink>,
        conversation_id: Option<String>,
        result_timeout: Option<Duration>,
    ) -> Self {
        Self {
            name: name.into(),
            label: label.into(),
            description: description.into(),
            parameters,
            registry,
            sink,
            conversation_id,
            result_timeout,
        }
    }
}

#[async_trait]
impl Tool for BridgedIoTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn label(&self) -> &str {
        &self.label
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn parameters(&self) -> Value {
        self.parameters.clone()
    }
    async fn execute(
        &self,
        _tool_call_id: &str,
        input: Value,
        _on_update: Option<Box<dyn Fn(ToolUpdate) + Send + Sync>>,
    ) -> PiResult<ToolOutput> {
        let request_id = self.registry.next_request_id();
        // Register BEFORE dispatching so a fast executor reply always lands.
        // Interaction tools (ask_user) carry a generous timeout so a human's
        // slow answer isn't dropped as a "timeout".
        let ticket = self
            .registry
            .register_with_timeout(request_id.clone(), self.result_timeout);
        self.sink
            .request(self.conversation_id.as_deref(), &request_id, &self.name, &input);
        Ok(ticket.await_result().await)
    }
    fn effects(&self) -> ToolEffects {
        // IO tools may touch the network / external state — declare write so the
        // scheduler serializes them fail-closed (matches pi's default).
        ToolEffects::write()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asupersync::runtime::RuntimeBuilder;

    #[test]
    fn respond_resolves_pending_execution() {
        let reg = ToolResultRegistry::new(AgentCx::for_testing(), Duration::from_secs(5));
        // Register a slot the way await_result does, then respond.
        let (tx, _rx) = oneshot::channel();
        reg.pending.lock().unwrap().insert("tool-0".into(), tx);
        assert_eq!(reg.pending_len(), 1);
        assert!(reg.respond("tool-0", text_output("ok", false)));
        assert!(!reg.respond("tool-0", text_output("again", false)));
        assert!(!reg.respond("missing", text_output("x", false)));
    }

    #[test]
    fn next_request_id_is_unique() {
        let reg = ToolResultRegistry::new(AgentCx::for_testing(), Duration::from_secs(5));
        assert_ne!(reg.next_request_id(), reg.next_request_id());
    }

    #[test]
    fn await_result_times_out_fail_closed_to_error_output() {
        let runtime = RuntimeBuilder::current_thread().build().expect("rt");
        runtime.block_on(async {
            let reg = ToolResultRegistry::new(AgentCx::for_testing(), Duration::from_millis(40));
            // No executor responds → fail-closed error ToolOutput (never hang).
            let out = reg.register("tool-stuck".into()).await_result().await;
            assert!(out.is_error);
            assert_eq!(reg.pending_len(), 0, "slot cleaned on timeout");
        });
    }

    #[test]
    fn register_with_timeout_override_governs_over_registry_default() {
        // The per-tool override (interaction tools like ask_user) must win over
        // the registry default. Long default (60s) + short override (40ms): if the
        // override were ignored this would wait ~60s; a fast fail-closed proves it
        // applies. (The real direction is the inverse — a long override on top of
        // a short default — but the same code path; a short override is the only
        // way to assert it without sleeping for the default.)
        let runtime = RuntimeBuilder::current_thread().build().expect("rt");
        runtime.block_on(async {
            let reg = ToolResultRegistry::new(AgentCx::for_testing(), Duration::from_secs(60));
            let out = reg
                .register_with_timeout("tool-x".into(), Some(Duration::from_millis(40)))
                .await_result()
                .await;
            assert!(out.is_error, "short override governs over the 60s default");
            assert_eq!(reg.pending_len(), 0);
        });
    }

    #[test]
    fn register_then_respond_then_await_resolves_to_output() {
        let runtime = RuntimeBuilder::current_thread().build().expect("rt");
        runtime.block_on(async {
            let reg = ToolResultRegistry::new(AgentCx::for_testing(), Duration::from_secs(5));
            // Pre-seed the oneshot (value queued before await), like a fast executor.
            let (tx, rx) = oneshot::channel();
            reg.pending.lock().unwrap().insert("tool-7".into(), tx);
            assert!(reg.respond("tool-7", text_output("hello from tokio", false)));
            // Now await the SAME receiver (mirror await_result's recv).
            let cx = AgentCx::for_testing();
            let mut rx = rx;
            let out = rx.recv(cx.cx()).await.expect("value");
            let r = crate::dto::tool_output_to_result(&out);
            assert_eq!(r.as_str().unwrap(), "hello from tokio");
        });
    }
}
