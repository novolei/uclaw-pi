//! [R1 接线] Tauri `EventSink` adapter — bridges `uclaw-pi-engine`'s `EventSink`
//! to Tauri's `AppHandle::emit`, so `PiEngine`'s `chat:stream-*` events reach the
//! frontend (`useGlobalAgentListeners`). See docs/R1-wiring-plan.md §3.

use std::sync::{Arc, OnceLock, Weak};

use tauri::{AppHandle, Emitter, Manager};
use uclaw_pi_engine::EventSink;

/// Wraps the Tauri `AppHandle` and forwards engine events to the frontend.
pub struct TauriEventSink {
    app: AppHandle,
    /// Last `agent:turn_cost` payload per conversation, cached when emitted (on
    /// AgentEnd, just before `chat:stream-complete`) so `persist_assistant` can
    /// write the token/cost/duration columns onto the assistant row — the
    /// metadata badge must survive reload. Keyed by conversationId.
    turn_cost: std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>,
}

impl TauriEventSink {
    #[must_use]
    pub fn new(app: AppHandle) -> Arc<dyn EventSink> {
        Arc::new(Self {
            app,
            turn_cost: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }
}

impl TauriEventSink {
    /// [R2 闭环] Persist the assistant message to uClaw SQLite from a
    /// `chat:stream-complete` payload (`{conversationId, text, …}`). F2: uClaw
    /// SQLite is the source of truth, so the frontend's complete→`get_messages`
    /// refresh renders this row. Best-effort: any failure is logged, never blocks
    /// the emit. Runs on the engine thread — `state.db` is a sync `Mutex`, safe
    /// to lock from any thread.
    fn persist_assistant(&self, payload: &serde_json::Value) {
        let (Some(conv), Some(text)) = (
            payload
                .get("conversationId")
                .and_then(serde_json::Value::as_str),
            payload.get("text").and_then(serde_json::Value::as_str),
        ) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        // Thinking/reasoning text the ACL accumulated over the turn → persisted so
        // the Agent view re-renders the THINKING block on reload (the live badge is
        // driven by chat:stream-reasoning; this is its durable counterpart). Empty
        // ⇒ None (no thinking block written).
        let reasoning = payload
            .get("reasoning")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty());
        // The turn's token/cost/duration (cached from the preceding
        // agent:turn_cost) → the assistant row's metadata-badge columns.
        let usage = self
            .turn_cost
            .lock()
            .ok()
            .and_then(|cache| cache.get(conv).cloned())
            .map(|v| crate::engine_persist::TurnUsage::from_turn_cost(&v))
            .unwrap_or_default();
        let Some(state) = self.app.try_state::<crate::app::AppState>() else {
            return;
        };
        let Ok(conn) = state.db.lock() else { return };
        let id = uuid::Uuid::new_v4().to_string();
        // Route to the conversation's actual table: an Agent-view session lives in
        // agent_messages (read by get_agent_session_messages); a chat conversation
        // lives in messages (read by get_messages).
        let is_agent_session = conn
            .query_row(
                "SELECT 1 FROM agent_sessions WHERE id = ?1",
                [conv],
                |_| Ok(()),
            )
            .is_ok();
        let result = if is_agent_session {
            crate::engine_persist::persist_agent_text_message(
                &conn, &id, conv, "assistant", text, reasoning, &usage,
            )
        } else {
            crate::engine_persist::persist_chat_text_message(&conn, &id, conv, "assistant", text, reasoning)
        };
        if let Err(e) = result {
            tracing::warn!("PiEngine assistant persist failed: {e}");
        }
        // bucket_seal ingest 通水 — feed the assistant turn into the openhuman
        // bucket-seal tree. Best-effort, fire-and-forget; the async task writes
        // chunks.db (not this `conn`), so spawning while the state.db guard is
        // still held is safe.
        crate::engine_persist::spawn_bucket_seal_ingest(
            state.bucket_seal_adapter.clone(),
            conv.to_string(),
            "assistant".to_string(),
            text.to_string(),
        );

        // P3: turn-count trigger — every N agent turns, distill recent turns
        // into a reflection. Fire-and-forget on Tauri's runtime; run_once is
        // best-effort and can never break this turn. Agent sessions only (chat
        // turns don't populate agent_messages, which the pass reads).
        const REFLECTION_EVERY_N_TURNS: u64 = 20;
        if is_agent_session {
            let n = state
                .reflection_turn_counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1;
            if crate::memory_graph::reflection_service::should_run_reflection(
                n,
                REFLECTION_EVERY_N_TURNS,
            ) {
                let app = self.app.clone();
                tauri::async_runtime::spawn(async move {
                    crate::memory_graph::reflection_service::run_once(app).await;
                });
            }
        }
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        let mut payload = payload;
        // [metadata] On agent:turn_cost (emitted on AgentEnd, just before
        // stream-complete): delegate the cache-aware cost recompute + cost_records
        // recording to the cost service (pi leaves cost=0 for ad-hoc models), set
        // the corrected costUsd back, and cache the payload so persist_assistant
        // writes the cost_usd column. This bridge stays a translator (ADR 2026-05-31).
        if event == "agent:turn_cost" {
            if let (Some(model), Some(input), Some(output), Some(conv)) = (
                payload.get("model").and_then(serde_json::Value::as_str).map(str::to_owned),
                payload.get("inputTokens").and_then(serde_json::Value::as_u64),
                payload.get("outputTokens").and_then(serde_json::Value::as_u64),
                payload.get("conversationId").and_then(serde_json::Value::as_str).map(str::to_owned),
            ) {
                let cache_read = payload
                    .get("cacheReadTokens")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                if let Some(state) = self.app.try_state::<crate::app::AppState>() {
                    use crate::services::cost_service::CostService as _;
                    let cost_usd = crate::services::cost_service::PricingCostService.settle_turn(
                        &state, &conv, &model, input as u32, output as u32, cache_read as u32,
                    );
                    if let Some(obj) = payload.as_object_mut() {
                        obj.insert("costUsd".into(), serde_json::Value::String(cost_usd));
                    }
                }
            }
            if let Some(conv) = payload.get("conversationId").and_then(serde_json::Value::as_str) {
                if let Ok(mut cache) = self.turn_cost.lock() {
                    cache.insert(conv.to_owned(), payload.clone());
                }
            }
        }
        // [R2 闭环] On complete, persist the assistant message BEFORE emitting so
        // the frontend's complete→refresh sees it. Gated by the same migration
        // flag as the routing (only fires when the engine path is active).
        if event == uclaw_pi_engine::event::STREAM_COMPLETE && pi_engine_enabled() {
            self.persist_assistant(&payload);
        }
        // `app.emit` is thread-safe (callable from the engine thread). Failures
        // (no webview yet, serialization) are logged, never panic.
        if let Err(e) = self.app.emit(event, payload) {
            tracing::warn!("PiEngine EventSink emit {event} failed: {e}");
        }
    }
}

use crate::agent::tools::builtin::{load_skill, skill_marketplace, skill_search};
use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
use crate::app::AppState;

/// Fixed conversation id for pi-invoked skill tools. The bridge's `request()`
/// carries no conversation context, so events these tools emit use this
/// placeholder; threading the real per-conversation id is a follow-up.
const PI_TOOL_CONV: &str = "pi-agent";

/// The skill tools pi may call, in advertised order.
const PI_SKILL_TOOLS: &[&str] = &["skill_search", "load_skill", "skill_marketplace_search"];

/// [R4 IO 桥] The real uClaw tool executor for pi. `io_tool_specs()` advertises the
/// skill tools; `request()` runs the named async `Tool::execute` on Tauri's tokio
/// runtime (the engine thread is asupersync — not a tokio reactor) and replies via
/// `EngineCmd::ToolResult`. The engine reply-handle is injected post-spawn
/// (`attach_engine`) as a `Weak` to avoid a sink↔engine ownership cycle.
pub struct RealToolRequestSink {
    app: AppHandle,
    engine: OnceLock<Weak<uclaw_pi_engine::PiEngine>>,
}

impl RealToolRequestSink {
    #[must_use]
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            engine: OnceLock::new(),
        }
    }

    /// Give the sink a weak handle to the spawned engine so `request()` can reply.
    /// Call once, right after `PiEngine::spawn`; later calls are no-ops.
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

/// Flatten a tool execution into `(text, is_error)` for `EngineCmd::ToolResult`:
/// success → the result JSON serialized; error → the error's Display. Pure — tested.
fn tool_result_text(result: Result<ToolOutput, ToolError>) -> (String, bool) {
    match result {
        Ok(output) => (
            serde_json::to_string(&output.result).unwrap_or_else(|_| "{}".to_string()),
            false,
        ),
        Err(e) => (format!("{e}"), true),
    }
}

/// Construct the named skill tool from `AppState` handles, or `None` for an unknown
/// name. Used by both `io_tool_specs` (metadata) and `request` (execution).
fn build_skill_tool(name: &str, state: &AppState, app: &AppHandle) -> Option<Box<dyn Tool>> {
    // `let _: Box<dyn Tool> = match …` makes each arm a trait-object coercion site.
    let tool: Box<dyn Tool> = match name {
        "skill_search" => Box::new(
            skill_search::SkillSearchTool::new(
                Arc::clone(&state.skills_registry),
                Arc::clone(&state.memory_graph_store),
                app.clone(),
                PI_TOOL_CONV.to_string(),
                "default".to_string(),
            )
            .with_memu(state.memu_client.clone()),
        ),
        "load_skill" => Box::new(load_skill::LoadSkillTool::new(
            Arc::clone(&state.skills_registry),
            Arc::clone(&state.memory_graph_store),
            app.clone(),
            PI_TOOL_CONV.to_string(),
            "default".to_string(),
        )),
        "skill_marketplace_search" => {
            // Both keys: skillsmp is the keyless default; skills.sh needs its key.
            let (skills_sh_key, skillsmp_key) = state
                .db
                .lock()
                .ok()
                .map(|c| {
                    let read = |k: &str| {
                        c.query_row("SELECT value FROM settings WHERE key=?1", [k], |r| {
                            r.get::<_, String>(0)
                        })
                        .ok()
                    };
                    (read("skills_sh_api_key"), read("skillsmp_api_key"))
                })
                .unwrap_or((None, None));
            Box::new(skill_marketplace::SkillMarketplaceSearchTool::new(
                skills_sh_key,
                skillsmp_key,
            ))
        }
        _ => return None,
    };
    Some(tool)
}

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

/// Build + execute a bridged MCP tool (`mcp__server__tool`), returning
/// `(text, is_error)`. Mirrors [`run_skill_tool`]: the manager read-guard is
/// dropped before the `.await` (not `Send`), and execution goes through
/// `McpToolProxy::execute` — the same lock-free network call legacy uses, with
/// the same `tool_result_text` shape as skill tools so results render uniformly.
async fn run_mcp_tool(app: &AppHandle, tool_name: &str, input: serde_json::Value) -> (String, bool) {
    let proxy = {
        let Some(state) = app.try_state::<AppState>() else {
            return ("agent state unavailable".to_string(), true);
        };
        let mgr = state.mcp_manager.read().await;
        crate::mcp::McpManager::create_tool_proxies(&state.mcp_manager, &mgr)
            .into_iter()
            .find(|p| p.name() == tool_name)
    };
    match proxy {
        Some(proxy) => tool_result_text(proxy.execute(input).await),
        None => (format!("unknown or disconnected MCP tool: {tool_name}"), true),
    }
}

impl uclaw_pi_engine::ToolRequestSink for RealToolRequestSink {
    fn io_tool_specs(&self) -> Vec<uclaw_pi_engine::IoToolSpec> {
        let Some(state) = self.app.try_state::<AppState>() else {
            return Vec::new();
        };
        let mut specs: Vec<uclaw_pi_engine::IoToolSpec> = PI_SKILL_TOOLS
            .iter()
            .filter_map(|name| build_skill_tool(name, &state, &self.app))
            .map(|t| spec_from_tool(t.as_ref()))
            .collect();
        // Bridge connected MCP server tools (gbrain, playwright, …) so the pi
        // agent can call them like legacy — without this it only ever sees the 3
        // skill tools. Best-effort sync snapshot via `try_read`: if a write is in
        // progress (rare: connect/reconnect), this session starts without MCP
        // tools and the next session picks them up. `create_tool_proxies` honours
        // each server's `tool_allowlist` (same filtering as the legacy registry).
        if let Ok(mgr) = state.mcp_manager.try_read() {
            for proxy in crate::mcp::McpManager::create_tool_proxies(&state.mcp_manager, &mgr) {
                specs.push(spec_from_tool(&proxy));
            }
        }
        specs
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
            // Capture the tool-input summary before `input` is moved into the
            // executor below; mirrors the legacy ToolDispatcher InfraService publish.
            let input_summary = crate::agent::dispatcher::truncate_utf8(
                &serde_json::to_string(&input).unwrap_or_default(),
                500,
            );
            // Route MCP tools (`mcp__server__tool`) to the bridged MCP executor;
            // everything else is a uClaw skill tool.
            let (text, is_error) = if crate::mcp::parse_mcp_tool_name(&tool_name).is_some() {
                run_mcp_tool(&app, &tool_name, input).await
            } else {
                run_skill_tool(&app, &tool_name, input).await
            };
            // 写 seam: feed the pi tool execution to InfraService — the GeneCandidate
            // pool subscribes to InfraEventType::ToolExecuted, so this re-feeds gene
            // distillation + capsule fitness on the pi path. Fire-and-forget; the
            // metadata shape mirrors the legacy ToolDispatcher publish (duration_ms
            // is unavailable on the pi executor → 0).
            if let Some(state) = app.try_state::<crate::app::AppState>() {
                let infra = std::sync::Arc::clone(&state.infra_service);
                let tn = tool_name.clone();
                let success = !is_error;
                let output_summary = crate::agent::dispatcher::truncate_utf8(&text, 500);
                tauri::async_runtime::spawn(async move {
                    infra
                        .publish_tool_executed(
                            "local",
                            &tn,
                            &output_summary,
                            serde_json::json!({
                                "tool_name": tn,
                                "success": success,
                                "duration_ms": 0,
                                "tool_input": input_summary,
                            }),
                        )
                        .await;
                });
            }
            match engine.and_then(|w| w.upgrade()) {
                Some(engine) => {
                    let sent = engine.send(uclaw_pi_engine::EngineCmd::ToolResult {
                        request_id: request_id.clone(),
                        text,
                        is_error,
                    });
                    if !sent {
                        // Channel full (1024-deep) or engine gone: the awaiting tool
                        // fail-closes to a timeout. Warn so the drop is diagnosable.
                        tracing::warn!(
                            request_id,
                            tool_name,
                            "pi tool result dropped: engine command channel full or closed"
                        );
                    }
                }
                None => tracing::warn!(
                    request_id,
                    tool_name,
                    "pi tool result dropped: engine handle not attached"
                ),
            }
        });
    }
}

/// Runtime override for the agent engine backend, set from Settings (the
/// `set_pi_engine_enabled` command) and seeded once at startup from the persisted
/// `pi_engine_enabled` setting. `0` = unset (fall back to the `UCLAW_PI_ENGINE`
/// env var), `1` = ON (PiEngine), `2` = OFF (legacy loop).
static PI_ENGINE_OVERRIDE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// [R1 Done-when#3] Whether the agent chat commands (`send_message`/`stop_agent`)
/// route through `PiEngine`.
///
/// Read per send (a conversation boundary), so flipping the Settings toggle takes
/// effect on the next message, never mid-stream. Precedence: the Settings override
/// (persisted) wins; absent that, the `UCLAW_PI_ENGINE` env var (the R1→R2 migration
/// default). The legacy path stays the default until R2 lands, then we flip the seed.
#[must_use]
pub fn pi_engine_enabled() -> bool {
    match PI_ENGINE_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => std::env::var_os("UCLAW_PI_ENGINE").is_some(),
    }
}

/// Apply the engine-backend choice at runtime (called by `set_pi_engine_enabled`
/// and the startup seed). `Some(true|false)` forces the backend; `None` clears the
/// override so `pi_engine_enabled()` falls back to the env var.
pub fn set_pi_engine_override(enabled: Option<bool>) {
    PI_ENGINE_OVERRIDE.store(
        match enabled {
            Some(true) => 1,
            Some(false) => 2,
            None => 0,
        },
        std::sync::atomic::Ordering::Relaxed,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_engine_override_forces_backend() {
        set_pi_engine_override(Some(true));
        assert!(pi_engine_enabled(), "override ON must force pi");
        set_pi_engine_override(Some(false));
        assert!(!pi_engine_enabled(), "override OFF must force legacy");
        // Restore to unset so the global doesn't leak to other code/tests.
        set_pi_engine_override(None);
    }

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
    fn request_routing_classifies_mcp_vs_skill_tools() {
        // request() dispatches on parse_mcp_tool_name: `mcp__server__tool` names
        // go to run_mcp_tool (the bridged MCP executor), everything else to
        // run_skill_tool. Pin that contract so a rename can't silently re-route.
        assert!(crate::mcp::parse_mcp_tool_name("mcp__gbrain__get_page").is_some());
        assert!(crate::mcp::parse_mcp_tool_name("mcp__playwright__browser_click").is_some());
        assert!(crate::mcp::parse_mcp_tool_name("skill_search").is_none());
        assert!(crate::mcp::parse_mcp_tool_name("load_skill").is_none());
        assert!(crate::mcp::parse_mcp_tool_name("skill_marketplace_search").is_none());
    }

    #[test]
    fn spec_from_tool_maps_metadata() {
        use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
        struct FakeTool;
        #[async_trait::async_trait]
        impl Tool for FakeTool {
            fn name(&self) -> &str {
                "skill_search"
            }
            fn description(&self) -> &str {
                "search skills"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object", "properties": {"query": {"type": "string"}}})
            }
            async fn execute(
                &self,
                _p: serde_json::Value,
            ) -> Result<ToolOutput, ToolError> {
                Ok(ToolOutput::new(serde_json::json!({}), 0))
            }
        }
        let spec = spec_from_tool(&FakeTool);
        assert_eq!(spec.name, "skill_search");
        assert_eq!(spec.label, "skill_search");
        assert_eq!(spec.description, "search skills");
        assert_eq!(spec.parameters["properties"]["query"]["type"], "string");
    }
}
