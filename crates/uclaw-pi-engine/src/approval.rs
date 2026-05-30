//! [R3 交互] Per-request interaction registry — the pending + oneshot machinery
//! that backfills uClaw's approval / ask_user / exit_plan dialogs onto pi's
//! [`pi::agent::ToolApprovalHandler`].
//!
//! Mirrors pi's own `acp.rs` reference: **register** a oneshot keyed by request
//! id, **emit** the request to the frontend, **await** the reply with a timeout,
//! and RAII-clean the pending slot on every exit path (resolve / cancel / drop /
//! timeout) so there is no leak and no deadlock under concurrency.
//!
//! Ordering matters: the ticket is registered **synchronously** before the
//! request is emitted, so a `respond_*` arriving the instant the dialog appears
//! always finds its waiter.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use asupersync::channel::oneshot;
use asupersync::time::{timeout, wall_now};
use pi::agent::{ToolApprovalDecision, ToolApprovalHandler, ToolApprovalRequest};
use pi::agent_cx::AgentCx;
use serde_json::json;

use crate::events::{event, EventSink};

type PendingMap = Arc<StdMutex<HashMap<String, oneshot::Sender<ToolApprovalDecision>>>>;

/// RAII: drop the pending slot on every exit path (resolve / cancel / timeout),
/// so the map never leaks a waiter even if the future is cancelled mid-await.
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

/// A registered pending request. Hold it across the emit, then `.await` it.
/// Owning the [`PendingGuard`] means the slot is cleaned even if this ticket is
/// dropped without awaiting (caller cancelled).
pub struct PendingTicket {
    rx: oneshot::Receiver<ToolApprovalDecision>,
    _guard: PendingGuard,
    timeout: Duration,
}

impl PendingTicket {
    /// Await the decision with the registry's timeout. Any failure path
    /// (closed channel, timeout) denies fail-closed — a tool never runs without
    /// an explicit allow.
    pub async fn await_decision(mut self) -> ToolApprovalDecision {
        let cx = AgentCx::for_current_or_request();
        match timeout(wall_now(), self.timeout, Box::pin(self.rx.recv(cx.cx()))).await {
            Ok(Ok(decision)) => decision,
            Ok(Err(_)) => ToolApprovalDecision::deny("approval channel closed"),
            Err(_) => ToolApprovalDecision::deny("approval request timed out"),
        }
    }
}

/// Shared registry between the pi `ToolApprovalHandler` (asupersync, awaiting)
/// and the engine command loop (which resolves via [`ApprovalRegistry::respond`]
/// on `EngineCmd::Respond`). Cheap to clone (all `Arc`).
#[derive(Clone)]
pub struct ApprovalRegistry {
    pending: PendingMap,
    counter: Arc<AtomicU64>,
    timeout: Duration,
    /// Held so `respond` can `oneshot::Sender::send` (asupersync needs a `&Cx` to
    /// wake the awaiting handler). Captured on the engine thread.
    cx: AgentCx,
}

impl ApprovalRegistry {
    #[must_use]
    pub fn new(cx: AgentCx, timeout: Duration) -> Self {
        Self {
            pending: Arc::new(StdMutex::new(HashMap::new())),
            counter: Arc::new(AtomicU64::new(0)),
            timeout,
            cx,
        }
    }

    /// Synchronously register a oneshot for `request_id` and return its ticket.
    /// Call this **before** emitting the request so a fast `respond` always lands.
    #[must_use]
    pub fn register(&self, request_id: String) -> PendingTicket {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut g) = self.pending.lock() {
            g.insert(request_id.clone(), tx);
        }
        PendingTicket {
            rx,
            _guard: PendingGuard {
                pending: Arc::clone(&self.pending),
                key: request_id,
            },
            timeout: self.timeout,
        }
    }

    /// Resolve a pending request (from the engine loop on `EngineCmd::Respond`).
    /// Returns `true` iff a waiter was found and notified.
    pub fn respond(&self, request_id: &str, decision: ToolApprovalDecision) -> bool {
        let tx = self
            .pending
            .lock()
            .ok()
            .and_then(|mut g| g.remove(request_id));
        match tx {
            Some(tx) => tx.send(self.cx.cx(), decision).is_ok(),
            None => false,
        }
    }

    /// A stable request id: the `tool_call_id` when present (round-trips through
    /// the frontend dialog), else a counter (ask_user / exit_plan have none).
    #[must_use]
    pub fn next_request_id(&self, tool_call_id: &str) -> String {
        if tool_call_id.is_empty() {
            format!("ix-{}", self.counter.fetch_add(1, Ordering::SeqCst))
        } else {
            tool_call_id.to_owned()
        }
    }

    /// Number of in-flight requests (diagnostics / tests).
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.lock().map(|g| g.len()).unwrap_or(0)
    }
}

/// Build pi's [`ToolApprovalHandler`] from a registry + sink: on each tool, emit
/// `agent:need_approval` to the frontend and await the user's decision. Register
/// happens **before** emit so the `approve_tool_call` reply always finds it.
#[must_use]
pub fn make_approval_handler(
    registry: ApprovalRegistry,
    sink: Arc<dyn EventSink>,
) -> ToolApprovalHandler {
    Arc::new(move |req: ToolApprovalRequest| {
        let registry = registry.clone();
        let sink = Arc::clone(&sink);
        Box::pin(async move {
            let request_id = registry.next_request_id(&req.tool_call_id);
            let ticket = registry.register(request_id.clone());
            sink.emit(
                event::NEED_APPROVAL,
                json!({
                    "requestId": request_id,
                    "toolCallId": req.tool_call_id,
                    "toolName": req.tool_name,
                    "arguments": req.arguments,
                }),
            );
            ticket.await_decision().await
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use asupersync::runtime::RuntimeBuilder;

    // ── synchronous oneshot/pending mechanics (no runtime needed) ────────────

    #[test]
    fn respond_finds_registered_waiter_then_slot_is_gone() {
        let reg = ApprovalRegistry::new(AgentCx::for_testing(), Duration::from_secs(5));
        let _ticket = reg.register("call-1".into());
        assert_eq!(reg.pending_len(), 1);
        // First respond finds + notifies the waiter.
        assert!(reg.respond("call-1", ToolApprovalDecision::Allow));
        // The slot is consumed — a second respond (or an unknown id) is a no-op.
        assert!(!reg.respond("call-1", ToolApprovalDecision::Allow));
        assert!(!reg.respond("never", ToolApprovalDecision::Allow));
    }

    #[test]
    fn concurrent_requests_resolve_independently() {
        let reg = ApprovalRegistry::new(AgentCx::for_testing(), Duration::from_secs(5));
        let _a = reg.register("a".into());
        let _b = reg.register("b".into());
        let _c = reg.register("c".into());
        assert_eq!(reg.pending_len(), 3);
        assert!(reg.respond("b", ToolApprovalDecision::deny("no")));
        assert_eq!(reg.pending_len(), 2);
        assert!(reg.respond("a", ToolApprovalDecision::Allow));
        assert!(reg.respond("c", ToolApprovalDecision::Allow));
        assert_eq!(reg.pending_len(), 0);
    }

    #[test]
    fn dropping_a_ticket_cancels_and_cleans_the_slot() {
        let reg = ApprovalRegistry::new(AgentCx::for_testing(), Duration::from_secs(5));
        {
            let _ticket = reg.register("x".into());
            assert_eq!(reg.pending_len(), 1);
        } // ticket (and its PendingGuard) dropped here → slot removed
        assert_eq!(reg.pending_len(), 0);
        assert!(!reg.respond("x", ToolApprovalDecision::Allow));
    }

    #[test]
    fn distinct_request_ids_for_toolless_requests() {
        let reg = ApprovalRegistry::new(AgentCx::for_testing(), Duration::from_secs(5));
        assert_eq!(reg.next_request_id("tc-7"), "tc-7"); // tool id round-trips
        let a = reg.next_request_id("");
        let b = reg.next_request_id("");
        assert_ne!(a, b); // ask_user/exit_plan get unique counter ids
    }

    // ── async await paths (need the asupersync runtime) ──────────────────────

    #[test]
    fn await_times_out_fail_closed_to_deny() {
        let runtime = RuntimeBuilder::current_thread().build().expect("rt");
        runtime.block_on(async {
            let reg = ApprovalRegistry::new(AgentCx::for_testing(), Duration::from_millis(40));
            let ticket = reg.register("slow".into());
            // No respond ever arrives → must time out and DENY (never hang).
            let decision = ticket.await_decision().await;
            assert!(matches!(decision, ToolApprovalDecision::Deny { .. }));
        });
    }

    #[test]
    fn register_then_respond_then_await_resolves_to_allow() {
        let runtime = RuntimeBuilder::current_thread().build().expect("rt");
        runtime.block_on(async {
            let reg = ApprovalRegistry::new(AgentCx::for_testing(), Duration::from_secs(5));
            // Register synchronously, respond, THEN await — the decision is
            // already queued in the oneshot, so the await resolves immediately.
            let ticket = reg.register("call-9".into());
            assert!(reg.respond("call-9", ToolApprovalDecision::Allow));
            let decision = ticket.await_decision().await;
            assert_eq!(decision, ToolApprovalDecision::Allow);
        });
    }
}
