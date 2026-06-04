# Audit: per-turn side-effects skipped by the pi-engine path

**Date:** 2026-06-04
**Scope:** `send_agent_message` (`src-tauri/src/tauri_commands.rs`). The pi-engine branch
(`if engine_sink::pi_engine_enabled() && msg != "/compact" { … engine.send(EngineCmd::Prompt); return Ok(()); }`)
returns early. Everything in the "legacy tail" after that `return` is **skipped when the pi
engine is active** (the user's default loop). Session-title generation was found to be
silently dead this way (fixed in #71); this audit enumerates the rest.

## Method

Compared the pi-engine branch + its `EventSink` (`engine_sink.rs`) against the legacy tail,
classifying each per-turn side-effect as **COVERED** (pi/EventSink does an equivalent — do
NOT re-wire, would double-fire), **MISSING** (genuinely skipped — candidate to wire), or
**EXPECTED-SKIP** (legacy-only by design).

## Findings

| Side-effect | Legacy line(s) | pi coverage | Evidence | Action |
|---|---|---|---|---|
| Session title generation | ~5707 | **was MISSING → FIXED** | wired into pi branch | #71 |
| `publish_incoming` → ProactiveService (conversation_learning, skill_extraction) | ~5703 | **MISSING → FIXED** | pi branch never called it | **this PR** |
| Plan-mode auto-suggest (`agent:plan_mode_suggest`) | ~5243-5295 | COVERED | runs after the pi check (shared, not inside the returning branch) | none |
| Bucket-seal ingest (user + assistant) | pi 4996; `engine_sink.rs:95-100` | COVERED | pi spawns user ingest; EventSink ingests assistant | none |
| Memory reflection trigger (every N turns) | `engine_sink.rs:102-121` | COVERED | EventSink runs `reflection_service::run_once` | none |
| Tool-execution publishing (gene distillation) | `engine_sink.rs:365-389` | COVERED | EventSink publishes tool-executed | none |
| Message persistence (user/system INSERT) + `message_count` | ~5683-5698 | COVERED (different mechanism) | pi persists user at ~4978-4988; EventSink persists assistant — do NOT re-wire (would duplicate rows / double-count) | none |
| `/compact` compaction + its `chat:stream-complete` | ~5297-5630 | EXPECTED-SKIP | pi branch explicitly excludes `/compact` (`msg != "/compact"`) | none |
| **Slash-skill citation + system-prompt injection** | ~5646-5652, 4359-4399, 5683-5688 | **MISSING — deferred** | pi never calls `resolve_slash_skill`; `cited_count` not bumped, draft→promoted skipped, and the skill instructions aren't injected into the engine prompt | see below |
| **Failure memory recording** (FailureRecord → proactive avoidance) | ~6580-6604 | **MISSING — deferred** | needs the turn's failure outcome, which the engine handles internally; belongs in `EventSink`, not the pre-dispatch branch | see below |
| **Preference extraction** (user+assistant → learned prefs) | ~6606-6620 | **MISSING — deferred** | needs the assistant RESPONSE, which arrives async via `EventSink::persist_assistant`, not synchronously before the pi `return` | see below |

## Done in this PR

**`publish_incoming` wired into the pi branch** (before `return Ok(())`), mirroring the
legacy call. Fire-and-forget, safe (pi branch did not call it, so no double-fire). Restores
ProactiveService visibility (conversation_learning / skill_extraction / gene candidates) for
pi-engine turns.

## Deferred (with rationale — NOT wired, to avoid broken/half behavior)

These are genuinely missing for pi turns but cannot be correctly fixed by copying into the
pre-dispatch branch:

1. **Slash-skill citation + injection.** Two coupled effects: (a) bump `cited_count` /
   auto-promote a learned skill when the user invokes `/slash-skill`, and (b) inject the
   skill's prompt so the model actually applies it. (b) requires feeding the skill text into
   the engine's prompt (`EngineCmd::Prompt.input`/`context`), not just inserting an
   `agent_messages` system row the engine won't read. Wiring only (a) records a citation for
   a skill that wasn't actually applied — worse than skipping. **Plan:** resolve the slash
   skill before building the pi prompt and fold its instructions into the prompt context,
   then bump the citation. Own PR.

2. **Failure memory + 3. Preference extraction.** Both run *after* the turn completes and
   need the assistant response / failure outcome. In the pi path the turn is async and its
   result surfaces via `EventSink::persist_assistant` (`engine_sink.rs`), not synchronously
   in `send_agent_message`. Copying them before the `return` would run them with no response
   to learn from. **Plan:** invoke preference extraction + failure recording from
   `EventSink::persist_assistant` (where the user msg + assistant response + outcome are in
   hand), guarded so the legacy path doesn't double-run them. Needs `EventSink` to hold the
   `pref_extractor` + failure recorder; verify against any equivalent the engine already
   does. Own PR.

## Guardrail

The recurring root cause is "wire per-turn features into the pi path, not just the legacy
tail of `send_agent_message`." Any NEW per-turn side-effect must be added to the pi branch
(or `EventSink` for post-turn effects), not only after the pi `return`.
