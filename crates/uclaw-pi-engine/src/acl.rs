//! ACL: translate pi [`AgentEvent`]s into uClaw frontend `chat:stream-*` payloads.
//!
//! Two halves:
//! 1. [`demux`] projects a real pi [`AgentEvent`] into [`RawEvt`] — owned data
//!    safe to cross the tokio↔asupersync boundary (never a future). Sharing this
//!    with the live callback means "we can handle real AgentEvents" is checked at
//!    compile time, not guessed.
//! 2. [`Acl`] consumes [`RawEvt`]s and emits [`FeEvent`]s whose JSON shape matches
//!    `ui/src/lib/chat-types.ts`. It owns the per-conversation `seq` counter
//!    (pi does not provide one) and accumulates assistant text for `complete`.

use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use pi::model::AssistantMessageEvent;
use pi::sdk::AgentEvent;
use serde::Serialize;
use serde_json::{json, Value};

use crate::events::event;

/// Boundary data projected from a real pi [`AgentEvent`]. Only owned values
/// cross the runtime boundary.
#[derive(Debug, Clone)]
pub enum RawEvt {
    AgentStart { session_id: String },
    TextDelta { delta: String },
    ThinkingDelta { delta: String },
    ToolStart {
        tool_name: String,
        tool_call_id: String,
        input: Value,
    },
    /// Streaming partial tool output (drives `liveOutput` — wired in a later slice).
    ToolUpdate {
        tool_call_id: String,
        partial: Value,
    },
    ToolEnd {
        tool_name: String,
        tool_call_id: String,
        result: Value,
        is_error: bool,
    },
    TurnEnd,
    AgentEnd { error: Option<String> },
    /// Any event the ACL does not (yet) project to a frontend event.
    Other { name: String },
}

/// Project a real pi [`AgentEvent`] into [`RawEvt`]. The live prompt callback and
/// any replay path share this function.
#[must_use]
pub fn demux(ev: &AgentEvent) -> RawEvt {
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
            tool_call_id,
            tool_name,
            args,
        } => RawEvt::ToolStart {
            tool_name: tool_name.clone(),
            tool_call_id: tool_call_id.clone(),
            input: args.clone(),
        },
        AgentEvent::ToolExecutionUpdate {
            tool_call_id,
            partial_result,
            ..
        } => RawEvt::ToolUpdate {
            tool_call_id: tool_call_id.clone(),
            partial: serde_json::to_value(partial_result).unwrap_or(Value::Null),
        },
        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
            is_error,
        } => RawEvt::ToolEnd {
            tool_name: tool_name.clone(),
            tool_call_id: tool_call_id.clone(),
            // [R4 F5] Normalize pi's ToolOutput → the flattened text string the
            // tool-renderers read, not the raw ToolOutput object.
            result: crate::dto::tool_output_to_result(result),
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

/// Short label for an [`AssistantMessageEvent`] (readable `Other` logging only).
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

/// Short label for an [`AgentEvent`] via its serde `type` tag (no per-variant hardcode).
fn agentevent_tag(e: &AgentEvent) -> String {
    serde_json::to_value(e)
        .ok()
        .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}

/// A translated frontend event: the `listen()` name + its JSON payload.
#[derive(Debug, Clone)]
pub struct FeEvent {
    pub name: &'static str,
    pub payload: Value,
}

// ── camelCase wire DTOs (must match ui/src/lib/chat-types.ts) ───────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamDelta {
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
    /// Accumulated thinking/reasoning text across the turn. The persist layer
    /// stores it as a leading `thinking` ContentBlock so the Agent view renders
    /// the THINKING block on reload. Empty string when the model didn't think.
    reasoning: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamErrorPayload {
    conversation_id: String,
    error: String,
}

/// Per-conversation ACL state: **independent** monotonic seq counters for the
/// chunk and reasoning channels, accumulated assistant text, the one-shot
/// `complete` latch, and tool start times for `durationMs`.
///
/// The two seq spaces are separate because `useGlobalAgentListeners.ts` tracks
/// `lastChunkSeq` and `lastReasoningSeq` independently and treats `seq === 0` as
/// "new stream" **per channel**. A shared counter would emit the second-started
/// channel's first delta at seq > 0, defeating its new-stream reset (a
/// thinks-then-speaks turn would append text onto a stale buffer).
pub struct Acl {
    conv_id: String,
    chunk_seq: u64,
    reasoning_seq: u64,
    acc_text: String,
    acc_reasoning: String,
    completed: bool,
    tool_starts: HashMap<String, Instant>,
    /// Per-tool-call (last cumulative output text already forwarded, next seq).
    /// pi's `ToolUpdate` carries the CUMULATIVE (tail-truncated) output so far,
    /// but the frontend `liveOutput` appends each chunk — so we diff against this
    /// to emit only the new suffix. Cleared on `ToolEnd`.
    tool_output_acc: HashMap<String, (String, u64)>,
}

impl Acl {
    #[must_use]
    pub fn new(conv_id: impl Into<String>) -> Self {
        Self {
            conv_id: conv_id.into(),
            chunk_seq: 0,
            reasoning_seq: 0,
            acc_text: String::new(),
            acc_reasoning: String::new(),
            completed: false,
            tool_starts: HashMap::new(),
            tool_output_acc: HashMap::new(),
        }
    }

    fn next_chunk_seq(&mut self) -> u64 {
        let s = self.chunk_seq;
        self.chunk_seq += 1;
        s
    }

    fn next_reasoning_seq(&mut self) -> u64 {
        let s = self.reasoning_seq;
        self.reasoning_seq += 1;
        s
    }

    /// Translate one boundary event into a frontend event. `None` = this event
    /// produces no frontend event.
    pub fn translate(&mut self, raw: &RawEvt) -> Option<FeEvent> {
        match raw {
            RawEvt::TextDelta { delta } => {
                self.acc_text.push_str(delta);
                let seq = self.next_chunk_seq();
                Some(FeEvent {
                    name: event::STREAM_CHUNK,
                    payload: to_value(&StreamDelta {
                        conversation_id: self.conv_id.clone(),
                        delta: delta.clone(),
                        seq,
                    }),
                })
            }
            RawEvt::ThinkingDelta { delta } => {
                self.acc_reasoning.push_str(delta);
                let seq = self.next_reasoning_seq();
                Some(FeEvent {
                    name: event::STREAM_REASONING,
                    payload: to_value(&StreamDelta {
                        conversation_id: self.conv_id.clone(),
                        delta: delta.clone(),
                        seq,
                    }),
                })
            }
            RawEvt::ToolStart {
                tool_name,
                tool_call_id,
                input,
            } => {
                self.tool_starts.insert(tool_call_id.clone(), Instant::now());
                Some(FeEvent {
                    name: event::STREAM_TOOL_ACTIVITY,
                    payload: json!({
                        "conversationId": self.conv_id,
                        "activity": {
                            "type": "tool_start",
                            "toolName": tool_name,
                            "toolCallId": tool_call_id,
                            "input": input,
                            "timestamp": now_ms(),
                        }
                    }),
                })
            }
            RawEvt::ToolEnd {
                tool_name,
                tool_call_id,
                result,
                is_error,
            } => {
                let duration_ms = self
                    .tool_starts
                    .remove(tool_call_id)
                    .map(|s| u64::try_from(s.elapsed().as_millis()).unwrap_or(u64::MAX));
                self.tool_output_acc.remove(tool_call_id);
                Some(FeEvent {
                    name: event::STREAM_TOOL_ACTIVITY,
                    payload: json!({
                        "conversationId": self.conv_id,
                        "activity": {
                            "type": "tool_result",
                            "toolName": tool_name,
                            "toolCallId": tool_call_id,
                            "result": result,
                            "isError": is_error,
                            "durationMs": duration_ms,
                            "timestamp": now_ms(),
                        }
                    }),
                })
            }
            RawEvt::AgentEnd { error: Some(e) } => Some(FeEvent {
                name: event::STREAM_ERROR,
                payload: to_value(&StreamErrorPayload {
                    conversation_id: self.conv_id.clone(),
                    error: e.clone(),
                }),
            }),
            // `complete` fires ONLY on AgentEnd{error:None} — the single terminal
            // event pi emits once the whole turn drains (agent.rs:1851). NOT on
            // TurnEnd: pi emits a TurnEnd after EVERY assistant message
            // (agent.rs:1818), so in a tool turn (LLM→toolcall→tool→TurnEnd→LLM→
            // TurnEnd→AgentEnd) completing on the first TurnEnd would latch right
            // after the preamble and silently drop the tool result + final answer —
            // the "stuck running / reply is just '好的，我看一下…'" bug. `truncated`
            // is false here; a real run sets it when uClaw-side compaction drops
            // history.
            RawEvt::AgentEnd { error: None } if !self.completed => {
                self.completed = true;
                Some(FeEvent {
                    name: event::STREAM_COMPLETE,
                    payload: to_value(&StreamComplete {
                        conversation_id: self.conv_id.clone(),
                        text: self.acc_text.clone(),
                        truncated: false,
                        reasoning: self.acc_reasoning.clone(),
                    }),
                })
            }
            // [pi 闭环] Streaming tool output → `tool_output_chunk` (drives the
            // frontend `liveOutput`, e.g. live bash stdout). pi's update carries
            // the CUMULATIVE tail-truncated buffer; the frontend appends, so we
            // forward only the new suffix (common-prefix diff). On a tail-
            // truncation reshuffle the prefix no longer matches and we resend the
            // current view (rare; the live preview is ephemeral, and the terminal
            // `tool_result` is authoritative). pi merges stdout+stderr into one
            // buffer → stream "stdout".
            RawEvt::ToolUpdate { tool_call_id, partial } => {
                let cur = extract_tool_update_text(partial);
                let (delta, seq) = {
                    let entry = self
                        .tool_output_acc
                        .entry(tool_call_id.clone())
                        .or_default();
                    let delta = if cur.len() >= entry.0.len() && cur.starts_with(&entry.0) {
                        cur[entry.0.len()..].to_string()
                    } else {
                        cur.clone()
                    };
                    entry.0 = cur;
                    if delta.is_empty() {
                        (delta, 0)
                    } else {
                        let s = entry.1;
                        entry.1 += 1;
                        (delta, s)
                    }
                };
                if delta.is_empty() {
                    None
                } else {
                    Some(FeEvent {
                        name: event::STREAM_TOOL_ACTIVITY,
                        payload: json!({
                            "conversationId": self.conv_id,
                            "activity": {
                                "type": "tool_output_chunk",
                                "toolCallId": tool_call_id,
                                "seq": seq,
                                "stream": "stdout",
                                "chunk": delta,
                                "timestamp": now_ms(),
                            }
                        }),
                    })
                }
            }
            // AgentStart, TurnEnd, duplicate completes → no FE event.
            _ => None,
        }
    }
}

/// Extract the plain text from a serialized pi `ToolOutput`/`ToolUpdate`
/// (`{ "content": [{ "type": "text", "text": "…" }, …] }`) — the cumulative
/// tool output carried by `RawEvt::ToolUpdate`. Non-text blocks are skipped.
fn extract_tool_update_text(partial: &Value) -> String {
    let Some(blocks) = partial.get("content").and_then(Value::as_array) else {
        return String::new();
    };
    let mut out = String::new();
    for b in blocks {
        if b.get("type").and_then(Value::as_str) == Some("text") {
            if let Some(t) = b.get("text").and_then(Value::as_str) {
                out.push_str(t);
            }
        }
    }
    out
}

fn to_value<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_seq_is_monotonic_and_text_accumulates() {
        let mut acl = Acl::new("c1");
        // AgentStart produces no frontend event.
        assert!(acl
            .translate(&RawEvt::AgentStart {
                session_id: "s".into()
            })
            .is_none());

        let e0 = acl
            .translate(&RawEvt::TextDelta {
                delta: "pong".into(),
            })
            .expect("chunk");
        assert_eq!(e0.name, event::STREAM_CHUNK);
        assert_eq!(e0.payload["conversationId"], "c1");
        assert_eq!(e0.payload["delta"], "pong");
        assert_eq!(e0.payload["seq"], 0);

        let e1 = acl
            .translate(&RawEvt::TextDelta {
                delta: " from the void".into(),
            })
            .expect("chunk");
        assert_eq!(e1.payload["seq"], 1);

        // TurnEnd fires after every assistant message and must NOT complete;
        // the single terminal AgentEnd{None} carries the accumulated text.
        assert!(acl.translate(&RawEvt::TurnEnd).is_none());
        let c = acl
            .translate(&RawEvt::AgentEnd { error: None })
            .expect("complete");
        assert_eq!(c.name, event::STREAM_COMPLETE);
        assert_eq!(c.payload["text"], "pong from the void");
        assert_eq!(c.payload["truncated"], false);

        // A trailing TurnEnd must NOT re-emit complete (one-shot latch).
        assert!(acl.translate(&RawEvt::TurnEnd).is_none());
    }

    #[test]
    fn chunk_and_reasoning_have_independent_seq_each_from_zero() {
        // Mirrors useGlobalAgentListeners.ts: `lastChunkSeq` / `lastReasoningSeq`
        // are tracked separately and `seq === 0` means "new stream" PER channel.
        // So the first delta on EACH channel must be seq 0 — a thinks-then-speaks
        // turn must not emit text at seq 1 (which would defeat the chunk channel's
        // new-stream reset and append onto a stale buffer).
        let mut acl = Acl::new("c1");
        let r = acl
            .translate(&RawEvt::ThinkingDelta { delta: "hmm".into() })
            .expect("reasoning");
        assert_eq!(r.name, event::STREAM_REASONING);
        assert_eq!(r.payload["seq"], 0);

        let t = acl
            .translate(&RawEvt::TextDelta { delta: "x".into() })
            .expect("chunk");
        assert_eq!(
            t.payload["seq"], 0,
            "chunk seq is independent — first text delta is seq 0 even after reasoning"
        );

        // Each channel then advances on its own counter.
        let r1 = acl
            .translate(&RawEvt::ThinkingDelta { delta: "m".into() })
            .unwrap();
        assert_eq!(r1.payload["seq"], 1);
        let t1 = acl.translate(&RawEvt::TextDelta { delta: "y".into() }).unwrap();
        assert_eq!(t1.payload["seq"], 1);
    }

    #[test]
    fn agent_end_with_error_maps_to_stream_error() {
        let mut acl = Acl::new("c1");
        let e = acl
            .translate(&RawEvt::AgentEnd {
                error: Some("boom".into()),
            })
            .expect("error");
        assert_eq!(e.name, event::STREAM_ERROR);
        assert_eq!(e.payload["conversationId"], "c1");
        assert_eq!(e.payload["error"], "boom");
    }

    #[test]
    fn tool_activity_start_and_result_shapes() {
        let mut acl = Acl::new("c1");
        let s = acl
            .translate(&RawEvt::ToolStart {
                tool_name: "bash".into(),
                tool_call_id: "t1".into(),
                input: json!({ "cmd": "ls" }),
            })
            .expect("tool_start");
        assert_eq!(s.name, event::STREAM_TOOL_ACTIVITY);
        assert_eq!(s.payload["activity"]["type"], "tool_start");
        assert_eq!(s.payload["activity"]["toolName"], "bash");
        assert_eq!(s.payload["activity"]["toolCallId"], "t1");
        assert_eq!(s.payload["activity"]["input"]["cmd"], "ls");

        let e = acl
            .translate(&RawEvt::ToolEnd {
                tool_name: "bash".into(),
                tool_call_id: "t1".into(),
                result: json!({ "content": [] }),
                is_error: false,
            })
            .expect("tool_result");
        assert_eq!(e.payload["activity"]["type"], "tool_result");
        assert_eq!(e.payload["activity"]["isError"], false);
        // durationMs present (resolved from the matching start).
        assert!(e.payload["activity"]["durationMs"].is_u64());
    }

    /// Frontend event-shape CONTRACT guard. Each event the pi ACL projects must
    /// carry the exact fields the frontend listeners hard-depend on (see
    /// `docs/audits/2026-06-04-legacy-pi-event-parity-matrix.md`). A field the
    /// frontend reads behind an early-return — e.g. `chat:stream-tool-activity`
    /// `tool_start` needs `toolCallId`, else `useGlobalAgentListeners` returns
    /// before opening the write preview — must never silently disappear from the
    /// ACL output. This is the regression guard for the whole class of
    /// "pi emits one fewer field than legacy → feature silently dies on pi" bugs
    /// (the auto-preview `previewTarget` regression was exactly this shape).
    ///
    /// NOTE: `previewTarget` is injected by the uClaw bridge (`engine_sink.rs`,
    /// covered by its own `inject_preview_target_*` test), not the ACL, so it is
    /// asserted there. This test pins only what the engine-side ACL must emit.
    #[test]
    fn fe_contract_required_fields_present() {
        let req = |payload: &serde_json::Value, keys: &[&str], ctx: &str| {
            for k in keys {
                assert!(
                    payload.pointer(k).is_some(),
                    "{ctx}: missing frontend-required field `{k}` in {payload}"
                );
            }
        };

        let mut acl = Acl::new("c1");

        // chat:stream-chunk → conversationId, delta, seq
        let chunk = acl
            .translate(&RawEvt::TextDelta { delta: "hi".into() })
            .expect("chunk");
        assert_eq!(chunk.name, event::STREAM_CHUNK);
        req(&chunk.payload, &["/conversationId", "/delta", "/seq"], "stream-chunk");

        // chat:stream-reasoning → conversationId, delta, seq
        let reasoning = acl
            .translate(&RawEvt::ThinkingDelta { delta: "mm".into() })
            .expect("reasoning");
        assert_eq!(reasoning.name, event::STREAM_REASONING);
        req(&reasoning.payload, &["/conversationId", "/delta", "/seq"], "stream-reasoning");

        // chat:stream-tool-activity tool_start → conversationId + activity.{type,toolName,toolCallId,input}
        let ts = acl
            .translate(&RawEvt::ToolStart {
                tool_name: "write".into(),
                tool_call_id: "t1".into(),
                input: json!({ "path": "/w/a.rs" }),
            })
            .expect("tool_start");
        req(
            &ts.payload,
            &[
                "/conversationId",
                "/activity/type",
                "/activity/toolName",
                "/activity/toolCallId",
                "/activity/input",
            ],
            "tool_start",
        );

        // chat:stream-tool-activity tool_result → + result, isError, durationMs
        let tr = acl
            .translate(&RawEvt::ToolEnd {
                tool_name: "write".into(),
                tool_call_id: "t1".into(),
                result: json!({ "content": [] }),
                is_error: false,
            })
            .expect("tool_result");
        req(
            &tr.payload,
            &[
                "/conversationId",
                "/activity/type",
                "/activity/toolName",
                "/activity/toolCallId",
                "/activity/result",
                "/activity/isError",
                "/activity/durationMs",
            ],
            "tool_result",
        );

        // chat:stream-complete → conversationId, text
        let done = acl
            .translate(&RawEvt::AgentEnd { error: None })
            .expect("complete");
        assert_eq!(done.name, event::STREAM_COMPLETE);
        req(&done.payload, &["/conversationId", "/text"], "stream-complete");

        // chat:stream-error → conversationId, error
        let mut acl2 = Acl::new("c2");
        let err = acl2
            .translate(&RawEvt::AgentEnd { error: Some("boom".into()) })
            .expect("error");
        assert_eq!(err.name, event::STREAM_ERROR);
        req(&err.payload, &["/conversationId", "/error"], "stream-error");
    }

    #[test]
    fn tool_update_emits_incremental_output_chunks() {
        // pi sends CUMULATIVE tail-truncated output; the ACL must forward only
        // the new suffix so the frontend's appending liveOutput isn't duplicated.
        let mut acl = Acl::new("c1");
        let upd = |text: &str| RawEvt::ToolUpdate {
            tool_call_id: "t1".into(),
            partial: json!({ "content": [{ "type": "text", "text": text }] }),
        };

        let a = acl.translate(&upd("hello")).expect("first chunk");
        assert_eq!(a.payload["activity"]["type"], "tool_output_chunk");
        assert_eq!(a.payload["activity"]["toolCallId"], "t1");
        assert_eq!(a.payload["activity"]["stream"], "stdout");
        assert_eq!(a.payload["activity"]["chunk"], "hello");
        assert_eq!(a.payload["activity"]["seq"], 0);

        // cumulative "hello world" → only " world" forwarded
        let b = acl.translate(&upd("hello world")).expect("delta chunk");
        assert_eq!(b.payload["activity"]["chunk"], " world");
        assert_eq!(b.payload["activity"]["seq"], 1);

        // identical cumulative (no new bytes) → no event
        assert!(acl.translate(&upd("hello world")).is_none());

        // a different tool call has its own accumulator
        let other = acl
            .translate(&RawEvt::ToolUpdate {
                tool_call_id: "t2".into(),
                partial: json!({ "content": [{ "type": "text", "text": "x" }] }),
            })
            .expect("t2 chunk");
        assert_eq!(other.payload["activity"]["chunk"], "x");

        // tool_result clears the accumulator so a re-used id starts fresh
        let _ = acl.translate(&RawEvt::ToolEnd {
            tool_name: "bash".into(),
            tool_call_id: "t1".into(),
            result: json!({ "content": [] }),
            is_error: false,
        });
        let c = acl.translate(&upd("again")).expect("post-end chunk");
        assert_eq!(c.payload["activity"]["chunk"], "again");
    }

    /// [R2 Done-when#1] A whole realistic turn — think, speak, call a tool, speak
    /// again, finish — produces exactly the frontend-render event sequence:
    /// reasoning deltas (own seq space), chunk deltas (own seq space), a
    /// tool_start + tool_result card, and one terminal `complete` carrying the
    /// full accumulated text. This is the scripted proof that "一整段对话渲染"
    /// 1:1; the live screenshot with an API key is the remaining manual check.
    #[test]
    fn full_turn_renders_text_thinking_and_tool_card() {
        let mut acl = Acl::new("conv");
        let feed = [
            RawEvt::AgentStart {
                session_id: "s".into(),
            },
            RawEvt::ThinkingDelta {
                delta: "let me ".into(),
            },
            RawEvt::ThinkingDelta {
                delta: "think".into(),
            },
            RawEvt::TextDelta {
                delta: "Running ".into(),
            },
            RawEvt::TextDelta { delta: "ls.".into() },
            RawEvt::ToolStart {
                tool_name: "bash".into(),
                tool_call_id: "t1".into(),
                input: json!({ "cmd": "ls" }),
            },
            RawEvt::ToolEnd {
                tool_name: "bash".into(),
                tool_call_id: "t1".into(),
                result: json!({ "content": [{ "type": "text", "text": "a.txt" }] }),
                is_error: false,
            },
            // pi emits a TurnEnd after the tool-call assistant message
            // (agent.rs:1818), BEFORE the follow-up LLM answer streams. It must NOT
            // complete — the old TurnEnd-completes path latched here and dropped the
            // " Done." that follows (the stuck-running bug).
            RawEvt::TurnEnd,
            RawEvt::TextDelta {
                delta: " Done.".into(),
            },
            RawEvt::TurnEnd,
            RawEvt::AgentEnd { error: None },
        ];
        let events: Vec<FeEvent> = feed.iter().filter_map(|ev| acl.translate(ev)).collect();

        // AgentStart yields nothing; the rest map 1:1 in order.
        let names: Vec<&str> = events.iter().map(|e| e.name).collect();
        assert_eq!(
            names,
            vec![
                event::STREAM_REASONING,
                event::STREAM_REASONING,
                event::STREAM_CHUNK,
                event::STREAM_CHUNK,
                event::STREAM_TOOL_ACTIVITY,
                event::STREAM_TOOL_ACTIVITY,
                event::STREAM_CHUNK,
                event::STREAM_COMPLETE,
            ]
        );
        // Independent seq spaces: reasoning 0,1 — chunk 0,1,2.
        assert_eq!(events[0].payload["seq"], 0);
        assert_eq!(events[1].payload["seq"], 1);
        assert_eq!(events[2].payload["seq"], 0);
        assert_eq!(events[3].payload["seq"], 1);
        assert_eq!(events[6].payload["seq"], 2);
        // Tool card shapes (start + result).
        assert_eq!(events[4].payload["activity"]["type"], "tool_start");
        assert_eq!(events[4].payload["activity"]["toolName"], "bash");
        assert_eq!(events[5].payload["activity"]["type"], "tool_result");
        assert_eq!(events[5].payload["activity"]["isError"], false);
        // Complete carries the full accumulated assistant text (chunks only)…
        assert_eq!(events[7].payload["text"], "Running ls. Done.");
        assert_eq!(events[7].payload["truncated"], false);
        // …and the accumulated thinking text (persisted as a leading thinking block).
        assert_eq!(events[7].payload["reasoning"], "let me think");
    }

    #[test]
    fn complete_emits_once_from_agent_end_when_no_turn_end() {
        let mut acl = Acl::new("c1");
        let c = acl
            .translate(&RawEvt::AgentEnd { error: None })
            .expect("complete from AgentEnd");
        assert_eq!(c.name, event::STREAM_COMPLETE);
        // A later TurnEnd must not double-emit.
        assert!(acl.translate(&RawEvt::TurnEnd).is_none());
    }

    /// [R2 Done-when#4] Under a flood of interleaved chunk/reasoning/tool events,
    /// each channel's synthesized `seq` must be strictly monotonic with no gaps
    /// and no duplicates (independently — chunk and reasoning have separate seq
    /// spaces, matching the frontend's `lastChunkSeq`/`lastReasoningSeq`), and
    /// `complete` must fire exactly once. Tool-activity carries no `seq` (keyed by
    /// toolCallId) — assert that explicitly too.
    #[test]
    fn flood_per_channel_seq_contiguous_no_gap_no_dup() {
        let mut acl = Acl::new("flood");
        let mut chunk_seqs: Vec<u64> = Vec::new();
        let mut reasoning_seqs: Vec<u64> = Vec::new();
        let mut completes = 0;

        // 600 interleaved events: text, thinking, and tool start/end every 7th.
        for i in 0..600u64 {
            let ev = match i % 7 {
                0 => RawEvt::ThinkingDelta {
                    delta: format!("r{i}"),
                },
                3 => RawEvt::ToolStart {
                    tool_name: "bash".into(),
                    tool_call_id: format!("t{i}"),
                    input: json!({}),
                },
                4 => RawEvt::ToolEnd {
                    tool_name: "bash".into(),
                    tool_call_id: format!("t{}", i - 1),
                    result: json!({}),
                    is_error: false,
                },
                _ => RawEvt::TextDelta {
                    delta: format!("c{i}"),
                },
            };
            if let Some(fe) = acl.translate(&ev) {
                match fe.name {
                    event::STREAM_CHUNK => {
                        chunk_seqs.push(fe.payload["seq"].as_u64().expect("seq present"));
                    }
                    event::STREAM_REASONING => {
                        reasoning_seqs.push(fe.payload["seq"].as_u64().expect("seq present"));
                    }
                    event::STREAM_TOOL_ACTIVITY => {
                        // Tool activity is keyed by toolCallId, not seq.
                        assert!(fe.payload.get("seq").is_none());
                    }
                    other => panic!("unexpected event during flood: {other}"),
                }
            }
        }

        // Each channel is independently contiguous from 0: [0, 1, 2, ... n-1].
        assert!(!chunk_seqs.is_empty() && !reasoning_seqs.is_empty());
        for (i, s) in chunk_seqs.iter().enumerate() {
            assert_eq!(*s, i as u64, "chunk seq must be contiguous (no gap/dup/reorder)");
        }
        for (i, s) in reasoning_seqs.iter().enumerate() {
            assert_eq!(*s, i as u64, "reasoning seq must be contiguous (no gap/dup/reorder)");
        }

        // Exactly one complete, regardless of how many terminal events arrive.
        for _ in 0..3 {
            if acl.translate(&RawEvt::TurnEnd).is_some() {
                completes += 1;
            }
            if acl.translate(&RawEvt::AgentEnd { error: None }).is_some() {
                completes += 1;
            }
        }
        assert_eq!(completes, 1, "complete is one-shot under terminal-event flood");
    }
}
