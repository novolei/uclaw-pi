//! R0 探针:验证 pi 作为库依赖能否在 **stable 1.85** 编译,并演示最小 **Engine Actor**——
//! pi 的 `asupersync` runtime 跑在一条专用 OS 线程上,事件经 `std::mpsc` 桥回主线程
//! (真实 uClaw 里主线程是 Tauri/tokio 侧;这里用 std::mpsc 代替以保持探针**零 tokio 依赖**,
//!  从而硬性证明"跨边界只传数据、绝不让 tokio poll 一个 pi future")。
//!
//! 三个验证目标(对应 R0 Done-when 1/2/3):
//!   1) 能否在 stable 1.85 编译 pi 库(核心 go/no-go 闸门 F3)。
//!   2) 专用 asupersync 线程 ⇄ 主线程的事件桥成立:捕获 AgentStart → MessageUpdate(TextDelta)
//!      → TurnEnd/AgentEnd 的有序序列。
//!   3) §3.3 翻译 seam:TextDelta → `chat:stream-chunk{conversationId,delta,seq}`、
//!      TurnEnd/AgentEnd → `chat:stream-complete{conversationId,text,truncated}`,seq 由 ACL 单调自增。
//!
//! 驱动来源二选一:
//!   - 若环境有可解析的 API key → 跑一次**真实** prompt,回调里 demux 真实流式 AgentEvent。
//!   - 否则(本机无凭证)→ **确定性注入一段用真实 pi `AgentEvent` 类型构造的事件序列**,
//!     走**同一条** demux + 桥 + ACL 代码路径(诚实标注:这不是 live 模型回复,而是事件注入)。
//!   两种来源都只让"数据"跨线程边界,ACL 翻译统一发生在主线程(= tokio 侧的忠实镜像)。

use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use pi::model::{AssistantMessage, AssistantMessageEvent, Message};
use pi::sdk::{create_agent_session, AgentEvent, SessionOptions};
use serde::Serialize;

/// 探针里固定的会话 id(真实 uClaw 用前端传入的 conversationId)。
const CONV_ID: &str = "r0-conv-1";

// ===========================================================================
// 跨 tokio↔asupersync 边界传输的"数据"(永远不是 future)。
// demux 把真实 pi `AgentEvent` 投影成这些纯 owned 值,再经 std::mpsc 过边界。
// ===========================================================================
// 部分字段(session_id/tool_call_id/is_error)是为忠实 demux 真实事件而保留,
// 本探针 ACL 演示未全部读取——allow dead_code 以保持构建零警告。
#[allow(dead_code)]
#[derive(Debug, Clone)]
enum RawEvt {
    AgentStart { session_id: String },
    TextDelta { delta: String },
    ThinkingDelta { delta: String },
    ToolStart { tool_name: String, tool_call_id: String },
    ToolEnd { tool_name: String, tool_call_id: String, is_error: bool },
    TurnEnd,
    AgentEnd { error: Option<String> },
    Other { name: String },
}

/// 引擎线程 → 主线程的桥消息。
enum Bridge {
    Status(String),
    Raw(RawEvt),
    Eos,
}

/// 把**真实** pi `AgentEvent` demux 成边界数据。live 与注入两条路径共用此函数,
/// 因此"能处理真实 AgentEvent"是编译期被检查过的(而非猜测的形状)。
fn demux(ev: &AgentEvent) -> RawEvt {
    match ev {
        AgentEvent::AgentStart { session_id } => RawEvt::AgentStart {
            session_id: session_id.to_string(),
        },
        AgentEvent::MessageUpdate {
            assistant_message_event,
            ..
        } => match assistant_message_event {
            AssistantMessageEvent::TextDelta { delta, .. } => RawEvt::TextDelta {
                delta: delta.clone(),
            },
            AssistantMessageEvent::ThinkingDelta { delta, .. } => RawEvt::ThinkingDelta {
                delta: delta.clone(),
            },
            other => RawEvt::Other {
                name: format!("MessageUpdate:{}", amev_tag(other)),
            },
        },
        AgentEvent::ToolExecutionStart {
            tool_name,
            tool_call_id,
            ..
        } => RawEvt::ToolStart {
            tool_name: tool_name.clone(),
            tool_call_id: tool_call_id.clone(),
        },
        AgentEvent::ToolExecutionEnd {
            tool_name,
            tool_call_id,
            is_error,
            ..
        } => RawEvt::ToolEnd {
            tool_name: tool_name.clone(),
            tool_call_id: tool_call_id.clone(),
            is_error: *is_error,
        },
        AgentEvent::TurnEnd { .. } => RawEvt::TurnEnd,
        AgentEvent::AgentEnd { error, .. } => RawEvt::AgentEnd {
            error: error.clone(),
        },
        other => RawEvt::Other {
            name: agentevent_tag(other),
        },
    }
}

/// AssistantMessageEvent 的短标签(仅用于 Other 的可读日志)。
fn amev_tag(e: &AssistantMessageEvent) -> &'static str {
    match e {
        AssistantMessageEvent::Start { .. } => "start",
        AssistantMessageEvent::TextStart { .. } => "text_start",
        AssistantMessageEvent::TextDelta { .. } => "text_delta",
        AssistantMessageEvent::TextEnd { .. } => "text_end",
        AssistantMessageEvent::ThinkingStart { .. } => "thinking_start",
        AssistantMessageEvent::ThinkingDelta { .. } => "thinking_delta",
        AssistantMessageEvent::ThinkingEnd { .. } => "thinking_end",
        AssistantMessageEvent::ToolCallStart { .. } => "toolcall_start",
        AssistantMessageEvent::ToolCallDelta { .. } => "toolcall_delta",
        AssistantMessageEvent::ToolCallEnd { .. } => "toolcall_end",
        _ => "other",
    }
}

/// AgentEvent 的短标签(用 serde 的 `tag = "type"` snake_case 判别名,避免逐变体硬编码)。
fn agentevent_tag(e: &AgentEvent) -> String {
    serde_json::to_value(e)
        .ok()
        .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}

/// 有序序列日志里每个事件的可读标签(Done-when #2 的"事件序列")。
fn raw_tag(r: &RawEvt) -> String {
    match r {
        RawEvt::AgentStart { .. } => "AgentStart".into(),
        RawEvt::TextDelta { .. } => "MessageUpdate(TextDelta)".into(),
        RawEvt::ThinkingDelta { .. } => "MessageUpdate(ThinkingDelta)".into(),
        RawEvt::ToolStart { tool_name, .. } => format!("ToolExecutionStart({tool_name})"),
        RawEvt::ToolEnd { tool_name, .. } => format!("ToolExecutionEnd({tool_name})"),
        RawEvt::TurnEnd => "TurnEnd".into(),
        RawEvt::AgentEnd { .. } => "AgentEnd".into(),
        RawEvt::Other { name } => format!("Other({name})"),
    }
}

// ===========================================================================
// §3.3 翻译 seam(ACL)。住在**主线程**(= tokio 侧),只消费边界数据。
// 形状须与 ui/src/lib/chat-types.ts 对齐;seq 由 ACL 维护(pi 不提供)。
// ===========================================================================
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamChunk {
    conversation_id: String,
    delta: String,
    seq: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamComplete {
    conversation_id: String,
    text: String,
    truncated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamError {
    conversation_id: String,
    error: String,
}

/// per-conversation ACL 状态:单调 seq + 累积文本 + 完成标志。
struct Acl {
    conv_id: String,
    seq: u64,
    acc_text: String,
    completed: bool,
}

impl Acl {
    fn new(conv_id: &str) -> Self {
        Self {
            conv_id: conv_id.to_string(),
            seq: 0,
            acc_text: String::new(),
            completed: false,
        }
    }

    /// 把一条边界事件翻译成"前端事件名 + payload(JSON)"。None = 此事件不产出前端事件。
    fn translate(&mut self, raw: &RawEvt) -> Option<String> {
        match raw {
            RawEvt::TextDelta { delta } => {
                self.acc_text.push_str(delta);
                let p = StreamChunk {
                    conversation_id: self.conv_id.clone(),
                    delta: delta.clone(),
                    seq: self.seq,
                };
                self.seq += 1;
                Some(format!(
                    "chat:stream-chunk     {}",
                    serde_json::to_string(&p).unwrap()
                ))
            }
            RawEvt::ThinkingDelta { delta } => {
                let p = StreamChunk {
                    conversation_id: self.conv_id.clone(),
                    delta: delta.clone(),
                    seq: self.seq,
                };
                self.seq += 1;
                Some(format!(
                    "chat:stream-reasoning {}",
                    serde_json::to_string(&p).unwrap()
                ))
            }
            RawEvt::AgentEnd { error: Some(e) } => {
                let p = StreamError {
                    conversation_id: self.conv_id.clone(),
                    error: e.clone(),
                };
                Some(format!(
                    "chat:stream-error     {}",
                    serde_json::to_string(&p).unwrap()
                ))
            }
            // TurnEnd 或 AgentEnd{error:None} 中**最先到达者**产出一次 complete(§3.3 两者皆为来源)。
            RawEvt::TurnEnd | RawEvt::AgentEnd { error: None } if !self.completed => {
                self.completed = true;
                // truncated:本探针无截断;真实 ACL 在超 token 窗口时置 true。
                let p = StreamComplete {
                    conversation_id: self.conv_id.clone(),
                    text: self.acc_text.clone(),
                    truncated: false,
                };
                Some(format!(
                    "chat:stream-complete  {}",
                    serde_json::to_string(&p).unwrap()
                ))
            }
            _ => None,
        }
    }
}

// ===========================================================================
// 引擎线程(asupersync 专用线程)的异步主体。
// ===========================================================================
async fn engine_async(tx: &mpsc::Sender<Bridge>) {
    let provider = std::env::var("PI_PROVIDER").unwrap_or_else(|_| "anthropic".into());
    let model =
        std::env::var("PI_MODEL").unwrap_or_else(|_| "claude-sonnet-4-20250514".into());

    let opts = SessionOptions {
        provider: Some(provider),
        model: Some(model),
        no_session: true,            // F2:pi 无状态
        enabled_tools: Some(vec![]), // 探针关工具,保持轻量
        ..Default::default()
    };

    let has_key =
        std::env::var("ANTHROPIC_API_KEY").is_ok() || std::env::var("PI_API_KEY").is_ok();

    match create_agent_session(opts).await {
        Ok(mut handle) => {
            let (p, m) = handle.model();
            let _ = tx.send(Bridge::Status(format!(
                "SESSION_CREATED provider={p} model={m}"
            )));

            if has_key {
                // ── LIVE:真实 prompt。回调须 Fn+Send+Sync,std Sender 非 Sync,
                //    故回调只往 Arc<Mutex<Vec>> 收集 demux 结果,prompt 返回后再过桥。
                let _ = tx.send(Bridge::Status(
                    "MODE=live:发真实 prompt(回调内 demux 真实流式 AgentEvent)".into(),
                ));
                let buf: Arc<Mutex<Vec<RawEvt>>> = Arc::new(Mutex::new(Vec::new()));
                let buf2 = buf.clone();
                let res = handle
                    .prompt(
                        "Reply with exactly one word: pong",
                        move |ev: AgentEvent| {
                            if let Ok(mut g) = buf2.lock() {
                                g.push(demux(&ev));
                            }
                        },
                    )
                    .await;
                if let Ok(g) = buf.lock() {
                    for r in g.iter() {
                        let _ = tx.send(Bridge::Raw(r.clone()));
                    }
                }
                match res {
                    Ok(_) => {
                        let _ = tx.send(Bridge::Status("PROMPT_OK(live)".into()));
                    }
                    Err(e) => {
                        let s: String = format!("{e:?}").chars().take(160).collect();
                        let _ = tx.send(Bridge::Status(format!("PROMPT_ERR(live): {s}")));
                    }
                }
            } else {
                let _ = tx.send(Bridge::Status(
                    "MODE=inject:本机无凭证,改用确定性注入(真实 pi AgentEvent 类型)演示桥+ACL".into(),
                ));
                inject_deterministic(tx);
            }
        }
        Err(e) => {
            // 无凭证时 create_agent_session 会在 resolve_api_key 处失败——这与 stable-1.85
            // **编译闸门(F3)无关**,纯属环境缺 key。仍走注入路径演示桥+ACL。
            let s: String = format!("{e:?}").chars().take(160).collect();
            let _ = tx.send(Bridge::Status(format!(
                "SESSION_CREATE_ERR(无凭证,与编译闸门无关): {s}"
            )));
            let _ = tx.send(Bridge::Status(
                "MODE=inject:改用确定性注入(真实 pi AgentEvent 类型)演示桥+ACL".into(),
            ));
            inject_deterministic(tx);
        }
    }
}

/// 用**真实** pi `AgentEvent` 类型构造一段单回合事件序列,逐条经**同一** demux 过桥。
/// 诚实标注:这不是 live 模型回复,而是确定性事件注入——用于在无凭证时仍能端到端
/// 演示 §3.3 seam(seq 单调、payload 形状、accumulated text)。
fn inject_deterministic(tx: &mpsc::Sender<Bridge>) {
    let sid: Arc<str> = Arc::from("r0-sess-INJECTED");
    // AssistantMessage 派生 Default;探针演示用的最终文本来自下面注入的 TextDelta 串,
    // 与 message 内容无关,故 default() 足够(且避开 `new` 这类版本相关构造器)。
    let am = Arc::new(AssistantMessage::default());

    let seq: Vec<AgentEvent> = vec![
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

    for ev in &seq {
        // 只把 demux 后的"数据"过桥——绝不把 AgentEvent 的任何 future 交给主线程 poll。
        let _ = tx.send(Bridge::Raw(demux(ev)));
    }
}

fn main() {
    println!("== R0 pi-spike: Engine Actor(asupersync 专用线程)⇄ 主线程(tokio 侧镜像)==");

    let (tx, rx) = mpsc::channel::<Bridge>();

    let engine = thread::Builder::new()
        .name("pi-engine".into())
        .spawn(move || {
            // 在专用线程上 bootstrap pi 的 asupersync runtime(注意:不是 tokio)。
            let reactor = match asupersync::runtime::reactor::create_reactor() {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(Bridge::Status(format!("FATAL create_reactor: {e:?}")));
                    let _ = tx.send(Bridge::Eos);
                    return;
                }
            };
            let runtime = match asupersync::runtime::RuntimeBuilder::current_thread()
                .with_reactor(reactor)
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(Bridge::Status(format!("FATAL build runtime: {e:?}")));
                    let _ = tx.send(Bridge::Eos);
                    return;
                }
            };

            runtime.block_on(engine_async(&tx));
            let _ = tx.send(Bridge::Eos);
        })
        .expect("spawn pi-engine thread");

    // ── 主线程 = tokio 侧镜像:只收"数据",ACL 翻译在此发生 ──
    let mut acl = Acl::new(CONV_ID);
    let mut seq_log: Vec<String> = Vec::new();
    let mut emitted: Vec<String> = Vec::new();
    let mut session_ok = false;

    while let Ok(msg) = rx.recv() {
        match msg {
            Bridge::Status(s) => {
                if s.starts_with("SESSION_CREATED") {
                    session_ok = true;
                }
                println!("[engine→main] {s}");
            }
            Bridge::Raw(raw) => {
                seq_log.push(raw_tag(&raw));
                println!("[bridge ] AgentEvent → {}", raw_tag(&raw));
                if let Some(out) = acl.translate(&raw) {
                    println!("[ACL→FE] {out}");
                    emitted.push(out);
                }
            }
            Bridge::Eos => break,
        }
    }
    let _ = engine.join();

    println!("\n== 捕获到的事件序列(Done-when #2)==");
    if seq_log.is_empty() {
        println!("  (空——无事件;多为会话构造失败且未进注入路径)");
    } else {
        println!("  {}", seq_log.join("  →  "));
    }

    println!("\n== §3.3 翻译 seam 合成的前端 payload(Done-when #3)==");
    for line in &emitted {
        println!("  {line}");
    }

    println!("\n== 小结 ==");
    println!(
        "  session_constructed={session_ok}  bridged_events={}  emitted_fe_payloads={}",
        seq_log.len(),
        emitted.len()
    );
    // 注:R0 实测 pi 依赖图最低需 stable 1.88(pi build-dep vergen-gix 9.1.0 + sysinfo/time
    // 把 MSRV 抬到 1.88);1.85 在解析阶段即被拒。本探针在 stable(1.88/1.95)上编译通过。
    if session_ok {
        println!("  R0: stable(≥1.88)编译通过 + asupersync 线程⇄主线程桥成立 + §3.3 seam 演示完成");
    } else {
        println!(
            "  R0: stable(≥1.88)编译通过 + 桥/seam 演示完成;会话未构造(无凭证,与编译闸门无关)"
        );
    }
}
