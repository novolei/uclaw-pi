# skills.sh Marketplace P5 — pi IO Bridge (`RealToolRequestSink`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (or executing-plans). Steps use `- [ ]`.

**Goal:** Wire the three skill tools (`skill_search`, `load_skill`, `skill_marketplace_search`) into the **pi engine** by replacing `StubToolRequestSink` with a real executor — delivering R4 Slice 1 (the IO-tool bridge keystone).

**Architecture:** The pi-engine IO-bridge infra already exists and is tested (`crates/uclaw-pi-engine/src/tool_bridge.rs`: `ToolRequestSink` trait, `BridgedIoTool`, `ToolResultRegistry`; `tool_factory.rs` wraps each `io_tool_specs()` entry as a `BridgedIoTool`). We implement the uClaw side: a `RealToolRequestSink` whose `io_tool_specs()` advertises the 3 skill tools and whose `request()` dispatches to the real async `Tool::execute` on Tauri's tokio runtime, replying via `EngineCmd::ToolResult`. The sink obtains the engine reply-handle post-spawn through a `OnceLock<Weak<PiEngine>>` (Weak breaks the sink↔engine ownership cycle).

**Tech Stack:** Rust — `tauri::async_runtime::spawn` (the engine thread is asupersync, so **not** `tokio::spawn`), `OnceLock<Weak<PiEngine>>`, the uClaw `Tool` trait (`crate::agent::tools::tool`).

**Verified anchors (2026-06-01):**
- Trait: `tool_bridge.rs:44-52` — `io_tool_specs(&self) -> Vec<IoToolSpec>` + `request(&self, request_id, tool_name, input)` (both **sync**; `request` non-blocking).
- `IoToolSpec` (`tool_bridge.rs:32-38`): `{ name, label, description, parameters: Value }` (all pub).
- Reply: `EngineCmd::ToolResult { request_id, text, is_error }` (`engine.rs:107-111`); send via `PiEngine::send(&self, EngineCmd) -> bool` (`engine.rs:184`, "callable from any thread incl. tokio").
- Wrap site (unchanged): `tool_factory.rs:99-110` calls `req_sink.io_tool_specs()` per `create_tool_registry`.
- Exports: `uclaw_pi_engine::{EngineCmd, PiEngine, ToolRequestSink, IoToolSpec}` (lib.rs:36-39).
- Wire site: `src-tauri/src/main.rs:239-257` (constructs `StubToolRequestSink`, `PiEngine::spawn(sink, Some(tool_request_sink), config)`, `app.manage(Arc::new(pi_engine))`). `AppState` is managed at `main.rs:232` (before this block).
- Tool ctors: `SkillSearchTool::new(registry, store, app_handle, conv_id, space_id).with_memu(memu)` (`builtin/skill_search.rs:33`); `LoadSkillTool::new(registry, store, app_handle, conv_id, space_id)` (`builtin/load_skill.rs:27`); `SkillMarketplaceSearchTool::new(api_key)` (`builtin/skill_marketplace.rs:53`). `Tool::execute(&self, Value) -> Result<ToolOutput, ToolError>`; `ToolOutput.result: Value`.
- AppState fields: `state.skills_registry: Arc<RwLock<SkillsRegistry>>`, `state.memory_graph_store: Arc<MemoryGraphStore>`, `state.memu_client: Option<Arc<MemUClient>>`, `state.db` (settings read for `skills_sh_api_key`). Legacy passes `space_id = "default"` (tauri_commands.rs:5485) — we match it.

**Scope (locked):**
- The 3 skill tools become callable from pi. Browser/MCP IO tools are explicitly **out** (separate R4 slices).
- `conv_id` = a fixed `"pi-agent"` placeholder and `space_id = "default"` (matches the legacy path's hardcoded default). Threading the real per-conversation id / active workspace into the pi sink is a **noted follow-up** (the `request()` signature carries no conversation context today).
- Only `skill_search` + `load_skill` + `skill_marketplace_search`. `skill_install_from_marketplace` stays UI-driven (P3 card), not a pi tool.

---

## Task 1: `RealToolRequestSink` in `engine_sink.rs`

**Files:**
- Modify: `src-tauri/src/engine_sink.rs` (remove `StubToolRequestSink`; add `RealToolRequestSink` + helpers + tests)

- [ ] **Step 1: Write the failing tests** (the two pure helpers)

Add to the `tests` module in `engine_sink.rs`:

```rust
#[test]
fn tool_result_text_maps_ok_and_err() {
    use crate::agent::tools::tool::{ToolError, ToolErrorKind, ToolOutput};
    let ok = tool_result_text(Ok(ToolOutput::new(serde_json::json!({"hits": 3}), 0)));
    assert!(!ok.1, "ok is not an error");
    assert!(ok.0.contains("hits"), "serialized result json: {}", ok.0);
    let err = tool_result_text(Err(ToolError::kinded(ToolErrorKind::Other, "boom")));
    assert!(err.1, "err flagged");
    assert!(err.0.contains("boom"), "err text: {}", err.0);
}

#[test]
fn spec_from_tool_maps_metadata() {
    use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
    struct FakeTool;
    #[async_trait::async_trait]
    impl Tool for FakeTool {
        fn name(&self) -> &str { "skill_search" }
        fn description(&self) -> &str { "search skills" }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {"query": {"type": "string"}}})
        }
        async fn execute(&self, _p: serde_json::Value) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::new(serde_json::json!({}), 0))
        }
    }
    let spec = spec_from_tool(&FakeTool);
    assert_eq!(spec.name, "skill_search");
    assert_eq!(spec.label, "skill_search");
    assert_eq!(spec.description, "search skills");
    assert_eq!(spec.parameters["properties"]["query"]["type"], "string");
}
```

> At execution: confirm `ToolError::kinded(ToolErrorKind, msg)` + `ToolOutput::new(Value, u64)` exist (Explore confirmed both in `agent/tools/tool.rs`). If `kinded` isn't the constructor, use `ToolError::Execution("boom".into())` and assert on its Display.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib -p uclaw engine_sink`
Expected: FAIL — `tool_result_text` / `spec_from_tool` not defined.

- [ ] **Step 3: Implement** — replace the `StubToolRequestSink` block with:

```rust
use std::sync::{OnceLock, Weak};

use crate::agent::tools::builtin::{load_skill, skill_marketplace, skill_search};
use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
use crate::app::AppState;

/// Fixed conversation id for pi-invoked skill tools. The bridge's `request()`
/// carries no conversation context, so events these tools emit use this
/// placeholder; threading the real per-conversation id is a follow-up.
const PI_TOOL_CONV: &str = "pi-agent";

/// [R4 IO 桥] The real uClaw tool executor for pi. `io_tool_specs()` advertises the
/// skill tools; `request()` runs the named async `Tool::execute` on Tauri's tokio
/// runtime and replies via `EngineCmd::ToolResult`. The engine reply-handle is
/// injected post-spawn (`attach_engine`) as a `Weak` to avoid a sink↔engine cycle.
pub struct RealToolRequestSink {
    app: AppHandle,
    engine: OnceLock<Weak<uclaw_pi_engine::PiEngine>>,
}

impl RealToolRequestSink {
    #[must_use]
    pub fn new(app: AppHandle) -> Self {
        Self { app, engine: OnceLock::new() }
    }

    /// Give the sink a weak handle to the spawned engine so `request()` can reply.
    /// Call once, right after `PiEngine::spawn`. Idempotent (later calls are no-ops).
    pub fn attach_engine(&self, engine: Weak<uclaw_pi_engine::PiEngine>) {
        let _ = self.engine.set(engine);
    }
}

/// Map a uClaw `Tool`'s metadata into a pi `IoToolSpec` (the bridge wraps each as a
/// `BridgedIoTool`). Pure — unit-tested.
fn spec_from_tool(tool: &dyn Tool) -> uclaw_pi_engine::IoToolSpec {
    uclaw_pi_engine::IoToolSpec {
        name: tool.name().to_string(),
        label: tool.name().to_string(),
        description: tool.description().to_string(),
        parameters: tool.parameters_schema(),
    }
}

/// Flatten a tool execution into `(text, is_error)` for `EngineCmd::ToolResult`.
/// Success → the result JSON serialized; error → the error's Display. Pure — tested.
fn tool_result_text(result: Result<ToolOutput, ToolError>) -> (String, bool) {
    match result {
        Ok(output) => (
            serde_json::to_string(&output.result).unwrap_or_else(|_| "{}".to_string()),
            false,
        ),
        Err(e) => (format!("{e}"), true),
    }
}

/// Construct the named skill tool from `AppState` handles, or `None` for an
/// unknown name. Used by both `io_tool_specs` (metadata) and `request` (execute).
fn build_skill_tool(name: &str, state: &AppState, app: &AppHandle) -> Option<Box<dyn Tool>> {
    match name {
        "skill_search" => Some(Box::new(
            skill_search::SkillSearchTool::new(
                std::sync::Arc::clone(&state.skills_registry),
                std::sync::Arc::clone(&state.memory_graph_store),
                app.clone(),
                PI_TOOL_CONV.to_string(),
                "default".to_string(),
            )
            .with_memu(state.memu_client.clone()),
        )),
        "load_skill" => Some(Box::new(load_skill::LoadSkillTool::new(
            std::sync::Arc::clone(&state.skills_registry),
            std::sync::Arc::clone(&state.memory_graph_store),
            app.clone(),
            PI_TOOL_CONV.to_string(),
            "default".to_string(),
        ))),
        "skill_marketplace_search" => {
            let api_key = state
                .db
                .lock()
                .ok()
                .and_then(|c| {
                    c.query_row(
                        "SELECT value FROM settings WHERE key='skills_sh_api_key'",
                        [],
                        |r| r.get::<_, String>(0),
                    )
                    .ok()
                });
            Some(Box::new(skill_marketplace::SkillMarketplaceSearchTool::new(api_key)))
        }
        _ => None,
    }
}

/// The skill tools pi may call. Order is the advertised order.
const PI_SKILL_TOOLS: &[&str] = &["skill_search", "load_skill", "skill_marketplace_search"];

/// Build + execute the named tool, returning `(text, is_error)`. The `AppState`
/// guard is dropped before the `.await` (it is not `Send`).
async fn run_skill_tool(app: &AppHandle, tool_name: &str, input: serde_json::Value) -> (String, bool) {
    let tool = {
        let Some(state) = app.try_state::<AppState>() else {
            return ("agent state unavailable".to_string(), true);
        };
        build_skill_tool(tool_name, &state, app)
    };
    match tool {
        Some(tool) => tool_result_text(tool.execute(input).await),
        None => (format!("unknown IO tool: {tool_name}"), true),
    }
}

impl uclaw_pi_engine::ToolRequestSink for RealToolRequestSink {
    fn io_tool_specs(&self) -> Vec<uclaw_pi_engine::IoToolSpec> {
        let Some(state) = self.app.try_state::<AppState>() else {
            return Vec::new();
        };
        PI_SKILL_TOOLS
            .iter()
            .filter_map(|name| build_skill_tool(name, &state, &self.app))
            .map(|t| spec_from_tool(t.as_ref()))
            .collect()
    }

    fn request(&self, request_id: &str, tool_name: &str, input: &serde_json::Value) {
        let app = self.app.clone();
        let engine = self.engine.get().cloned();
        let request_id = request_id.to_string();
        let tool_name = tool_name.to_string();
        let input = input.clone();
        // The engine thread is asupersync — spawn onto Tauri's tokio runtime, NOT
        // tokio::spawn (no tokio reactor on this thread).
        tauri::async_runtime::spawn(async move {
            let (text, is_error) = run_skill_tool(&app, &tool_name, input).await;
            match engine.and_then(|w| w.upgrade()) {
                Some(engine) => {
                    engine.send(uclaw_pi_engine::EngineCmd::ToolResult { request_id, text, is_error });
                }
                None => tracing::warn!(
                    request_id, tool_name,
                    "pi tool result dropped: engine handle not attached"
                ),
            }
        });
    }
}
```

- [ ] **Step 4: Run tests + build**

Run: `cargo test --lib -p uclaw engine_sink` (expect the 2 new + existing pass) and `cargo build 2>&1 | grep -E "^error"` (expect none; note `main.rs` still references `StubToolRequestSink` until Task 2 — if build fails ONLY on that, proceed to Task 2 then rebuild).

> Because removing `StubToolRequestSink` breaks `main.rs:246`, Tasks 1+2 compile together. Run `cargo test --lib engine_sink` (compiles the lib, not the bin) to green the unit tests first; the full `cargo build` goes green after Task 2.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/engine_sink.rs
git commit -m "feat(engine_sink): RealToolRequestSink — run skill tools for pi via EngineCmd::ToolResult (P5 T1)"
```

---

## Task 2: Wire `RealToolRequestSink` in `main.rs`

**Files:**
- Modify: `src-tauri/src/main.rs:239-257`

- [ ] **Step 1: Replace the sink construction + add the weak attach**

```rust
            {
                let sink = uclaw_core::engine_sink::TauriEventSink::new(app.handle().clone());
                // [R4 IO 桥] The real tool executor: skill_search / load_skill /
                // skill_marketplace_search run on tokio and reply via EngineCmd::ToolResult.
                let real_sink = std::sync::Arc::new(
                    uclaw_core::engine_sink::RealToolRequestSink::new(app.handle().clone()),
                );
                let tool_request_sink: std::sync::Arc<dyn uclaw_pi_engine::ToolRequestSink> =
                    real_sink.clone();
                let pi_engine = std::sync::Arc::new(uclaw_pi_engine::PiEngine::spawn(
                    sink,
                    Some(tool_request_sink),
                    uclaw_pi_engine::EngineConfig {
                        no_session: true,
                        ..Default::default()
                    },
                ));
                // Hand the sink a Weak engine handle so its tokio executor can reply
                // (Weak avoids the sink↔engine ownership cycle). Must precede any prompt.
                real_sink.attach_engine(std::sync::Arc::downgrade(&pi_engine));
                app.manage(pi_engine);
                tracing::info!("[R4] PiEngine spawned with RealToolRequestSink (skill tools wired)");
            }
```

- [ ] **Step 2: Full build + the marketplace/engine suites**

Run: `cargo build 2>&1 | grep -E "^error" | head` (expect none) and `cargo test --lib -p uclaw "engine_sink" "skills_marketplace"` (expect pass).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat(main): wire RealToolRequestSink into PiEngine (skill tools live in pi) (P5 T2)"
```

---

## Task 3: Verify + manual checkpoint + PR

- [ ] **Step 1: Build + test green**

`cargo build` clean; `cargo test --lib -p uclaw engine_sink skills_marketplace` pass.

- [ ] **Step 2: ⚠️ Manual checkpoint (cannot drive the native window / live model)**

Document for a human spot-check:
1. Settings → enable the **pi engine** toggle (#19) + set a `skills_sh_api_key` (#23).
2. In a pi-backed chat, ask "search for a skill that does X" → pi should call `skill_search` / `skill_marketplace_search`; the tool result returns (verify via the tool-result card — P3 renders `skill_marketplace_search`).
3. Confirm no `"engine handle not attached"` or `"executor is not wired (stub)"` warnings in `~/.uclaw-pi/logs/`.

- [ ] **Step 3: Final review subagent over the P5 diff, then PR.**

---

## Self-Review

- **Spec coverage:** Design §6 / P5 row = "pi `RealToolRequestSink` (skill_search/load_skill/skill_search_marketplace) + main.rs 接线". Tasks 1+2 deliver exactly that; browser/MCP IO tools explicitly deferred (separate R4 slices). ✓
- **Type consistency:** `IoToolSpec { name, label, description, parameters }` matches the engine struct; `EngineCmd::ToolResult { request_id, text, is_error }` matches; `Tool::execute -> Result<ToolOutput, ToolError>` with `ToolOutput.result: Value`. ✓
- **Threading:** `request()` is sync/non-blocking → `tauri::async_runtime::spawn` (NOT tokio); the `State` guard drops before `.await`; reply via `PiEngine::send` (any-thread). ✓
- **No placeholders:** full code in every step; the two `NOTE at execution` lines are verification guards (ToolError ctor; build-order), not deferred work.
- **Known limitation (noted, not silent):** `conv_id`="pi-agent" / `space_id`="default" (matches legacy); per-conversation/workspace threading is a follow-up.
