//! The **Engine Actor**: a dedicated OS thread running pi's `asupersync` runtime,
//! driven by [`EngineCmd`]s from the Tauri/tokio side. Data only crosses the
//! boundary — never a future.
//!
//! ## Concurrency (F6)
//! The actor loop awaits commands on an `asupersync` mpsc channel and **spawns**
//! each `Prompt` as its own task (`RuntimeHandle::spawn`), so:
//! - multiple conversations (tabs) stream concurrently, and
//! - the loop keeps reading commands while prompts run — so `Stop` can abort an
//!   in-flight prompt via its stored [`AbortHandle`].
//!
//! Per-session a single [`AgentSessionHandle`] sits behind an async `Mutex`, so
//! two prompts on the *same* conversation serialize (you cannot prompt a busy
//! session) while *different* conversations proceed in parallel.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::thread::JoinHandle;

use asupersync::channel::mpsc;
use asupersync::runtime::{reactor::create_reactor, RuntimeBuilder, RuntimeHandle};
use asupersync::sync::Mutex as AMutex;
use pi::agent_cx::AgentCx;
use pi::sdk::{create_agent_session, AbortHandle, AgentEvent, AgentSessionHandle, SessionOptions};

use crate::acl::{demux, Acl};
use crate::events::{event, EventSink};

/// Commands from the Tauri/tokio side into the engine thread.
#[derive(Debug)]
pub enum EngineCmd {
    /// Drive one user prompt on `conv_id`'s session (lazily created).
    Prompt { conv_id: String, input: String },
    /// Abort `conv_id`'s in-flight run (via its stored [`AbortHandle`]).
    Stop { conv_id: String },
    /// Forget `conv_id`'s in-memory session handle.
    Drop { conv_id: String },
}

/// Base session configuration. Per F2 (reversed), pi owns persistence, so
/// `no_session` is `false` and `session_dir` points under the if2pi namespace
/// (`~/.uclaw/if2pi/agent/sessions`).
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

/// Per-conversation engine state, held on the engine thread and shared with the
/// spawned prompt task by `Arc`.
struct SessionEntry {
    handle: Arc<AMutex<AgentSessionHandle>>,
    /// The current run's abort handle (if any). `Stop` takes + fires it.
    abort: Arc<AMutex<Option<AbortHandle>>>,
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
        let (cmd_tx, cmd_rx) = mpsc::channel::<EngineCmd>(1024);
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
    /// Returns `false` if the channel is full or the engine is gone.
    pub fn send(&self, cmd: EngineCmd) -> bool {
        self.cmd_tx.try_send(cmd).is_ok()
    }
}

/// Engine-thread entry: bootstrap pi's asupersync runtime, then run the actor loop.
fn run_engine_thread(cmd_rx: mpsc::Receiver<EngineCmd>, sink: Arc<dyn EventSink>, config: EngineConfig) {
    let reactor = match create_reactor() {
        Ok(r) => r,
        Err(e) => {
            emit_error(&sink, "", format!("engine reactor init failed: {e:?}"));
            return;
        }
    };
    let runtime = match RuntimeBuilder::current_thread().with_reactor(reactor).build() {
        Ok(r) => r,
        Err(e) => {
            emit_error(&sink, "", format!("engine runtime init failed: {e:?}"));
            return;
        }
    };
    let handle = runtime.handle();
    runtime.block_on(actor_loop(cmd_rx, handle, sink, config));
}

async fn actor_loop(
    mut cmd_rx: mpsc::Receiver<EngineCmd>,
    rt: RuntimeHandle,
    sink: Arc<dyn EventSink>,
    config: EngineConfig,
) {
    let cx = AgentCx::for_current_or_request();
    let mut sessions: HashMap<String, SessionEntry> = HashMap::new();

    while let Ok(cmd) = cmd_rx.recv(&cx).await {
        match cmd {
            EngineCmd::Prompt { conv_id, input } => {
                // Ensure the session exists (lazy create on the engine thread).
                if !sessions.contains_key(&conv_id) {
                    match create_agent_session(config.to_session_options()).await {
                        Ok(h) => {
                            sessions.insert(
                                conv_id.clone(),
                                SessionEntry {
                                    handle: Arc::new(AMutex::new(h)),
                                    abort: Arc::new(AMutex::new(None)),
                                },
                            );
                        }
                        Err(e) => {
                            emit_error(&sink, &conv_id, format!("session create failed: {e:?}"));
                            continue;
                        }
                    }
                }
                let entry = sessions.get(&conv_id).expect("session present");
                let handle = Arc::clone(&entry.handle);
                let abort_slot = Arc::clone(&entry.abort);

                // Create + register this run's abort handle before spawning, so a
                // Stop arriving mid-prompt finds it.
                let (abort_h, abort_sig) = AgentSessionHandle::new_abort_handle();
                if let Ok(mut slot) = abort_slot.lock(&cx).await {
                    *slot = Some(abort_h);
                }

                let sink_task = Arc::clone(&sink);
                let cx_task = cx.clone();
                rt.spawn(async move {
                    let acl = StdMutex::new(Acl::new(conv_id.clone()));
                    let sink_cb = Arc::clone(&sink_task);
                    let on_event = move |ev: AgentEvent| {
                        let raw = demux(&ev);
                        if let Ok(mut a) = acl.lock() {
                            if let Some(fe) = a.translate(&raw) {
                                sink_cb.emit(fe.name, fe.payload);
                            }
                        }
                    };

                    match handle.lock(&cx_task).await {
                        Ok(mut guard) => {
                            if let Err(e) = guard.prompt_with_abort(input, abort_sig, on_event).await {
                                emit_error(&sink_task, &conv_id, format!("prompt failed: {e:?}"));
                            }
                        }
                        Err(e) => emit_error(&sink_task, &conv_id, format!("session lock failed: {e:?}")),
                    }
                });
            }
            EngineCmd::Stop { conv_id } => {
                if let Some(entry) = sessions.get(&conv_id) {
                    if let Ok(mut slot) = entry.abort.lock(&cx).await {
                        if let Some(h) = slot.take() {
                            h.abort();
                        }
                    }
                }
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
    struct RecordingSink(StdMutex<Vec<(String, Value)>>);
    impl RecordingSink {
        fn new() -> Arc<Self> {
            Arc::new(Self(StdMutex::new(Vec::new())))
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

    /// End-to-end: a one-turn sequence of REAL pi `AgentEvent`s through
    /// demux → ACL → EventSink, exactly as the spawned prompt callback does.
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
        let acl = StdMutex::new(Acl::new("c1"));
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
