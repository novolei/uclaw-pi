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

use pi::sdk::{default_tool_registry, Config, ToolFactory, ToolRegistry};

use crate::approval::ApprovalRegistry;
use crate::events::EventSink;

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
    #[allow(dead_code)] // consumed by wrapped interaction tools (R4 scaling work)
    approval: ApprovalRegistry,
    #[allow(dead_code)]
    sink: Arc<dyn EventSink>,
}

impl UclawToolFactory {
    #[must_use]
    pub fn new(approval: ApprovalRegistry, sink: Arc<dyn EventSink>) -> Arc<Self> {
        Arc::new(Self { approval, sink })
    }
}

impl ToolFactory for UclawToolFactory {
    fn create_tool_registry(&self, enabled: &[&str], cwd: &Path, config: &Config) -> ToolRegistry {
        // F5: start from pi's built-in set verbatim — read/bash/edit/write/grep/
        // find/ls are pi's, never reimplemented.
        let reg = default_tool_registry(enabled, cwd, config);

        // uClaw-unique tools land here via `reg.push(Box::new(...))`:
        //   - interaction (ask_user / exit_plan) → reuse `self.approval` round-trip
        //   - IO (browser / skill / MCP) → bridge `execute()` to uClaw's tokio
        //     client over a channel (cross-runtime, data-only)
        // Each tool's wiring is the remaining R4 work; this is the injection point.

        reg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The factory's F5 inventory matches pi's built-in set (no tool dropped or
    /// reimplemented). This pins the F5 boundary; if pi adds a built-in, this
    /// fails until the list is reconciled.
    #[test]
    fn pi_builtin_inventory_is_the_f5_boundary() {
        assert_eq!(PI_BUILTIN_TOOLS, pi::sdk::BUILTIN_TOOL_NAMES);
    }
}
