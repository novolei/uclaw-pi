# `load_context` Memory-Context Seam — Design

**Date:** 2026-06-01
**Status:** Approved (design); implementation pending
**Scope:** consolidate-only (no behavior change, no `MemoryAdapter` routing yet)

## Goal

Collapse the ~80-line agent-prompt memory-context assembly — currently **duplicated** across two call sites in `src-tauri/src/tauri_commands.rs` — into one tested function, `load_context`, establishing a single seam. This is the structural pre-work for the later "route the prompt's memory through the unified `MemoryAdapter`" change: once the seam exists, swapping memory sources touches **one function body** instead of two scattered call sites.

This slice changes **no behavior**. It is a pure refactor.

## Background — current state

The agent prompt's memory block is produced by composing **three sources** into one string, then handed to the dispatcher via `delegate.set_memory_context(String)` (the single injection seam, `agent/dispatcher/content_assembler.rs:479`, which wraps it in `<memory_context>…</memory_context>` in the per-turn dynamic block):

1. **Graph recall** — `MemoryRecallEngine::build_recall_plan(space, query)` → `format_recall_for_prompt(&plan, budget)` (the 5-layer boot/triggered/relevant/expanded/recent block, ~5000-token budget). `memory_graph/recall.rs`.
2. **Session memories** — `memory_store.search(query, Some("session:<id>"), 5)` → a `<session_memories>` block.
3. **Browser-task memory** — a heuristic match on the user message.

Plus two side-effects when `total > 0`: emit an `agent:memory-recall` observability event, and `recall_engine.record_used_skills(&plan)`.

This block is duplicated at **two call sites** that are *structurally parallel but differ in mechanics*:

| | Site 1 (`tauri_commands.rs` ≈1930, main path) | Site 2 (`tauri_commands.rs` ≈5827, background `tokio::spawn`) |
|---|---|---|
| Handles | full `&AppState` | narrow cloned handles (no `AppState`) |
| Browser memory fn | `build_browser_task_memory_context(&state, q)` | `browser_task_memory_for_query(&memory_store, q)` (narrow re-impl) |
| Outcome | `delegate.set_memory_context(ctx)` directly | compose → cache into `recall_ctx_cache` for the next turn |
| Event `conversationId` | `input.conversation_id` | `session_id_for_recall` |

The **common core** (build plan → `total` → `format_recall_for_prompt` + session search + append browser ctx → build event + `record_used_skills`) is identical and is what we extract. The differing parts (which browser fn, which `app_handle`, set-vs-cache) stay at the call site.

The unified `memory_adapter` layer (`memory_unified_*` IPC) does **not** participate in the prompt path today and is **out of scope** here.

## Design

A new module `src-tauri/src/agent/memory_context.rs` exposing one async function over **narrow common inputs** — so it serves both the `AppState` path and the no-`AppState` background path — returning a structured result; Tauri/`AppState`-specific bits stay at the caller.

```rust
pub struct MemoryContextInputs<'a> {
    /// Pre-built by the caller (the build deps differ per site).
    pub recall_engine: &'a crate::memory_graph::recall::MemoryRecallEngine,
    pub memory_store: &'a crate::memory::MemoryStore,
    pub space_id: &'a str,
    /// Used for BOTH the `session:<id>` namespace and the event `conversationId`
    /// (verified identical at both sites).
    pub conversation_id: &'a str,
    pub query: &'a str,
    /// Pre-computed per site (Site 1 vs Site 2 use different fns), so the seam
    /// stays free of `AppState`.
    pub browser_ctx: Option<String>,
}

pub struct LoadedMemoryContext {
    /// The assembled `<…>` block, or `None` when nothing was recalled.
    pub context: Option<String>,
    /// Emit-ready `agent:memory-recall` payload, or `None` when `total == 0`.
    /// The caller emits it with its own `app_handle`.
    pub recall_event: Option<serde_json::Value>,
}

pub async fn load_context(inputs: MemoryContextInputs<'_>) -> LoadedMemoryContext;
```

**Inside `load_context`** (byte-for-byte the current logic, just relocated):
- `recall_engine.build_recall_plan(space_id, query, false).await`; on `Err`, log a warning and return `LoadedMemoryContext { context: None, recall_event: None }` (same as today's "proceed without memory context").
- `total = boot+triggered+relevant+expanded+recent`.
- Session search → optional `<session_memories>` block.
- If `total > 0`: `format_recall_for_prompt(&plan, engine.config().token_budget)` + append session + append `browser_ctx`; build the `recall_event` JSON (totalCandidates/skillsCount/boot/triggered/relevant/expanded/recent counts + up-to-20 items + `conversationId` + timestamp); call `recall_engine.record_used_skills(&plan)`.
- Else: concat session + `browser_ctx`; `recall_event = None`.
- `context = Some(s)` iff the assembled string is non-empty, else `None`.

**Each call site becomes** (≈10 lines): build its `recall_engine`, compute its own `browser_ctx`, call `load_context`, then site-specifically `emit(recall_event)` + (`set_memory_context` | cache).

**Decomposition for testability:** the pure string composition — `compose_memory_context(graph_block: Option<String>, session_block: Option<String>, browser_block: Option<String>) -> Option<String>` (ordering + empty-handling) — is split into a private helper and unit-tested directly (no engine/store needed). `load_context` itself gets one integration-style test over a temp `MemoryStore` + a minimal in-memory recall engine asserting the no-recall path returns `None`/`None` and a seeded session memory surfaces in `context`.

## Non-goals

- **No behavior change.** The produced string + event must match the current output for the same inputs.
- **No `MemoryAdapter` routing.** Sources stay exactly as today (graph recall + KV session + browser heuristic).
- **No change to the two browser-memory fns**, the `recall_ctx_cache` mechanism, or `set_memory_context`/`content_assembler`.
- No new dependencies.

## Testing

- `cargo test --lib agent::memory_context` — unit tests for `compose_memory_context` (all 8 present/absent combinations, ordering, empty→`None`) + the `load_context` integration test.
- `cargo build` green; `cargo test --lib` shows no regression in the existing send/stream paths.
- Manual reasoning checkpoint: diff the extracted body against both original sites to confirm byte-identical logic (the chief safety mechanism for a behavior-preserving refactor).

## Risks & mitigations

- **Silent behavior drift between the two sites.** The sites differ subtly (event `conversationId` source, set-vs-cache). Mitigation: the differing bits stay at the call site; only the verified-identical core moves. Confirm the session-namespace id == event `conversationId` at *both* sites before extracting.
- **`record_used_skills` / event ordering.** Must remain inside the `total > 0` branch, after composition, exactly as today.
- **Background path is in a `tokio::spawn`.** `load_context` must be `Send`-friendly (no non-`Send` state held across its internal `.await`). The inputs are `&` refs + owned `String`; fine.

## Future (enabled by this seam, out of scope here)

Once `load_context` is the single seam, a follow-up can swap the graph-recall source for `MemoryAdapter::recall` (routing through `legacy_steward` + `bucket_seal`) by editing only this function — the call sites and `content_assembler` stay untouched.
