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
use std::time::Duration;

use asupersync::channel::mpsc;
use asupersync::runtime::{reactor::create_reactor, RuntimeBuilder, RuntimeHandle};
use asupersync::sync::Mutex as AMutex;
use pi::agent::{ToolApprovalDecision, ToolApprovalHandler};
use pi::agent_cx::AgentCx;
use pi::sdk::{create_agent_session, AbortHandle, AgentEvent, AgentSessionHandle, SessionOptions};

use crate::acl::{demux, Acl};
use crate::approval::{make_approval_handler, ApprovalRegistry};
use crate::events::{event, EventSink};
use crate::tool_bridge::{tool_output_text, ToolRequestSink, ToolResultRegistry};
use crate::tool_factory::UclawToolFactory;

/// An API key string that never appears in `Debug` output (so it can ride inside
/// a `#[derive(Debug)]` command without leaking into logs).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct RedactedString(pub String);

impl std::fmt::Debug for RedactedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_empty() { "\"\"" } else { "\"<redacted>\"" })
    }
}

/// [R4/F7] Dynamic provider / model / key for new pi sessions, sourced from
/// uClaw's `provider_service` (the Settings → 服务商 tab). Threaded into each new
/// session's `SessionOptions`, so pi uses the user's configured key
/// (`SessionOptions.api_key`) rather than `~/.pi/auth.json`.
#[derive(Clone, Default, PartialEq)]
pub struct ModelConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<RedactedString>,
}

/// Commands from the Tauri/tokio side into the engine thread.
#[derive(Debug)]
pub enum EngineCmd {
    /// [R4/F7] Set the provider/model/api_key for **new** sessions, sourced from
    /// uClaw's `provider_service` (Settings 服务商). Applied at session creation;
    /// existing sessions keep their config (start a new conversation to re-key).
    Configure {
        provider: Option<String>,
        model: Option<String>,
        api_key: Option<RedactedString>,
    },
    /// Drive one user prompt on `conv_id`'s session (lazily created).
    Prompt { conv_id: String, input: String },
    /// Continue `conv_id`'s loop without a new user message (steer/retry flows).
    FollowUp { conv_id: String },
    /// Switch the model for `conv_id`'s session.
    SetModel {
        conv_id: String,
        provider: String,
        model: String,
    },
    /// Abort `conv_id`'s in-flight run (via its stored [`AbortHandle`]).
    Stop { conv_id: String },
    /// Forget `conv_id`'s in-memory session handle.
    Drop { conv_id: String },
    /// [R3 交互] Resolve a pending approval / ask_user / exit_plan request (from
    /// the `respond_*` tauri commands). `allow=false` denies with `reason`.
    Respond {
        request_id: String,
        allow: bool,
        reason: Option<String>,
    },
    /// [R4 IO 桥] Resolve a pending IO-tool execution (from uClaw's tokio executor
    /// after it ran the real MCP/browser/skill tool). `text` becomes the tool's
    /// `ToolOutput` the awaiting `BridgedIoTool::execute()` returns.
    ToolResult {
        request_id: String,
        text: String,
        is_error: bool,
    },
}

/// How a streaming run is driven (shared spawn path for `Prompt`/`FollowUp`).
enum RunKind {
    Prompt(String),
    FollowUp,
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
    /// [R3 Piece E] The active workspace's cwd → pi `working_directory`.
    /// `None` keeps pi's default (process cwd).
    pub working_directory: Option<PathBuf>,
}

impl EngineConfig {
    fn to_session_options(&self) -> SessionOptions {
        SessionOptions {
            provider: self.provider.clone(),
            model: self.model.clone(),
            session_dir: self.session_dir.clone(),
            no_session: self.no_session,
            working_directory: self.working_directory.clone(),
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
    pub fn spawn(
        sink: Arc<dyn EventSink>,
        tool_request_sink: Option<Arc<dyn ToolRequestSink>>,
        config: EngineConfig,
    ) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<EngineCmd>(1024);
        let thread = std::thread::Builder::new()
            .name("pi-engine".into())
            .spawn(move || run_engine_thread(cmd_rx, sink, tool_request_sink, config))
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
fn run_engine_thread(
    cmd_rx: mpsc::Receiver<EngineCmd>,
    sink: Arc<dyn EventSink>,
    tool_request_sink: Option<Arc<dyn ToolRequestSink>>,
    config: EngineConfig,
) {
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
    runtime.block_on(actor_loop(cmd_rx, handle, sink, tool_request_sink, config));
}

async fn actor_loop(
    mut cmd_rx: mpsc::Receiver<EngineCmd>,
    rt: RuntimeHandle,
    sink: Arc<dyn EventSink>,
    tool_request_sink: Option<Arc<dyn ToolRequestSink>>,
    config: EngineConfig,
) {
    let cx = AgentCx::for_current_or_request();
    let mut sessions: HashMap<String, SessionEntry> = HashMap::new();
    // [R3 交互] One approval registry + handler shared across all sessions; each
    // session's `SessionOptions.tool_approval` points at the handler, and
    // `EngineCmd::Respond` resolves the pending request through the same registry.
    let approval = ApprovalRegistry::new(cx.clone(), Duration::from_secs(300));
    let approval_handler = make_approval_handler(approval.clone(), Arc::clone(&sink));
    // [R4 IO 桥] Resolves IO-tool executions; `EngineCmd::ToolResult` (from uClaw's
    // tokio executor) fires it, the awaiting BridgedIoTool returns the output.
    let tool_results = ToolResultRegistry::new(cx.clone(), Duration::from_secs(300));
    // [R4 F5] One UclawToolFactory shared across sessions: pi built-ins verbatim +
    // uClaw tools (ExitPlanTool + BridgedIoTools from the executor's specs). Set on
    // each session's SessionOptions.tool_factory.
    let tool_factory = UclawToolFactory::new(
        approval.clone(),
        Arc::clone(&sink),
        tool_results.clone(),
        tool_request_sink,
    );
    // [R4/F7] Current provider/model/key for new sessions — seeded from EngineConfig,
    // updated by `EngineCmd::Configure` (from uClaw's provider_service / 服务商 tab).
    let mut model_config = ModelConfig {
        provider: config.provider.clone(),
        model: config.model.clone(),
        api_key: None,
    };

    while let Ok(cmd) = cmd_rx.recv(&cx).await {
        match cmd {
            EngineCmd::Configure { provider, model, api_key } => {
                model_config = ModelConfig { provider, model, api_key };
            }
            EngineCmd::Prompt { conv_id, input } => {
                start_run(&mut sessions, &rt, &sink, &cx, &config, &model_config, &approval_handler, &tool_factory, conv_id, RunKind::Prompt(input)).await;
            }
            EngineCmd::FollowUp { conv_id } => {
                start_run(&mut sessions, &rt, &sink, &cx, &config, &model_config, &approval_handler, &tool_factory, conv_id, RunKind::FollowUp).await;
            }
            EngineCmd::SetModel { conv_id, provider, model } => {
                if let Some(entry) = sessions.get(&conv_id) {
                    let handle = Arc::clone(&entry.handle);
                    set_model_run(handle, &cx, &sink, &conv_id, &provider, &model).await;
                }
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
            EngineCmd::Respond { request_id, allow, reason } => {
                let decision = if allow {
                    ToolApprovalDecision::Allow
                } else {
                    ToolApprovalDecision::deny(reason.unwrap_or_else(|| "denied by user".into()))
                };
                // No waiter (stale/duplicate respond) is a harmless no-op.
                approval.respond(&request_id, decision);
            }
            EngineCmd::ToolResult { request_id, text, is_error } => {
                // Resolve the awaiting BridgedIoTool::execute() with the executor's
                // output. No waiter (stale/timed-out) is a harmless no-op.
                tool_results.respond(&request_id, tool_output_text(text, is_error));
            }
        }
    }
}

/// Ensure `conv_id`'s session exists, register a fresh abort handle, and spawn
/// the streaming run (prompt or continue) as its own asupersync task.
async fn start_run(
    sessions: &mut HashMap<String, SessionEntry>,
    rt: &RuntimeHandle,
    sink: &Arc<dyn EventSink>,
    cx: &AgentCx,
    config: &EngineConfig,
    model_config: &ModelConfig,
    approval_handler: &ToolApprovalHandler,
    tool_factory: &Arc<UclawToolFactory>,
    conv_id: String,
    kind: RunKind,
) {
    if !sessions.contains_key(&conv_id) {
        // [R3 交互] Inject the global approval gate (sdk hardcodes None) so pi
        // surfaces every tool through uClaw's approval dialog.
        // [R4 F5] Inject the tool factory: pi built-ins verbatim + uClaw tools.
        let mut opts = config.to_session_options();
        // [R4/F7] Apply the user's configured provider/model/key (provider_service
        // / 服务商 tab). pi uses SessionOptions.api_key, not ~/.pi/auth.json.
        if model_config.provider.is_some() {
            opts.provider = model_config.provider.clone();
        }
        if model_config.model.is_some() {
            opts.model = model_config.model.clone();
        }
        if let Some(key) = &model_config.api_key {
            opts.api_key = Some(key.0.clone());
        }
        opts.tool_approval = Some(approval_handler.clone());
        opts.tool_factory = Some(tool_factory.clone());
        match create_agent_session(opts).await {
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
                emit_error(sink, &conv_id, format!("session create failed: {e:?}"));
                return;
            }
        }
    }
    let entry = sessions.get(&conv_id).expect("session present");
    let handle = Arc::clone(&entry.handle);
    let abort_slot = Arc::clone(&entry.abort);

    // Register this run's abort handle before spawning, so a Stop arriving
    // mid-run finds it.
    let (abort_h, abort_sig) = AgentSessionHandle::new_abort_handle();
    if let Ok(mut slot) = abort_slot.lock(cx).await {
        *slot = Some(abort_h);
    }

    let sink_task = Arc::clone(sink);
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
                let res = match kind {
                    RunKind::Prompt(input) => {
                        guard.prompt_with_abort(input, abort_sig, on_event).await
                    }
                    RunKind::FollowUp => guard.continue_turn_with_abort(abort_sig, on_event).await,
                };
                if let Err(e) = res {
                    emit_error(&sink_task, &conv_id, format!("run failed: {e:?}"));
                }
            }
            Err(e) => emit_error(&sink_task, &conv_id, format!("session lock failed: {e:?}")),
        }
    });
}

/// Switch a session's model. Takes the handle by value so the lock guard's
/// temporary cannot outlive it.
async fn set_model_run(
    handle: Arc<AMutex<AgentSessionHandle>>,
    cx: &AgentCx,
    sink: &Arc<dyn EventSink>,
    conv_id: &str,
    provider: &str,
    model: &str,
) {
    match handle.lock(cx).await {
        Ok(mut guard) => {
            if let Err(e) = guard.set_model(provider, model).await {
                emit_error(sink, conv_id, format!("set_model failed: {e:?}"));
            }
        }
        Err(e) => emit_error(sink, conv_id, format!("session lock failed: {e:?}")),
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

    /// [R4/F7] The api_key must NEVER appear in Debug output — not as a bare
    /// RedactedString, nor inside an EngineCmd::Configure (which derives Debug and
    /// could otherwise leak the key into a log line).
    #[test]
    fn api_key_never_leaks_in_debug() {
        let k = RedactedString("sk-super-secret-123".into());
        assert_eq!(format!("{k:?}"), "\"<redacted>\"");
        assert_eq!(format!("{:?}", RedactedString(String::new())), "\"\"");

        let cmd = EngineCmd::Configure {
            provider: Some("anthropic".into()),
            model: Some("claude-x".into()),
            api_key: Some(RedactedString("sk-leak-me".into())),
        };
        let dbg = format!("{cmd:?}");
        assert!(!dbg.contains("sk-leak-me"), "api key leaked: {dbg}");
        assert!(dbg.contains("<redacted>") && dbg.contains("anthropic"));
    }

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
            working_directory: Some(PathBuf::from("/work/ws-1")),
        };
        let opts = cfg.to_session_options();
        assert_eq!(opts.provider.as_deref(), Some("anthropic"));
        assert_eq!(opts.model.as_deref(), Some("claude-x"));
        assert!(!opts.no_session);
        assert_eq!(opts.working_directory.as_deref(), Some(std::path::Path::new("/work/ws-1")));
    }
}
