//! [R4 工具/MCP/模型] `UclawToolFactory` — the `SessionOptions.tool_factory` that
//! keeps pi's **built-in** tools verbatim and is the injection point for uClaw's
//! own tools.
//!
//! F5 split:
//! - **Built-ins** (`read`/`bash`/`edit`/`write`/`grep`/`find`/`ls`/`hashline_edit`)
//!   are inherited unchanged via [`default_tool_registry`] — never reimplemented.
//!   Only their *output* is normalized for the renderers
//!   ([`crate::dto::tool_output_to_result`]).
//! - **uClaw-unique** tools (browser / skill / MCP + the interaction tools
//!   ask_user / exit_plan) are layered on top as `impl pi::sdk::Tool` via
//!   [`ToolRegistry::push`].
//!
//! Wrapping each uClaw tool needs a **cross-runtime bridge**: a wrapped tool's
//! `execute()` runs on pi's asupersync runtime, while uClaw's logic (MCP client,
//! browser, skills) is tokio-async. The interaction tools reuse the R3
//! [`ApprovalRegistry`] round-trip; the IO tools bridge to uClaw's tokio client
//! over a channel (the same data-only boundary the engine already uses). That
//! per-tool wiring is the remaining R4 scaling work — this factory is where each
//! `reg.push(...)` lands.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use pi::agent::ToolApprovalDecision;
use pi::error::Result as PiResult;
use pi::model::{ContentBlock, TextContent};
use pi::sdk::{default_tool_registry, Config, ToolFactory, ToolRegistry};
use pi::tools::{Tool, ToolEffects, ToolOutput, ToolUpdate};
use serde_json::{json, Value};

use crate::approval::ApprovalRegistry;
use crate::events::{event, EventSink};
use crate::tool_bridge::{BridgedIoTool, ToolRequestSink, ToolResultRegistry};

/// pi's built-in tool names, inherited verbatim (F5). Mirrors
/// `pi::sdk::BUILTIN_TOOL_NAMES`; kept here as documentation of the F5 boundary.
pub const PI_BUILTIN_TOOLS: &[&str] = &[
    "read",
    "bash",
    "edit",
    "write",
    "grep",
    "find",
    "ls",
    "hashline_edit",
];

/// Builds each session's [`ToolRegistry`]: pi built-ins + uClaw tools.
/// Holds the R3 [`ApprovalRegistry`] and the [`EventSink`] so wrapped interaction
/// tools can round-trip through the frontend.
pub struct UclawToolFactory {
    approval: ApprovalRegistry,
    sink: Arc<dyn EventSink>,
    /// Resolves IO-tool executions (tokio-resolver variant).
    tool_results: ToolResultRegistry,
    /// uClaw's tokio executor; `None` = no IO tools wired (interaction tools still
    /// work). Declares its tools via `io_tool_specs()`.
    tool_request_sink: Option<Arc<dyn ToolRequestSink>>,
    /// Owning conversation for the session this factory builds (set via
    /// [`UclawToolFactory::with_conversation`]); `None` on the shared engine-level
    /// factory. Captured into each `BridgedIoTool` for per-conv attribution.
    conversation_id: Option<String>,
}

impl UclawToolFactory {
    #[must_use]
    pub fn new(
        approval: ApprovalRegistry,
        sink: Arc<dyn EventSink>,
        tool_results: ToolResultRegistry,
        tool_request_sink: Option<Arc<dyn ToolRequestSink>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            approval,
            sink,
            tool_results,
            tool_request_sink,
            conversation_id: None,
        })
    }

    /// Clone this factory bound to a specific conversation. The engine builds one
    /// per session (sessions are 1:1 with `conv_id`), so each session's
    /// `BridgedIoTool`s carry their owning `conv_id` — letting per-conv side-
    /// effects (e.g. `agent:skill-recalled`) attribute to the right session
    /// rather than a shared placeholder. All fields are cheap handles (Arc /
    /// Clone registries).
    #[must_use]
    pub fn with_conversation(&self, conversation_id: String) -> Arc<Self> {
        Arc::new(Self {
            approval: self.approval.clone(),
            sink: Arc::clone(&self.sink),
            tool_results: self.tool_results.clone(),
            tool_request_sink: self.tool_request_sink.clone(),
            conversation_id: Some(conversation_id),
        })
    }
}

impl ToolFactory for UclawToolFactory {
    fn create_tool_registry(&self, enabled: &[&str], cwd: &Path, config: &Config) -> ToolRegistry {
        // F5: start from pi's built-in set verbatim — read/bash/edit/write/grep/
        // find/ls are pi's, never reimplemented.
        let mut reg = default_tool_registry(enabled, cwd, config);

        // uClaw-unique tools layered on top via `reg.push`:
        //   - interaction (exit_plan / ask_user) → reuse `self.approval` round-trip
        //     (the cross-runtime bridge: execute() on asupersync ↔ frontend respond)
        //   - IO (browser / skill / MCP) → same shape but resolve from a tokio
        //     executor instead of the frontend (next increment).
        reg.push(Box::new(ExitPlanTool::new(
            self.approval.clone(),
            Arc::clone(&self.sink),
        )));

        // IO tools (tokio-resolver): one BridgedIoTool per spec uClaw's executor
        // declares. execute() round-trips through the ToolRequestSink + the
        // ToolResultRegistry (resolved by EngineCmd::ToolResult).
        if let Some(req_sink) = &self.tool_request_sink {
            for spec in req_sink.io_tool_specs() {
                let result_timeout = spec.result_timeout;
                reg.push(Box::new(BridgedIoTool::new(
                    spec.name,
                    spec.label,
                    spec.description,
                    spec.parameters,
                    self.tool_results.clone(),
                    Arc::clone(req_sink),
                    self.conversation_id.clone(),
                    result_timeout,
                )));
            }
        }

        reg
    }
}

/// Wrap a plain-text result as a [`ToolOutput`] (the renderers read the flattened
/// text — see [`crate::dto::tool_output_to_result`]).
fn text_output(text: impl Into<String>) -> ToolOutput {
    ToolOutput {
        content: vec![ContentBlock::Text(TextContent {
            text: text.into(),
            text_signature: None,
        })],
        details: None,
        is_error: false,
    }
}

/// Map the user's plan-mode decision to the tool's result. Extracted so the
/// mapping is unit-testable without the async round-trip (which the
/// [`crate::approval`] registry tests already cover).
fn decision_to_output(decision: ToolApprovalDecision) -> ToolOutput {
    match decision {
        ToolApprovalDecision::Allow => text_output("Plan accepted; proceeding with execution."),
        ToolApprovalDecision::Deny { reason } => text_output(format!("Plan rejected by user: {reason}")),
    }
}

/// [R4 cross-runtime bridge] `exit_plan_mode` wrapped as a `pi::sdk::Tool`.
///
/// pi has no native plan mode — this is a uClaw tool pi can call. `execute()`
/// runs on pi's **asupersync** runtime and round-trips through the **frontend**:
/// emit `agent:exit_plan_request`, then await the user's decision via the R3
/// [`ApprovalRegistry`] (resolved by `respond_exit_plan_mode` → `EngineCmd::Respond`).
/// `Allow` = accept, `Deny` = reject. This is the bridge pattern every wrapped
/// uClaw tool follows; IO tools (MCP/browser/skill) swap the frontend for a tokio
/// executor as the resolver.
pub struct ExitPlanTool {
    approval: ApprovalRegistry,
    sink: Arc<dyn EventSink>,
}

impl ExitPlanTool {
    #[must_use]
    pub fn new(approval: ApprovalRegistry, sink: Arc<dyn EventSink>) -> Self {
        Self { approval, sink }
    }
}

#[async_trait]
impl Tool for ExitPlanTool {
    fn name(&self) -> &str {
        "exit_plan_mode"
    }
    fn label(&self) -> &str {
        "Exit Plan Mode"
    }
    fn description(&self) -> &str {
        "Request to exit plan mode and proceed with the proposed plan. The user accepts or rejects."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "plan": { "type": "string", "description": "The plan to execute once accepted." }
            },
            "required": []
        })
    }
    async fn execute(
        &self,
        tool_call_id: &str,
        input: Value,
        _on_update: Option<Box<dyn Fn(ToolUpdate) + Send + Sync>>,
    ) -> PiResult<ToolOutput> {
        let request_id = self.approval.next_request_id(tool_call_id);
        // Register BEFORE emitting so a fast respond always finds the waiter.
        let ticket = self.approval.register(request_id.clone());
        self.sink.emit(
            event::EXIT_PLAN_REQUEST,
            json!({
                "requestId": request_id,
                "plan": input.get("plan").and_then(Value::as_str).unwrap_or(""),
            }),
        );
        Ok(decision_to_output(ticket.await_decision().await))
    }
    fn effects(&self) -> ToolEffects {
        // Read-only: it only asks the user, mutates nothing itself.
        ToolEffects::read()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi::agent_cx::AgentCx;
    use std::time::Duration;

    struct NullSink;
    impl EventSink for NullSink {
        fn emit(&self, _event: &str, _payload: Value) {}
    }

    /// The factory's F5 inventory matches pi's built-in set (no tool dropped or
    /// reimplemented). This pins the F5 boundary; if pi adds a built-in, this
    /// fails until the list is reconciled.
    #[test]
    fn pi_builtin_inventory_is_the_f5_boundary() {
        assert_eq!(PI_BUILTIN_TOOLS, pi::sdk::BUILTIN_TOOL_NAMES);
    }

    #[test]
    fn exit_plan_tool_metadata_and_schema() {
        let tool = ExitPlanTool::new(
            ApprovalRegistry::new(AgentCx::for_testing(), Duration::from_secs(1)),
            Arc::new(NullSink),
        );
        assert_eq!(tool.name(), "exit_plan_mode");
        assert_eq!(tool.parameters()["type"], "object");
        assert!(tool.parameters()["properties"]["plan"].is_object());
    }

    /// [R4] The wrapped tool's decision → a ToolOutput that flattens to the
    /// renderer string (Done-when #1 path). Allow accepts; Deny carries the
    /// reason — neither is an error blob.
    #[test]
    fn exit_plan_decision_maps_to_renderable_output() {
        let accept = decision_to_output(ToolApprovalDecision::Allow);
        assert!(!accept.is_error);
        let r = crate::dto::tool_output_to_result(&accept);
        assert!(r.as_str().unwrap().contains("accepted"), "got {r}");

        let reject = decision_to_output(ToolApprovalDecision::deny("not ready"));
        let r = crate::dto::tool_output_to_result(&reject);
        let s = r.as_str().unwrap();
        assert!(s.contains("rejected") && s.contains("not ready"), "got {s}");
    }
}
