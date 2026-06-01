# MemUAdapter (piece 3a) — Design

**Date:** 2026-06-01
**Status:** Approved (mapping); implementation pending
**Scope:** wrap `MemUClient` behind `MemoryAdapter`; additive, conditional registration

## Goal

Ship `MemUAdapter`, a `MemoryAdapter` over the existing memU service (`MemUClient`), so memU's **semantic recall** becomes reachable through the unified registry. Combined with piece 2, pointing `MemoryRecallConfig.prompt_recall_backend = "memu"` then gives the agent prompt semantic recall through one uniform path.

Additive — registers a new `"memu"` backend, changes nothing else. The default backend stays `legacy_kv`.

## Two realities

1. **Recall-focused (impedance mismatch).** memU is semantic/episodic: `create_item` generates ids, `retrieve` is semantic search, `delete_item` is by-id. It has **no stable `(namespace, key)` addressing**, which the trait's `get`/`delete`/`clear_namespace` assume. The use case (`prompt_recall_backend = "memu"`) only exercises `recall`. So `recall`/`store`/`list` are real; the four KV-only ops are **minimal-with-warn** (approved) — we don't fake KV semantics memU lacks.
2. **Hard to unit-test.** `MemUClient` wraps a live Python subprocess (the memU bridge), absent in tests/CI. So only the **pure `Item → MemoryEntry` mapping** is unit-tested; the bridge-calling methods need a live memU (manual/integration). Registration is conditional on `state.memu_client` being `Some`.

## Mapping (8 methods → `MemUClient`)

| Trait method | memU mapping |
|---|---|
| `name()` | `"memu"` |
| `recall(query, limit, opts)` | `retrieve_with_context(query, memory_types=None, limit, include_categories=false)` → `Vec<EnrichedMemoryItem>` → `MemoryEntry`. (Typed; `include_categories=false` skips the LLM enrichment so it stays fast; errors → the call's `Err`.) |
| `store(ns, key, content, cat, sid)` | `create_item(memory_type, content, [category-name], user_scope)`. `key` is not a memU concept — it is dropped (documented). |
| `list(ns, cat, sid)` | `list_items(category, memory_type=None, limit=Some(50), offset=0, user_scope)` → `Vec<Value>` → `MemoryEntry` (defensive parse) |
| `get(ns, key)` | **minimal:** `Ok(None)` + `tracing::debug!` ("memU is semantic; get-by-key unsupported") |
| `delete(ns, key)` | **minimal:** `Ok(false)` + `tracing::debug!` ("delete-by-key unsupported; memU deletes by item id") |
| `clear_namespace(ns)` | **minimal:** `Ok(0)` + `tracing::debug!` |
| `namespace_summaries()` | **minimal:** `Ok(vec![])` (memU categories ≠ namespaces) |

**Conventions:**
- `user_scope`: `opts.namespace`/`namespace` → `Some(json!({"user_id": ns}))`, else `None` (global).
- **category ↔ memU type** (a small pure fn each direction):
  - `MemoryCategory → memory_type`: Core→`"knowledge"`, Conversation→`"event"`, Daily→`"event"`, Custom(s)→`s`.
  - `memory_type → MemoryCategory`: `"knowledge"|"profile"`→Core, `"event"`→Conversation, else Custom(memory_type).
- **`EnrichedMemoryItem → MemoryEntry`** (the pure, tested mapping): `content`→content; `memory_type`→category; `relevance_score`→`score: Some(_)`; `created_at`→`timestamp` (or `""`); `id` from `metadata.get("id")` as str else `""`; `key: ""`; `namespace`/`session_id` from `opts`.

## File structure

| File | Change |
|---|---|
| `src-tauri/src/memory_adapter/memu_adapter.rs` (new) | `MemUAdapter` + the trait impl + the two pure mapping fns + unit tests |
| `src-tauri/src/memory_adapter/mod.rs` | `mod memu_adapter;` + `pub use memu_adapter::MemUAdapter;` |
| `src-tauri/src/app.rs` | register `MemUAdapter` under `"memu"` **iff** `memu_client.is_some()` |

(Lives in `memory_adapter/` beside `legacy_kv`/`legacy_steward`, which likewise wrap other modules' stores; uses `super::{traits,types}`.)

## Non-goals

- No KV-addressing emulation (get/delete/clear are minimal-with-warn).
- No default-backend flip; no change to `recall_adapter_block`/`load_context` (piece 2 already routes by name).
- No new dependencies. memU's own write path (`MemorizationService`) is untouched — `store` is an additional, optional direct-write path.

## Testing

- **Unit (pure, no live memU):** `enriched_to_entry` (build an `EnrichedMemoryItem` incl. a `metadata.id`, assert the `MemoryEntry` fields incl. `score`/`category`/`timestamp`); `value_to_entry` for a `list_items` JSON shape; `category ↔ memory_type` round-trip; `name() == "memu"` (needs an adapter instance — construct with a `MemUClient` over a non-started bridge, only calling `name()` which doesn't touch the bridge — OR make the mapping fns free functions tested without an adapter instance).
- The bridge-calling methods (`recall`/`store`/`list`) are **integration-only** (need a live memU) — covered manually, noted in the PR.
- `cargo build` green; `cargo clippy` clean; full `cargo test --lib` no new failures vs the environmental baseline.

## Risks

- **Untyped `list_items` items:** defensive parse with field fallbacks (`content`/`memory_content`, `id`, `categories`/`memory_type`, `created_at`); missing fields → sensible defaults, never panic.
- **memU latency / unavailability:** `recall` errors propagate as `Err`; piece 2's `recall_adapter_block` already degrades any adapter error to `None` (never fails the turn). So a slow/absent memU just yields no supplement.
- **`MemUAdapter::name()` test needs an instance:** if constructing `MemUClient` for a test is awkward, keep the mapping logic in free functions so the meaningful tests need no client; test `name()` via a `#[ignore]` integration test or skip it.

## Future

memU's `embed_text` (FastEmbed, 384-dim) could later back a real `Embedder` for bucket_seal (the deferred PR9 follow-up) — out of scope here.
