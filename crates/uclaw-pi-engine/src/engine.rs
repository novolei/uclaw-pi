//! The **Engine Actor**: a dedicated OS thread running pi's `asupersync` runtime,
//! driven by [`EngineCmd`]s sent from the Tauri/tokio side over a plain
//! `std::sync::mpsc` channel (data only — never a future crosses the boundary).
//!
//! ## R1 status — SERIAL actor
//! This slice processes commands **one at a time**: a `Prompt` is fully awaited
//! (streaming events out through the [`EventSink`] as they arrive) before the
//! next command is read. This wires the complete command → pi → stream → ACL →
//! emit path end-to-end and compiles on the verified spike APIs.
//!
//! `Stop` cannot interrupt an in-flight prompt in the serial model. The next
//! slice upgrades the loop to spawn each prompt as an `asupersync` task
//! (`RuntimeHandle::spawn`) and store its [`pi::sdk::AbortHandle`] so `Stop`
//! (and per-tab concurrency, F6) work. The public surface ([`PiEngine`],
//! [`EngineCmd`]) is unchanged by that upgrade.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use pi::sdk::{create_agent_session, AgentEvent, SessionOptions};

use crate::acl::{demux, Acl};
use crate::events::{event, EventSink};

/// Commands from the Tauri/tokio side into the engine thread.
#[derive(Debug)]
pub enum EngineCmd {
    /// Drive one user prompt on `conv_id`'s session (lazily created).
    Prompt { conv_id: String, input: String },
    /// Request cancellation of `conv_id`'s current run (effective once the
    /// concurrent slice lands; serial loop records intent only).
    Stop { conv_id: String },
    /// Forget `conv_id`'s in-memory session handle.
    Drop { conv_id: String },
}

/// Base session configuration. Per F2 (reversed), pi owns persistence, so
/// `no_session` is `false` and `session_dir` points under the if2pi namespace
/// (`~/.uclaw/if2pi/agent/sessions`) — set by the caller.
#[derive(Clone, Debug, Default)]
pub struct EngineConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub session_dir: Option<PathBuf>,
    /// `true` = ephemeral (no disk). Default `false`: pi owns sessions (F2).
    pub no_session: bool,
}

impl EngineConfig {
    fn to_session_options(&self) -> SessionOptions {
        SessionOptions {
            provider: self.provider.clone(),
            model: self.model.clone(),
            session_dir: self.session_dir.clone(),
            no_session: self.no_session,
            ..Default::default()
        }
    }
}

/// Tokio-side handle to the engine. Holds the command sender and keeps the
/// engine thread alive.
pub struct PiEngine {
    cmd_tx: mpsc::Sender<EngineCmd>,
    _thread: JoinHandle<()>,
}

impl PiEngine {
    /// Spawn the dedicated asupersync engine thread.
    #[must_use]
    pub fn spawn(sink: Arc<dyn EventSink>, config: EngineConfig) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<EngineCmd>();
        let thread = std::thread::Builder::new()
            .name("pi-engine".into())
            .spawn(move || run_engine_thread(cmd_rx, sink, config))
            .expect("spawn pi-engine thread");
        Self {
            cmd_tx,
            _thread: thread,
        }
    }

    /// Send a command to the engine (sync; callable from any thread incl. tokio).
    /// Returns `false` if the engine thread has gone away.
    pub fn send(&self, cmd: EngineCmd) -> bool {
        self.cmd_tx.send(cmd).is_ok()
    }
}

/// Engine-thread entry: bootstrap pi's asupersync runtime, then run the actor
/// loop. (pi uses asupersync, not tokio — see spike / examples/basic_sdk.rs.)
fn run_engine_thread(cmd_rx: mpsc::Receiver<EngineCmd>, sink: Arc<dyn EventSink>, config: EngineConfig) {
    let reactor = match asupersync::runtime::reactor::create_reactor() {
        Ok(r) => r,
        Err(e) => {
            sink.emit(
                event::STREAM_ERROR,
                serde_json::json!({ "conversationId": "", "error": format!("engine reactor init failed: {e:?}") }),
            );
            return;
        }
    };
    let runtime = match asupersync::runtime::RuntimeBuilder::current_thread()
        .with_reactor(reactor)
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            sink.emit(
                event::STREAM_ERROR,
                serde_json::json!({ "conversationId": "", "error": format!("engine runtime init failed: {e:?}") }),
            );
            return;
        }
    };
    runtime.block_on(actor_loop(cmd_rx, sink, config));
}

/// Serial command loop. Blocking `recv()` between commands is fine: nothing else
/// runs on this runtime while idle, and each prompt's `.await` drives streaming.
async fn actor_loop(cmd_rx: mpsc::Receiver<EngineCmd>, sink: Arc<dyn EventSink>, config: EngineConfig) {
    let mut sessions: HashMap<String, pi::sdk::AgentSessionHandle> = HashMap::new();

    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            EngineCmd::Prompt { conv_id, input } => {
                if !sessions.contains_key(&conv_id) {
                    match create_agent_session(config.to_session_options()).await {
                        Ok(h) => {
                            sessions.insert(conv_id.clone(), h);
                        }
                        Err(e) => {
                            emit_error(&sink, &conv_id, format!("session create failed: {e:?}"));
                            continue;
                        }
                    }
                }
                let handle = sessions.get_mut(&conv_id).expect("session present");

                // The ACL lives behind a Mutex so the (Fn + Send + Sync) callback
                // can mutate it on each streamed event. Sink is cloned in.
                let acl = Mutex::new(Acl::new(conv_id.clone()));
                let sink_cb = Arc::clone(&sink);
                let on_event = move |ev: AgentEvent| {
                    let raw = demux(&ev);
                    if let Ok(mut a) = acl.lock() {
                        if let Some(fe) = a.translate(&raw) {
                            sink_cb.emit(fe.name, fe.payload);
                        }
                    }
                };

                if let Err(e) = handle.prompt(input, on_event).await {
                    emit_error(&sink, &conv_id, format!("prompt failed: {e:?}"));
                }
            }
            EngineCmd::Stop { conv_id } => {
                // Serial loop: a prompt is never in flight here. Recorded for the
                // concurrent slice (which stores an AbortHandle per session).
                let _ = conv_id;
            }
            EngineCmd::Drop { conv_id } => {
                sessions.remove(&conv_id);
            }
        }
    }
}

fn emit_error(sink: &Arc<dyn EventSink>, conv_id: &str, error: String) {
    sink.emit(
        event::STREAM_ERROR,
        serde_json::json!({ "conversationId": conv_id, "error": error }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Test EventSink that records every (name, payload) emitted.
    struct RecordingSink(Mutex<Vec<(String, Value)>>);
    impl RecordingSink {
        fn new() -> Arc<Self> {
            Arc::new(Self(Mutex::new(Vec::new())))
        }
        fn recorded(&self) -> Vec<(String, Value)> {
            self.0.lock().unwrap().clone()
        }
    }
    impl EventSink for RecordingSink {
        fn emit(&self, event: &str, payload: Value) {
            self.0.lock().unwrap().push((event.to_string(), payload));
        }
    }

    /// End-to-end: build a one-turn sequence of REAL pi `AgentEvent`s (the same
    /// shape a live prompt streams) and run it through demux → ACL → EventSink,
    /// exactly as `actor_loop`'s callback does. Proves demux handles real events
    /// and the emit path produces the frontend contract.
    #[test]
    fn demux_acl_emit_pipeline_produces_frontend_events() {
        use pi::model::{AssistantMessage, AssistantMessageEvent, Message};

        let sid: Arc<str> = Arc::from("sess-test");
        let am = Arc::new(AssistantMessage::default());
        let events: Vec<AgentEvent> = vec![
            AgentEvent::AgentStart {
                session_id: sid.clone(),
            },
            AgentEvent::MessageUpdate {
                message: Message::Assistant(am.clone()),
                assistant_message_event: AssistantMessageEvent::TextDelta {
                    content_index: 0,
                    delta: "pong".into(),
                    partial: am.clone(),
                },
            },
            AgentEvent::MessageUpdate {
                message: Message::Assistant(am.clone()),
                assistant_message_event: AssistantMessageEvent::TextDelta {
                    content_index: 0,
                    delta: " from the void".into(),
                    partial: am.clone(),
                },
            },
            AgentEvent::TurnEnd {
                session_id: sid.clone(),
                turn_index: 0,
                message: Message::Assistant(am.clone()),
                tool_results: Vec::new(),
                latency_breakdown: None,
            },
            AgentEvent::AgentEnd {
                session_id: sid.clone(),
                messages: vec![Message::Assistant(am.clone())],
                error: None,
            },
        ];

        let sink = RecordingSink::new();
        // Mirror actor_loop's per-prompt callback.
        let acl = Mutex::new(Acl::new("c1"));
        let sink_cb: Arc<dyn EventSink> = sink.clone();
        let on_event = move |ev: AgentEvent| {
            let raw = demux(&ev);
            if let Ok(mut a) = acl.lock() {
                if let Some(fe) = a.translate(&raw) {
                    sink_cb.emit(fe.name, fe.payload);
                }
            }
        };
        for ev in events {
            on_event(ev);
        }

        let rec = sink.recorded();
        assert_eq!(rec.len(), 3, "expected 2 chunks + 1 complete, got {rec:?}");
        assert_eq!(rec[0].0, event::STREAM_CHUNK);
        assert_eq!(rec[0].1["delta"], "pong");
        assert_eq!(rec[0].1["seq"], 0);
        assert_eq!(rec[1].0, event::STREAM_CHUNK);
        assert_eq!(rec[1].1["seq"], 1);
        assert_eq!(rec[2].0, event::STREAM_COMPLETE);
        assert_eq!(rec[2].1["text"], "pong from the void");
        assert_eq!(rec[2].1["truncated"], false);
    }

    #[test]
    fn engine_config_builds_session_options() {
        let cfg = EngineConfig {
            provider: Some("anthropic".into()),
            model: Some("claude-x".into()),
            session_dir: Some(PathBuf::from("/tmp/if2pi/sessions")),
            no_session: false,
        };
        let opts = cfg.to_session_options();
        assert_eq!(opts.provider.as_deref(), Some("anthropic"));
        assert_eq!(opts.model.as_deref(), Some("claude-x"));
        assert!(!opts.no_session);
    }
}
