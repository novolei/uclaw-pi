# GbrainAdapter (piece 3b) — Design

**Date:** 2026-06-01
**Status:** Approved (mapping); implementation pending
**Scope:** wrap the typed `gbrain::browse` client behind `MemoryAdapter`; additive

## Goal

Ship `GbrainAdapter`, a `MemoryAdapter` over gbrain (the primary durable-knowledge wiki), so gbrain's semantic search becomes reachable through the unified registry. With piece 2 (merged), `MemoryRecallConfig.prompt_recall_backend = "gbrain"` then gives the agent prompt recall over the agent's curated knowledge wiki.

Additive — registers a `"gbrain"` backend, changes nothing else. Default stays `legacy_kv`. Off main, like #36.

## Approach

Wrap the **existing typed `gbrain::browse` client** (`src-tauri/src/gbrain/browse.rs`) — which already wraps the `mcp__gbrain__*` MCP tools with typed async fns + a `parse_*`/IO split — not raw MCP. The adapter holds a `SharedMcpManager` (= `Arc<RwLock<McpManager>>`, `AppState.mcp_manager`).

**Two notes vs MemUAdapter:**
1. **More KV-capable.** gbrain has `get_page` (get-by-slug), so `recall`/`store`/`list`/**`get`** are all real; only `delete`/`clear_namespace`/`namespace_summaries` are minimal-with-warn (gbrain has no delete/bulk tool).
2. **Registration is UNCONDITIONAL.** gbrain is an MCP server that can connect *after* boot (unlike memU's boot-time `Option`). So register always; each call checks availability via `browse::` (`GbrainError::NotConnected` when offline). piece-2's `recall_adapter_block` degrades any error to `None`, so an offline gbrain yields no supplement — and gbrain connecting late Just Works.

## Mapping (8 methods → `gbrain::browse`)

| Trait method | gbrain `browse::` |
|---|---|
| `name()` | `"gbrain"` |
| `recall(query, limit, opts)` | `search(&mcp, query, limit as u32, 0)` → `Vec<SearchHit>` → `MemoryEntry` |
| `store(ns, key, content, _, _)` | `put_page(&mcp, &slug, content)`, `slug = slugify(ns, key)` |
| `list(ns, _cat, sid)` | `list_pages(&mcp, 50, None, None, None, None)` → `Vec<PageSummary>` → `MemoryEntry` |
| `get(ns, key)` | `get_page(&mcp, &slugify(ns,key))` → `PageDetail` → `MemoryEntry`; any error → `Ok(None)` |
| `delete` / `clear_namespace` / `namespace_summaries` | minimal-with-warn (`false`/`0`/`[]`) |

**Conventions:**
- `slugify(ns, key) -> String`: `format!("{ns}-{key}")` lowercased, non-`[a-z0-9]` → `-`, collapse repeats, trim `-`. (kebab-case, gbrain's slug form.) A pure, tested fn.
- **`SearchHit → MemoryEntry`** (pure, tested): `id`/`key` = `slug`; `content` = `snippet`; `namespace` from opts; `category: Core`; `timestamp: ""` (SearchHit has none); `session_id` from opts; `score: Some(similarity)`.
- **`PageSummary → MemoryEntry`**: `id`/`key` = `slug`; `content` = `title`; `timestamp` = `updated_at.unwrap_or_default()`; `category: Core`; `score: None`.
- **`PageDetail → MemoryEntry`**: `id`/`key` = `slug`; `content` = `compiled_truth`; `timestamp` = `updated_at.unwrap_or_default()`; `category: Core`; `score: None`.
- **Errors:** `GbrainError` is a plain enum (no `Display`) → `anyhow::anyhow!("gbrain recall: {}", e.to_command_string())`.
- **`get` caveat:** `browse::get_page` uses `fuzzy: true`, so `get` is a fuzzy lookup (may return a near-slug). Acceptable for trait-completeness; the use case is `recall`. Any `get_page` error (incl. not-found) → `Ok(None)`.

## File structure

| File | Change |
|---|---|
| `src-tauri/src/memory_adapter/gbrain_adapter.rs` (new) | `GbrainAdapter` + trait impl + pure mapping fns + unit tests |
| `src-tauri/src/memory_adapter/mod.rs` | `mod gbrain_adapter; pub use gbrain_adapter::GbrainAdapter;` |
| `src-tauri/src/app.rs` | register `GbrainAdapter::new(mcp_manager.clone())` under `"gbrain"` (unconditional, beside the others) |

## Non-goals

- No KV-addressing emulation for delete/clear (minimal-with-warn; gbrain has no delete tool).
- No default flip; no change to `recall_adapter_block`/`load_context` (piece 2 routes by name).
- No modification of `gbrain::browse` (e.g. `get_page` stays `fuzzy`). No new deps.

## Testing

- **Unit (pure, no live gbrain):** `slugify` (spaces/punct/case → kebab; idempotent-ish); `hit_to_entry` (slug→id/key, snippet→content, similarity→score, category Core); `summary_to_entry` / `detail_to_entry` (content source + timestamp). The `browse::`-calling methods (`recall`/`store`/`list`/`get`) are **integration-only** (need a live gbrain MCP server) — noted in the PR.
- `cargo build` green; `cargo clippy` clean; full `cargo test --lib` no new failures vs the environmental baseline.

## Risks

- **gbrain offline / late connect:** every call returns `GbrainError::NotConnected` → mapped to `Err` (or `Ok(None)` for `get`); recall_adapter_block degrades to `None`. Registration is unconditional, so a late-connecting gbrain becomes usable without a restart.
- **Wiki pollution from `store`:** adapter `store` writes `{ns}-{key}` slugged pages into the agent's wiki. Acceptable (namespaced by slug prefix); recall still searches all pages. If undesirable later, `store` can be downgraded to minimal — out of scope now.
- **Fuzzy `get`:** documented; recall is the real path.

## Future

A later pass could route the prompt's *primary* graph recall through gbrain (the deeper convergence), but that's the lossy-interface problem from piece 2's design — out of scope. This adapter just adds gbrain as a supplementary recall source.
