//! [R1 接线] Tauri `EventSink` adapter — bridges `uclaw-pi-engine`'s `EventSink`
//! to Tauri's `AppHandle::emit`, so `PiEngine`'s `chat:stream-*` events reach the
//! frontend (`useGlobalAgentListeners`). See docs/R1-wiring-plan.md §3.

use std::sync::Arc;

use tauri::{AppHandle, Emitter};
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

impl EventSink for TauriEventSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
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
