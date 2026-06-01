# Prompt Adapter-Recall Supplement (piece 2) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: subagent-driven-development or executing-plans. Steps use `- [ ]`.

**Goal:** `load_context` optionally appends a `<recalled_memories>` block from the unified `MemoryAdapter` (via `route_recall_in`), opt-in via a new setting, off by default (zero behavior change). Supplements — never replaces — the graph recall.

**Spec:** `docs/superpowers/specs/2026-06-01-load-context-adapter-recall-design.md`. **Depends on** the `load_context` seam (PR #34).

---

## File Structure

| File | Change |
|---|---|
| `src-tauri/src/memory_graph/recall.rs` | `MemoryRecallConfig` + DTO + both `From` impls + `Default` gain `prompt_recall_backend: Option<String>` + `prompt_recall_limit: usize` |
| `src-tauri/src/agent/memory_context.rs` | `AdapterRecall` struct, `recall_adapter_block`, `compose_memory_context` 4th block, `load_context` wiring, tests |
| `src-tauri/src/tauri_commands.rs` (Site 1) | build `adapter_recall` from `recall_engine.config()` |
| `src-tauri/src/tauri_commands.rs` (Site 2) | clone `memory_adapters` + default into the spawn; build `adapter_recall` |

---

### Task 1: Setting on `MemoryRecallConfig` (+ DTO)

**Files:** `src-tauri/src/memory_graph/recall.rs`.

- [ ] **Step 1:** Add two fields to `MemoryRecallConfig` (after `backlink_boost_weight`):
```rust
    /// When set (non-empty), load_context ALSO recalls from this MemoryAdapter
    /// backend and appends a <recalled_memories> block. None/empty = off (no
    /// behavior change). This is adapter-routing, not graph-recall tuning, but
    /// lives here because the config is already threaded to both recall sites.
    pub prompt_recall_backend: Option<String>,
    /// Max entries pulled for the adapter-recall supplement. Default 5.
    pub prompt_recall_limit: usize,
```
- [ ] **Step 2:** `Default` impl — add `prompt_recall_backend: None,` and `prompt_recall_limit: 5,`.
- [ ] **Step 3:** `MemoryRecallConfigDto` — add `#[serde(default)] pub prompt_recall_backend: Option<String>,` and `#[serde(default)] pub prompt_recall_limit: Option<usize>,`.
- [ ] **Step 4:** `From<MemoryRecallConfigDto> for MemoryRecallConfig` — add `prompt_recall_backend: dto.prompt_recall_backend.or(default.prompt_recall_backend),` and `prompt_recall_limit: dto.prompt_recall_limit.unwrap_or(default.prompt_recall_limit),`.
- [ ] **Step 5:** `From<MemoryRecallConfig> for MemoryRecallConfigDto` — add `prompt_recall_backend: cfg.prompt_recall_backend,` and `prompt_recall_limit: Some(cfg.prompt_recall_limit),`. (Read the existing tail of this impl to place them; mind the move of `cfg`.)
- [ ] **Step 6:** Build. `cargo build --manifest-path src-tauri/Cargo.toml --lib 2>&1 | grep -E "^error" | head` → zero errors.
- [ ] **Step 7:** Commit. `git add src-tauri/src/memory_graph/recall.rs && git commit -m "feat(memory_graph): MemoryRecallConfig.prompt_recall_backend setting (off by default)"`

---

### Task 2: `recall_adapter_block` + `AdapterRecall` + compose 4th block + load_context wiring

**Files:** `src-tauri/src/agent/memory_context.rs`.

- [ ] **Step 1:** Add imports + `AdapterRecall` struct near the top:
```rust
use std::collections::HashMap;
use std::sync::Arc;
use crate::memory_adapter::MemoryAdapter;

/// Optional adapter-recall supplement (off when `MemoryContextInputs.adapter_recall`
/// is None). AppState-free so the background path can use it too.
pub struct AdapterRecall<'a> {
    pub adapters: &'a HashMap<String, Arc<dyn MemoryAdapter>>,
    pub default_backend: &'a str,
    pub backend: &'a str,
    pub limit: usize,
}
```
- [ ] **Step 2:** Add the field to `MemoryContextInputs`:
```rust
    /// When `Some`, append a `<recalled_memories>` block from this backend.
    pub adapter_recall: Option<AdapterRecall<'a>>,
```
- [ ] **Step 3:** Extend `compose_memory_context` with a 4th `adapter_block: Option<&str>` (appended last, after browser); update the existing 5 tests to pass `None` as the 4th arg.
- [ ] **Step 4:** Add `recall_adapter_block`:
```rust
/// Recall from the configured adapter backend and format a bounded
/// `<recalled_memories>` block. Best-effort: any router/adapter error logs a
/// warning and yields `None` (never fails the turn).
async fn recall_adapter_block(ar: &AdapterRecall<'_>, query: &str) -> Option<String> {
    let opts = crate::memory_adapter::router::RecallOptsIpc::default();
    let hits = match crate::memory_adapter::router::route_recall_in(
        ar.adapters, ar.default_backend, Some(ar.backend), "global", query, ar.limit, &opts,
    )
    .await
    {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(backend = %ar.backend, error = %e, "adapter recall supplement failed; skipping");
            return None;
        }
    };
    if hits.is_empty() {
        return None;
    }
    let mut block = String::from("<recalled_memories>\n");
    for h in &hits {
        let snippet: String = h.content.chars().take(200).collect();
        block.push_str(&format!("- [{}] {}: {}\n", h.category, h.key, snippet));
    }
    block.push_str("</recalled_memories>\n");
    Some(block)
}
```
- [ ] **Step 5:** In `load_context`, after `session_block` is computed, add:
```rust
    let adapter_block: Option<String> = match &inputs.adapter_recall {
        Some(ar) => recall_adapter_block(ar, inputs.query).await,
        None => None,
    };
```
Pass `adapter_block.as_deref()` as the 4th arg to BOTH `compose_memory_context` calls (the `total>0` and `else` branches).
- [ ] **Step 6:** Tests — add to the module tests:
  - `recall_adapter_block_formats_hits`: build a stub `LegacyKvAdapter` in a `HashMap` (mirror `router.rs` test helpers: in-memory `MemoryStore` → `LegacyKvAdapter`), `store` an entry, call `recall_adapter_block` with `backend="legacy_kv"`, assert the block contains the key/content.
  - `recall_adapter_block_unknown_backend_is_none`: backend not in map → `None`.
  - `compose_appends_adapter_block_last`: 4-arg compose ordering.
- [ ] **Step 7:** Run. `cargo test --manifest-path src-tauri/Cargo.toml --lib agent::memory_context 2>&1 | tail -12` → all pass.
- [ ] **Step 8:** Commit. `git add src-tauri/src/agent/memory_context.rs && git commit -m "feat(agent): load_context adapter-recall supplement (off by default)"`

---

### Task 3: Site 1 wiring (main path)

**Files:** `src-tauri/src/tauri_commands.rs` (the `load_context` call added in PR #34, ≈1936).

- [ ] **Step 1:** Just before the `load_context` call, read the setting via the engine (since `recall_config` was moved into the engine) + the default backend, and build `adapter_recall`:
```rust
        let prompt_backend = recall_engine.config().prompt_recall_backend.clone();
        let prompt_limit = recall_engine.config().prompt_recall_limit;
        let default_backend_str = state
            .default_memory_backend
            .read()
            .ok()
            .map(|g| g.clone())
            .unwrap_or_else(|| "legacy_kv".to_string());
        let adapter_recall = prompt_backend
            .as_deref()
            .filter(|b| !b.is_empty())
            .map(|backend| crate::agent::memory_context::AdapterRecall {
                adapters: &state.memory_adapters,
                default_backend: &default_backend_str,
                backend,
                limit: prompt_limit,
            });
```
Then add `adapter_recall,` to the `MemoryContextInputs { … }` literal.
- [ ] **Step 2:** Build → zero errors. Commit `git add src-tauri/src/tauri_commands.rs && git commit -m "feat(tauri_commands): wire adapter-recall supplement on main path"`

---

### Task 4: Site 2 wiring (background task)

**Files:** `src-tauri/src/tauri_commands.rs` (the background `tokio::spawn`, ≈5750).

- [ ] **Step 1:** Before the `tokio::spawn(async move { … })`, add two captures alongside the other `*_for_recall` clones:
```rust
        let memory_adapters_for_recall = state.memory_adapters.clone();
        let default_backend_for_recall = state
            .default_memory_backend
            .read()
            .await
            .clone();
```
(If `default_memory_backend` is a `std::sync::RwLock`, use `.read().ok().map(|g| g.clone()).unwrap_or_else(|| "legacy_kv".to_string())` instead of `.await` — verify which at build time.)
- [ ] **Step 2:** Inside the spawn, just before the `load_context` call, build `adapter_recall` from the engine config + the captured handles:
```rust
            let prompt_backend = recall_engine.config().prompt_recall_backend.clone();
            let prompt_limit = recall_engine.config().prompt_recall_limit;
            let adapter_recall = prompt_backend
                .as_deref()
                .filter(|b| !b.is_empty())
                .map(|backend| crate::agent::memory_context::AdapterRecall {
                    adapters: &memory_adapters_for_recall,
                    default_backend: &default_backend_for_recall,
                    backend,
                    limit: prompt_limit,
                });
```
Add `adapter_recall,` to the `MemoryContextInputs { … }` literal.
- [ ] **Step 3:** Build → zero errors (watch the `Send`-ness of the spawn future; `AdapterRecall` holds `&` to spawn-owned `memory_adapters_for_recall`/`default_backend_for_recall`, both `move`d in). Commit `git add src-tauri/src/tauri_commands.rs && git commit -m "feat(tauri_commands): wire adapter-recall supplement on background path"`

---

### Task 5: Verification

- [ ] `cargo test --manifest-path src-tauri/Cargo.toml --lib agent::memory_context` → all pass.
- [ ] Full `cargo test --manifest-path src-tauri/Cargo.toml --lib 2>&1 | tail -8` → no NEW failures vs the 5 known-environmental baseline.
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --lib 2>&1 | grep -E "memory_context\.rs|recall\.rs|tauri_commands\.rs"` → no new hits attributable to this change.
- [ ] `git diff main -- src-tauri/Cargo.toml` empty (no new deps).
- [ ] **Off-by-default guard:** confirm `prompt_recall_backend` defaults to `None` in `Default` + the DTO `From` → with no setting, both call sites pass `adapter_recall: None` → `load_context` unchanged.

---

## Self-Review

- **Spec coverage:** setting + `recall_adapter_block` + compose 4th block + load_context wiring + both call sites + tests — all covered.
- **No placeholders:** concrete code each step; Task 4 Step 1 flags the `RwLock` vs async-lock verification.
- **Type consistency:** `AdapterRecall` fields match `route_recall_in`'s params; `adapter_recall` field name identical across Tasks 2-4.
- **Off-by-default:** every path gated on a non-empty `prompt_recall_backend`; default `None` → zero behavior change (the regression guard).
