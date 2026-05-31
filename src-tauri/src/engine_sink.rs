//! [R1 接线] Tauri `EventSink` adapter — bridges `uclaw-pi-engine`'s `EventSink`
//! to Tauri's `AppHandle::emit`, so `PiEngine`'s `chat:stream-*` events reach the
//! frontend (`useGlobalAgentListeners`). See docs/R1-wiring-plan.md §3.

use std::sync::Arc;

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

/// [R4 IO 桥 — stub] The uClaw side of the IO-tool bridge seam. The engine calls
/// `request(...)` when pi invokes a wrapped IO tool; the real executor will
/// dispatch to `mcp.rs` / browser / skills (on tokio) and reply via
/// `EngineCmd::ToolResult`. This stub declares **no** IO tools yet (so only pi
/// built-ins + `ExitPlanTool` are active) and logs any request — the seam is
/// connected and ready for the real executor.
pub struct StubToolRequestSink;

impl uclaw_pi_engine::ToolRequestSink for StubToolRequestSink {
    fn io_tool_specs(&self) -> Vec<uclaw_pi_engine::IoToolSpec> {
        Vec::new()
    }

    fn request(&self, request_id: &str, tool_name: &str, _input: &serde_json::Value) {
        tracing::warn!(
            request_id,
            tool_name,
            "PiEngine IO tool requested but the tokio executor is not wired yet (stub)"
        );
    }
}

/// [R1 Done-when#3] Whether the agent chat commands (`send_message`/`stop_agent`)
/// route through `PiEngine`.
///
/// Gated by the `UCLAW_PI_ENGINE` env var during the R1→R2 migration: R1 wires
/// the path (this flag exercises it end-to-end); R2 makes pi's streaming render
/// 1:1 with the legacy backend ("不做消息渲染正确性,那是 R2"). The legacy path
/// stays the default until R2 lands, then we flip the default here.
#[must_use]
pub fn pi_engine_enabled() -> bool {
    std::env::var_os("UCLAW_PI_ENGINE").is_some()
}
