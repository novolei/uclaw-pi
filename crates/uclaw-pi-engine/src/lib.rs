//! `uclaw-pi-engine` — the **Engine Actor** + **ACL** (anti-corruption layer)
//! that drives the vendored pi agent runtime (`crates/pi`) and translates pi
//! [`pi::sdk::AgentEvent`]s into uClaw's frontend event contract
//! (`chat:stream-*`, see `ui/src/hooks/useGlobalAgentListeners.ts`).
//!
//! ## Architecture (per replication-plan §1 / migration-analysis §5)
//! pi runs on its own `asupersync` runtime on a dedicated OS thread; the
//! Tauri/tokio side communicates only by passing **data** over channels (never
//! a future). The ACL turns each pi `AgentEvent` into a `(event_name, payload)`
//! pair whose JSON shape matches `ui/src/lib/chat-types.ts`, and the engine
//! emits it through an [`EventSink`] (Tauri's `AppHandle::emit` in production).
//!
//! ## R1 status
//! - [`acl`] — the streaming §3.3 seam (chunk / reasoning / tool-activity /
//!   complete / error), with monotonic per-conversation `seq`. **Implemented +
//!   unit-tested here.**
//! - [`events`] — the frontend event-name constants + the [`EventSink`] trait.
//! - Engine Actor loop (asupersync thread + `EngineCmd` + `SessionRegistry`)
//!   and the Tauri `EventSink` impl land in the next R1 slice.
//!
//! Standalone sub-workspace until uClaw migrates off `rusqlite` (F2 reversal).

#![forbid(unsafe_code)]

pub mod acl;
pub mod approval;
pub mod dto;
pub mod engine;
pub mod events;
pub mod tool_bridge;
pub mod tool_factory;

pub use acl::{demux, Acl, FeEvent, RawEvt};
pub use approval::{make_approval_handler, ApprovalRegistry, PendingTicket};
pub use dto::{content_block_to_fe, message_to_chat_message, tool_output_to_result};
pub use engine::{EngineCmd, EngineConfig, PiEngine};
pub use events::{event, EventSink};
pub use tool_bridge::{BridgedIoTool, ResultTicket, ToolRequestSink, ToolResultRegistry};
pub use tool_factory::{UclawToolFactory, PI_BUILTIN_TOOLS};
