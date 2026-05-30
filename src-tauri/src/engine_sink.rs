//! [R1 接线] Tauri `EventSink` adapter — bridges `uclaw-pi-engine`'s `EventSink`
//! to Tauri's `AppHandle::emit`, so `PiEngine`'s `chat:stream-*` events reach the
//! frontend (`useGlobalAgentListeners`). See docs/R1-wiring-plan.md §3.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use uclaw_pi_engine::EventSink;

/// Wraps the Tauri `AppHandle` and forwards engine events to the frontend.
pub struct TauriEventSink {
    app: AppHandle,
}

impl TauriEventSink {
    #[must_use]
    pub fn new(app: AppHandle) -> Arc<dyn EventSink> {
        Arc::new(Self { app })
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
        let Some(state) = self.app.try_state::<crate::app::AppState>() else {
            return;
        };
        let Ok(conn) = state.db.lock() else { return };
        let id = uuid::Uuid::new_v4().to_string();
        if let Err(e) = crate::engine_persist::persist_chat_text_message(
            &conn, &id, conv, "assistant", text, None,
        ) {
            tracing::warn!("PiEngine assistant persist failed: {e}");
        }
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
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
