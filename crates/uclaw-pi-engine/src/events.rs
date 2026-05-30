//! Frontend event contract: the event **names** uClaw's `useGlobalAgentListeners`
//! binds with bare `listen()`, plus the [`EventSink`] abstraction the engine
//! emits through.

use serde_json::Value;

/// The streaming event names the message-view main loop depends on (§3.3).
///
/// These MUST stay byte-identical to the strings in
/// `ui/src/hooks/useGlobalAgentListeners.ts` — the frontend is the contract.
pub mod event {
    /// `{conversationId, delta, seq}` — assistant text delta.
    pub const STREAM_CHUNK: &str = "chat:stream-chunk";
    /// `{conversationId, delta, seq}` — thinking/reasoning delta.
    pub const STREAM_REASONING: &str = "chat:stream-reasoning";
    /// `{conversationId, activity:{type, toolName, toolCallId, …}}`.
    pub const STREAM_TOOL_ACTIVITY: &str = "chat:stream-tool-activity";
    /// `{conversationId, text, truncated}` — final assistant text for the turn.
    pub const STREAM_COMPLETE: &str = "chat:stream-complete";
    /// `{conversationId, error}` — provider/agent error or user abort.
    pub const STREAM_ERROR: &str = "chat:stream-error";
}

/// Abstracts Tauri's `AppHandle::emit` so the engine/ACL stays testable and
/// independent of Tauri. `src-tauri` provides the real implementation (a thin
/// wrapper that calls `app.emit(event, payload)`); tests use a recording sink.
pub trait EventSink: Send + Sync + 'static {
    /// Emit a frontend event with the given name and JSON payload.
    fn emit(&self, event: &str, payload: Value);
}
