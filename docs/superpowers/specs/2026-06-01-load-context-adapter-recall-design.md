# Prompt Adapter-Recall Supplement (piece 2) — Design

**Date:** 2026-06-01
**Status:** Approved (Approach A); implementation pending
**Depends on:** `agent::memory_context::load_context` (the seam, PR #34)
**Scope:** opt-in, off by default — zero behavior change until explicitly enabled

## Goal

Make `load_context` pull supplementary memory from the unified `MemoryAdapter` registry, so the agent prompt gains "memory through the one handle" — **without** regressing the current rich 5-layer graph recall. Per the approved Approach A, the adapter recall is **appended as a 4th source** (alongside graph + session + browser), never replacing the graph block.

This is the user-visible payoff of the memory-layer convergence: once a good backend (memU semantic, bucket_seal FTS, gbrain) is the configured prompt-recall backend, the prompt recalls through it via one uniform path. It is **decoupled from which adapters exist** — the wiring lands now and works with whatever is registered; richer backends just get pointed at later.

## Background

- `load_context` (the seam) composes graph recall (`format_recall_for_prompt`) + `<session_memories>` + browser memory into one block. It takes **narrow inputs** (no `AppState`) so it serves both the main path and the no-`AppState` background task.
- The adapter router exposes an **AppState-free** entry point: `memory_adapter::router::route_recall_in(adapters: &HashMap<String, Arc<dyn MemoryAdapter>>, default_backend: &str, explicit_backend: Option<&str>, namespace: &str, query: &str, limit: usize, opts: &RecallOptsIpc) -> anyhow::Result<Vec<MemoryEntry>>`. This fits `load_context`'s narrow-inputs design — it needs only the adapters map + a couple of strings, not `AppState`.
- **The downgrade landmine** (why Approach A, not "replace"): `MemoryAdapter::recall` returns flat `Vec<MemoryEntry>`; the prompt's `format_recall_for_prompt` returns a rich multi-section block (skills SOPs + boot/triggered/relevant/expanded/recent + budgeting). Replacing the graph block with `recall()` would gut that. So we **supplement**, not replace.

## Design (Approach A)

### 1. Opt-in setting

A new optional field on `MemoryRecallConfig` (which both call sites already clone/thread):

```rust
/// When set, load_context ALSO recalls from this MemoryAdapter backend and
/// appends a <recalled_memories> block. None/empty = off (no behavior change).
pub prompt_recall_backend: Option<String>,
```

(Default `None`. A small companion `prompt_recall_limit: usize` with a default like 5 bounds the K.)

### 2. `load_context` gains one optional input

`MemoryContextInputs` gains a single optional bundle — present only when the feature is on:

```rust
pub struct AdapterRecall<'a> {
    pub adapters: &'a std::collections::HashMap<String, std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>>,
    pub default_backend: &'a str,
    pub backend: &'a str,   // the configured prompt_recall_backend
    pub limit: usize,
}
// in MemoryContextInputs:
pub adapter_recall: Option<AdapterRecall<'a>>,   // None = off → unchanged behavior
```

When `Some`, `load_context` calls a shared helper:

```rust
async fn recall_adapter_block(ar: &AdapterRecall<'_>, query: &str) -> Option<String>
```

which does `route_recall_in(ar.adapters, ar.default_backend, Some(ar.backend), namespace, query, ar.limit, &RecallOptsIpc::default())`, formats the returned `Vec<MemoryEntry>` into a bounded `<recalled_memories>` block (e.g. `- [{category}] {key}: {≤N-char content}` per entry), and returns `Some` iff non-empty. On router error (backend not found / adapter failed) it logs a warning and returns `None` (recall is best-effort supplementary — never fails the turn).

`namespace` for the supplement: a generic scope (e.g. `"global"`); the backend is selected explicitly via `Some(ar.backend)`, so `namespace` is only the recall filter. (Scoping to space/conversation is a later refinement.)

### 3. Composition

`compose_memory_context` gains the 4th block, appended **last** (supplementary, after graph/session/browser):

```
graph_block → session_block → browser_block → adapter_block
```

`load_context`'s `total>0` and `total==0` paths both append `adapter_block` when present (the supplement is independent of graph total — like session memory).

### 4. Call sites

Each site builds `adapter_recall: Option<AdapterRecall>` from its handles iff `cfg.prompt_recall_backend` is a non-empty string:
- **Site 1 (main):** `adapters = &state.memory_adapters`, `default_backend` from `state.default_memory_backend`, `backend = cfg.prompt_recall_backend`.
- **Site 2 (background):** clone `state.memory_adapters` (an `Arc<HashMap>`, cheap) + the default-backend string into the `tokio::spawn` before it starts; build `AdapterRecall` from those.

When the setting is `None`, both sites pass `adapter_recall: None` → `load_context` behaves exactly as today.

## Non-goals

- **No replacing** the graph recall (`format_recall_for_prompt` stays the primary source).
- **No default flip**, no new adapter. Works with whatever is registered (`legacy_kv`/`legacy_steward` today; `bucket_seal`/`memU`/`gbrain` as they land).
- No cross-source dedup between the graph block and the adapter block (v1 accepts possible overlap).
- No new dependencies.

## Testing

- `recall_adapter_block` unit test using stub adapters (the router tests already show the pattern: an in-memory `LegacyKvAdapter`): store an entry, set `backend`, assert the `<recalled_memories>` block contains it; unknown backend → `None` (no panic).
- `compose_memory_context` extended for the 4th block (ordering, all-absent → `None`).
- `load_context` with `adapter_recall: None` → byte-identical to today (regression guard).
- `cargo test --lib agent::memory_context`; full `cargo test --lib` shows no NEW failures vs the known-environmental baseline.

## Risks & mitigations

- **Latency:** an extra recall call per turn when enabled. Mitigated: off by default; the background path is off the critical 400ms deadline; `limit` is small; router errors degrade to `None`.
- **`Send` across the spawn:** Site 2 clones the `Arc<HashMap>` into the spawn; `AdapterRecall` holds `&` refs to spawn-owned data — fine.
- **Overlap with graph recall:** accepted for v1 (different stores; bounded K). Dedup is a future refinement.
- **Empty-supplement:** `route_recall_in` on an empty backend returns `Ok(vec![])` → `recall_adapter_block` returns `None` → composes to nothing. No empty block injected.

## Future (enabled, out of scope)

- Point `prompt_recall_backend` at `memU` (semantic) or `bucket_seal` (FTS) once those adapters register → the prompt gains semantic/keyword recall through the one handle.
- Eventually, move the graph recall itself behind the adapter (an enriched `recall_formatted` interface, the earlier "Approach C") as a separate effort — the seam makes that a one-function change.
