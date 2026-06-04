# Legacy ↔ Pi event / side-effect parity matrix (2026-06-04)

## Why this exists

Over the past weeks a recurring class of bug kept surfacing: a per-turn
side-effect or UI event that worked on the **legacy** agent path (the tail of
`send_agent_message` + the `ToolDispatcher`) silently stopped working once the
**pi engine** became the active path, because nobody re-wired it. Fixed instances:

| PR | Symptom | The missing pi wiring |
|----|---------|------------------------|
| #71 | session not auto-renamed | title generation only in legacy tail |
| #74 | proactive learning blind to pi turns | `publish_incoming` only in legacy tail |
| #75 | `/skill` slash command no-op | `resolve_slash_skill` only in legacy tail |
| #76 | failure memory / preference extraction never ran | only in legacy tail → moved to `EventSink` |
| #79 | write tool didn't auto-open preview | `previewTarget` only set by legacy `emit_tool_start` |

This document is the **standing parity checklist**: every frontend-consumed event
and every per-turn side-effect, with its status on each path. New per-turn
features must be checked against this before they're considered done.

## Method

Three inventories were cross-referenced:
- **Legacy**: `tauri_commands.rs` `send_agent_message` tail + `agent/tool_dispatch/mod.rs` + `agent/dispatcher/observability.rs`.
- **Pi**: `crates/uclaw-pi-engine/src/{acl,engine,events}.rs` + `engine_sink.rs` (`TauriEventSink`, `persist_assistant`, `RealToolRequestSink`) + the pi branch of `send_agent_message`.
- **Frontend**: every `listen(...)` in `ui/src/`, and the exact payload fields each handler hard-depends on (fields read behind an early-return).

## Event matrix (frontend-consumed)

Legend: ✅ covered · ⚠️ GAP (pi missing / mis-shaped) · ⏭️ expected-skip · 💀 dead on both paths

| Event | FE hard-required fields | Legacy | Pi | Status |
|-------|-------------------------|:------:|:--:|--------|
| `chat:stream-chunk` | conversationId, delta, seq | ✅ | ✅ (acl) | ✅ |
| `chat:stream-reasoning` | conversationId, delta, seq | ✅ | ✅ (acl) | ✅ |
| `chat:stream-tool-activity` `tool_start` | conversationId, activity.{type,toolName,toolCallId} (+previewTarget for write) | ✅ | ✅ (acl) + previewTarget via bridge (#79) | ✅ |
| `chat:stream-tool-activity` `tool_result` | conversationId, activity.{type,toolName,toolCallId,result,isError,durationMs} | ✅ | ✅ (acl) | ✅ |
| `chat:stream-tool-activity` `tool_output_chunk` | activity.{toolCallId,stream,chunk,seq} | ✅ (tool_dispatch drain) | ✅ ACL projects `RawEvt::ToolUpdate` → `tool_output_chunk`; pi sends cumulative tail-truncated output so the ACL forwards only the new suffix (common-prefix diff), stream `stdout` (pi merges streams) | ✅ |
| `chat:stream-complete` | conversationId (+text) | ✅ | ✅ (acl) → persist_assistant | ✅ |
| `chat:stream-error` | conversationId, error | ✅ | ✅ (acl) → failure record (#76) | ✅ |
| `agent:turn_cost` | conversationId, inputTokens, outputTokens, costUsd | ✅ | ✅ (engine.rs) + cost recompute in bridge | ✅ |
| `agent:context_stats` | conversationId, modelContextLength, skillsTokens, freeTokens | ✅ (observability) | ✅ derived from `agent:turn_cost` in the bridge (`engine_sink.rs`): `modelContextLength` via `get_model_context_length`, `freeTokens = window − (input+cache)`, `skillsTokens=0` (not broken out on pi) | ✅ |
| `session:title-pending` / `session:title-updated` | sessionId, title, emoji | ✅ | ✅ (#71; emoji varied #78) | ✅ |
| `agent:memory-recall` | totalCandidates, skillsCount, timestamp, conversationId | ✅ | ✅ (pi branch spawn) | ✅ |
| `agent:skill-recalled` | conversationId, toolCallId, kind, … | ✅ (real conv id) | ✅ real conv id threaded engine → `ToolRequestSink::request(conversation_id, …)` → `build_skill_tool`; per-conv `UclawToolFactory::with_conversation` so each session's `BridgedIoTool` carries its `conv_id` (falls back to `PI_TOOL_CONV` only when unknown) | ✅ |
| `agent:proactive-learning` | scenario, items_extracted, … | ✅ | ✅ (publish_incoming #74 → ProactiveService) | ✅ |
| `agent:need_approval` | toolName, toolId, sessionId, … | ✅ | ✅ functional — `approval.rs` emits `{requestId, toolCallId, toolName, arguments}`; round-trip keyed by `requestId` (works). Off by default (opt-in `UCLAW_PI_APPROVAL`). Cosmetic only: no `toolId`/`sessionId` | ✅ |
| `agent:exit_plan_request` | requestId, (sessionId), plan | ✅ | ✅ functional — ExitPlanTool (`tool_factory.rs`) emits `{requestId, plan}`; ExitPlanModeBanner renders `plan`, `respond_exit_plan_mode` answers by `requestId`. `sessionId`/`allowedPrompts` absent but unused by the round-trip | ✅ |
| `agent:ask_user_request` | requestId, sessionId, questions[] | ✅ (uClaw `AskUserTool`) | ⚠️ **GAP** — the `ask_user` tool isn't in pi's tool set (pi built-ins + ExitPlanTool + skill/MCP only), so `ASK_USER_REQUEST` never fires; the agent can't ask clarifying questions on pi | ⚠️ |
| `agent:stream-reset` | conversationId | ✅ (retry path) | ⚠️ likely GAP (pi retry doesn't emit) — low impact | ⚠️ |
| `agent:queued-consumed` | sessionId, uuid | ✅ (steering) | ⚠️ verify (pi steering) — low impact | ⚠️ |
| `agent:reflection_status` / `agent:reflection` | assistant_message_id, … | ✅ | ⚠️ pi runs `run_once` (#76 turn-count) but per-message reflection-status events unverified | ⚠️ |
| `agent:daydream` | content | n/a | ✅ (own feature) | ✅ |
| `agent:file-written` | path | 💀 no emitter | 💀 no emitter | 💀 dead on both — frontend `usePreviewRefresh` also uses `tauri://focus`; not pi-specific |

## Confirmed gaps worth fixing (follow-up PRs)

Ranked by user-visible impact. Each is its own small PR.

1. ~~**`agent:context_stats` on pi (HIGH).**~~ **RESOLVED.** The bridge now
   derives + emits `agent:context_stats` alongside `agent:turn_cost` (token
   counts already in hand): `modelContextLength` from `get_model_context_length`,
   `freeTokens = window − (input + cache)`, `skillsTokens = 0` (pi doesn't track
   the skills manifest separately). The per-turn used-token figure still comes
   from `agent:turn_cost` itself; this restores the meter's denominator.

2. ~~**`agent:skill-recalled` real conversation id (MEDIUM).**~~ **RESOLVED.** The
   real `conv_id` is now threaded engine-side: `UclawToolFactory::with_conversation`
   builds a per-session factory (sessions are 1:1 with `conv_id`), so each
   `BridgedIoTool` carries its owning `conv_id` and passes it through
   `ToolRequestSink::request(conversation_id, …)` → `build_skill_tool`. Skill tools
   stamp the real session on `agent:skill-recalled`; `PI_TOOL_CONV` remains only as
   the fallback when the conversation is unknown (e.g. spec advertisement).

3. ~~**`tool_output_chunk` streaming on pi (MEDIUM).**~~ **RESOLVED.** The ACL now
   projects `RawEvt::ToolUpdate` → `chat:stream-tool-activity` `tool_output_chunk`.
   pi sends the *cumulative* tail-truncated buffer while the frontend appends, so
   the ACL diffs against the last forwarded text and emits only the new suffix
   (per `tool_call_id`, cleared on `ToolEnd`); `stream` is `stdout` (pi merges
   stdout+stderr). Verified end-to-end: pi emits `ToolExecutionUpdate` from bash
   (`agent.rs`) → demux → ACL.

4. **Verify approval / ask_user / exit_plan field shapes (MEDIUM).** **VERIFIED:**
   - **need_approval** — functional. `approval.rs` emits `{requestId, toolCallId,
     toolName, arguments}`; the user reply is keyed by `requestId`, so the
     round-trip works. It's off by default (opt-in `UCLAW_PI_APPROVAL`). Missing
     `toolId`/`sessionId` is cosmetic (the frontend tolerates it). No fix needed.
   - **exit_plan** — functional. ExitPlanTool emits `{requestId, plan}`;
     `ExitPlanModeBanner` renders `plan` and `respond_exit_plan_mode` answers by
     `requestId`. `sessionId`/`allowedPrompts` are absent but unused by the
     round-trip. No fix needed.
   - **ask_user** — **NOT functional → new gap #6.** The `ask_user` tool isn't in
     pi's tool registry (pi built-ins + ExitPlanTool + skill/MCP only), so
     `ASK_USER_REQUEST` never fires and the agent can't ask the user clarifying
     questions on pi. This is a missing-tool (feature) gap, not a field tweak.

6. **`ask_user` tool on pi (MEDIUM).** Add a pi-side ask_user tool (mirroring the
   injected `ExitPlanTool` in `tool_factory.rs`): emit `agent:ask_user_request`
   `{requestId, questions[]}` and await via the approval registry; the existing
   `respond_ask_user` command already closes the loop by `requestId`. (uClaw's
   legacy `AskUserTool` lives in the `uclaw` crate, below `uclaw-pi-engine`, so it
   can't be pushed into the engine factory directly — a small engine-side tool is
   the clean shape.)

5. **`agent:stream-reset` / `agent:queued-consumed` / reflection-status (LOW).**
   Retry-reset, steering-queue, and per-message reflection-status events — verify
   and wire if the corresponding UI affordances are expected on pi.

## Regression guard (this PR)

`crates/uclaw-pi-engine/src/acl.rs::fe_contract_required_fields_present` pins the
field contract for every event the ACL projects (chunk, reasoning, tool_start,
tool_result, complete, error). If a future change drops a frontend-required field
from the ACL output, CI fails instead of the feature silently dying on pi.
(`previewTarget` is injected by the bridge, so it's guarded by
`engine_sink.rs::inject_preview_target_*` instead.)

## Guardrail

The root cause is always the same: **per-turn behavior must be wired into the pi
path, not only the legacy tail of `send_agent_message`.** Per-turn *prompt-time*
effects go in the pi branch of `send_agent_message`; *post-turn / streaming*
effects go in `EventSink` / the ACL. When adding any new event or side-effect,
add a row here and a field assertion to the contract test.
