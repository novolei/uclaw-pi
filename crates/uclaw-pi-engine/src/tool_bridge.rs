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

/// Engine → tokio: ask uClaw to execute a wrapped tool. uClaw provides the impl
/// (a channel to a tokio executor that runs MCP/browser/skill and replies via
/// `EngineCmd::ToolResult`). Abstracted like `EventSink` so the engine stays
/// runtime-agnostic and testable.
pub trait ToolRequestSink: Send + Sync + 'static {
    /// Dispatch `tool_name`(`input`) for `request_id`. Non-blocking: the result
    /// arrives later as `EngineCmd::ToolResult { request_id, .. }`.
    fn request(&self, request_id: &str, tool_name: &str, input: &Value);
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
            timeout: self.timeout,
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
    ) -> Self {
        Self {
            name: name.into(),
            label: label.into(),
            description: description.into(),
            parameters,
            registry,
            sink,
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
        let ticket = self.registry.register(request_id.clone());
        self.sink.request(&request_id, &self.name, &input);
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
