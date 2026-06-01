# `load_context` Memory-Context Seam — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans. Steps use `- [ ]`.

**Goal:** Extract the duplicated agent-prompt memory-context assembly (two sites in `tauri_commands.rs`) into one tested function `agent::memory_context::load_context`. Behavior-preserving.

**Architecture:** New module `src-tauri/src/agent/memory_context.rs`: a pure `compose_memory_context` helper (unit-tested) + an async `load_context` taking narrow inputs (so it serves both the `AppState` path and the no-`AppState` background path) and returning `{ context: Option<String>, recall_event: Option<Value> }`. The two call sites are rewritten to build their engine/browser-ctx, call `load_context`, then site-specifically emit + (set | cache).

**Tech Stack:** Rust, `serde_json`, `chrono`, `tracing`. No new deps.

**Spec:** `docs/superpowers/specs/2026-06-01-load-context-seam-design.md`.

---

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/agent/memory_context.rs` (new) | `MemoryContextInputs`, `LoadedMemoryContext`, `compose_memory_context` (pure), `load_context` (async), tests |
| `src-tauri/src/agent/mod.rs` (modify) | `pub mod memory_context;` |
| `src-tauri/src/tauri_commands.rs` (modify ≈1916-2035) | Site 1 → `load_context` |
| `src-tauri/src/tauri_commands.rs` (modify ≈5826-5956) | Site 2 → `load_context` |

---

### Task 1: New module — types + `compose_memory_context` (pure) + unit tests

**Files:** Create `src-tauri/src/agent/memory_context.rs`; modify `src-tauri/src/agent/mod.rs`.

- [ ] **Step 1: Write the module with the pure helper + types (no `load_context` yet).**

```rust
//! `load_context` — the single seam that assembles the agent prompt's memory
//! block. Consolidates the previously-duplicated assembly in `tauri_commands`
//! (the main send path + the background recall task). Behavior-preserving:
//! same three sources (graph recall + session KV + browser-task memory), same
//! `agent:memory-recall` event, same `record_used_skills` side-effect.

use serde_json::Value;

use crate::memory::MemoryStore;
use crate::memory_graph::recall::MemoryRecallEngine;

/// Narrow inputs both call sites can supply (the background task has no
/// `AppState`). The caller pre-builds `recall_engine` and `browser_ctx`.
pub struct MemoryContextInputs<'a> {
    pub recall_engine: &'a MemoryRecallEngine,
    pub memory_store: &'a MemoryStore,
    pub space_id: &'a str,
    /// Used for BOTH the `session:<id>` namespace and the event `conversationId`.
    pub conversation_id: &'a str,
    pub query: &'a str,
    /// Pre-computed per site (the two sites use different browser-memory fns).
    pub browser_ctx: Option<String>,
}

/// Result of [`load_context`]. The caller emits `recall_event` with its own
/// `app_handle` and routes `context` to `set_memory_context` (or the cache).
pub struct LoadedMemoryContext {
    pub context: Option<String>,
    pub recall_event: Option<Value>,
}

/// Concatenate the three optional blocks in the canonical order
/// (graph → session → browser). Returns `None` when nothing is present.
/// This is the pure core; `load_context` produces the blocks and calls it.
fn compose_memory_context(
    graph_block: Option<String>,
    session_block: Option<&str>,
    browser_block: Option<&str>,
) -> Option<String> {
    let mut out = graph_block.unwrap_or_default();
    if let Some(s) = session_block {
        out.push_str(s);
    }
    if let Some(b) = browser_block {
        out.push_str(b);
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_orders_graph_session_browser() {
        let out = compose_memory_context(
            Some("<g>\n".into()),
            Some("<s>\n"),
            Some("<b>\n"),
        )
        .unwrap();
        assert_eq!(out, "<g>\n<s>\n<b>\n");
    }

    #[test]
    fn compose_all_none_is_none() {
        assert!(compose_memory_context(None, None, None).is_none());
    }

    #[test]
    fn compose_session_only_when_no_graph() {
        let out = compose_memory_context(None, Some("<s>\n"), None).unwrap();
        assert_eq!(out, "<s>\n");
    }

    #[test]
    fn compose_browser_only() {
        let out = compose_memory_context(None, None, Some("<b>\n")).unwrap();
        assert_eq!(out, "<b>\n");
    }

    #[test]
    fn compose_empty_graph_string_with_no_aux_is_none() {
        // A graph_block of "" (defensive) + no aux → None, not Some("").
        assert!(compose_memory_context(Some(String::new()), None, None).is_none());
    }
}
```

- [ ] **Step 2: Register the module.** In `src-tauri/src/agent/mod.rs`, add `pub mod memory_context;` next to the other `pub mod` lines (verify exact location first).

- [ ] **Step 3: Build + run.**
Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib agent::memory_context::tests 2>&1 | tail -10`
Expected: 5 passed.

- [ ] **Step 4: Commit.**
```bash
git add src-tauri/src/agent/memory_context.rs src-tauri/src/agent/mod.rs
git commit -m "feat(agent): memory_context module — compose_memory_context helper (seam scaffold)"
```

---

### Task 2: `load_context` async fn

**Files:** Modify `src-tauri/src/agent/memory_context.rs`.

- [ ] **Step 1: Add `load_context`** above the `#[cfg(test)]` block. It mirrors the current inline logic exactly:

```rust
/// Assemble the prompt memory block from graph recall + session KV + the
/// caller-supplied browser context. On recall-plan failure, logs a warning and
/// returns empty (matches today's "proceed without memory context").
pub async fn load_context(inputs: MemoryContextInputs<'_>) -> LoadedMemoryContext {
    let plan = match inputs
        .recall_engine
        .build_recall_plan(inputs.space_id, inputs.query, false)
        .await
    {
        Ok(plan) => plan,
        Err(e) => {
            tracing::warn!(error = %e, "Memory recall failed, proceeding without memory context");
            return LoadedMemoryContext { context: None, recall_event: None };
        }
    };

    let total = plan.boot.len()
        + plan.triggered.len()
        + plan.relevant.len()
        + plan.expanded.len()
        + plan.recent.len();

    // Session-scoped memory (LIKE match) — independent of the graph total.
    let session_block: Option<String> = {
        let session_ns = format!("session:{}", inputs.conversation_id);
        let session_memories = inputs.memory_store.search(inputs.query, Some(&session_ns), 5);
        if session_memories.is_empty() {
            None
        } else {
            let mut ctx = String::from("<session_memories>\n");
            for m in &session_memories {
                ctx.push_str(&format!("- [{}] {}\n", m.kind, m.value));
            }
            ctx.push_str("</session_memories>\n");
            tracing::info!(session_memories = session_memories.len(), "Session-scoped memories injected");
            Some(ctx)
        }
    };

    if total > 0 {
        let budget = inputs.recall_engine.config().token_budget;
        let graph_block = MemoryRecallEngine::format_recall_for_prompt(&plan, budget);
        let context = compose_memory_context(
            Some(graph_block),
            session_block.as_deref(),
            inputs.browser_ctx.as_deref(),
        );

        let skills_count = plan
            .boot
            .iter()
            .chain(plan.triggered.iter())
            .chain(plan.relevant.iter())
            .chain(plan.expanded.iter())
            .filter(|c| c.kind == crate::memory_graph::models::MemoryNodeKind::Procedure)
            .count();
        let items: Vec<Value> = plan
            .boot
            .iter()
            .chain(plan.triggered.iter())
            .chain(plan.relevant.iter())
            .chain(plan.expanded.iter())
            .take(20)
            .map(|c| {
                serde_json::json!({
                    "nodeId": c.node_id,
                    "title": c.title,
                    "kind": c.kind,
                    "source": c.source,
                })
            })
            .collect();
        let recall_event = Some(serde_json::json!({
            "totalCandidates": total,
            "skillsCount": skills_count,
            "bootCount": plan.boot.len(),
            "triggeredCount": plan.triggered.len(),
            "relevantCount": plan.relevant.len(),
            "expandedCount": plan.expanded.len(),
            "recentCount": plan.recent.len(),
            "items": items,
            "conversationId": inputs.conversation_id,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));

        inputs.recall_engine.record_used_skills(&plan);
        if context.is_some() {
            tracing::info!(total_candidates = total, "Memory recall injected into system prompt");
        }
        LoadedMemoryContext { context, recall_event }
    } else {
        let context =
            compose_memory_context(None, session_block.as_deref(), inputs.browser_ctx.as_deref());
        LoadedMemoryContext { context, recall_event: None }
    }
}
```

- [ ] **Step 2: Verify the exact symbols** before/while writing: `MemoryRecallEngine::{build_recall_plan, config, format_recall_for_prompt, record_used_skills}`, `plan.{boot,triggered,relevant,expanded,recent}` (Vec of candidates with `.node_id/.title/.kind/.source`), `MemoryNodeKind::Procedure`, `MemoryStore::search(query, Some(ns), n)` returning items with `.kind/.value`. Adjust to the real signatures (read `memory_graph/recall.rs` + `memory.rs`).

- [ ] **Step 3: Build.**
Run: `cargo build --manifest-path src-tauri/Cargo.toml --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors.

- [ ] **Step 4: Commit.**
```bash
git add src-tauri/src/agent/memory_context.rs
git commit -m "feat(agent): load_context — assemble prompt memory from graph+session+browser"
```

---

### Task 3: Rewrite Site 1 (main path, `tauri_commands.rs` ≈1916-2035)

**Files:** Modify `src-tauri/src/tauri_commands.rs`.

- [ ] **Step 1:** Replace the whole `match recall_engine.build_recall_plan(...) { Ok(plan) => { …big block… } Err(e) => { warn } }` (≈1935-2035) with:

```rust
        let loaded = crate::agent::memory_context::load_context(
            crate::agent::memory_context::MemoryContextInputs {
                recall_engine: &recall_engine,
                memory_store: &state.memory_store,
                space_id: &space_id,
                conversation_id: &input.conversation_id,
                query: &input.content,
                browser_ctx: build_browser_task_memory_context(&state, &input.content),
            },
        )
        .await;
        if let Some(ev) = loaded.recall_event {
            let _ = app_handle.emit("agent:memory-recall", ev);
        }
        if let Some(ctx) = loaded.context {
            delegate.set_memory_context(ctx);
        }
```

Keep the surrounding `{ let recall_store = …; let recall_engine = MemoryRecallEngine::new(…); … }` block that builds `recall_engine` (lines ≈1918-1934) — only the `match … .await { … }` body is replaced.

- [ ] **Step 2: Build + targeted check.**
Run: `cargo build --manifest-path src-tauri/Cargo.toml --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors. (Watch for now-unused imports/vars at the call site — remove only what this edit orphaned.)

- [ ] **Step 3: Commit.**
```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "refactor(tauri_commands): route main-path memory assembly through load_context"
```

---

### Task 4: Rewrite Site 2 (background task, `tauri_commands.rs` ≈5826-5956)

**Files:** Modify `src-tauri/src/tauri_commands.rs`.

- [ ] **Step 1:** Inside the `tokio::spawn`, replace the `let composed: Option<String> = match recall_engine.build_recall_plan(...) { … }` block with:

```rust
            let loaded = crate::agent::memory_context::load_context(
                crate::agent::memory_context::MemoryContextInputs {
                    recall_engine: &recall_engine,
                    memory_store: &memory_store_for_recall,
                    space_id: "default",
                    conversation_id: &session_id_for_recall,
                    query: &user_msg_for_recall,
                    browser_ctx: browser_task_memory_for_query(&memory_store_for_recall, &user_msg_for_recall),
                },
            )
            .await;
            if let Some(ev) = loaded.recall_event {
                let _ = app_handle_for_recall.emit("agent:memory-recall", ev);
            }
            // Keep the not-yet-wired handles alive (future expansion).
            let _ = (&state_db_for_browser, &workspace_root_for_browser);
            let composed: Option<String> = loaded.context;
```

Leave the cache-stash + `recall_tx.send(composed)` (lines ≈5965-5987) unchanged.

- [ ] **Step 2: Build.**
Run: `cargo build --manifest-path src-tauri/Cargo.toml --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors.

- [ ] **Step 3: Commit.**
```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "refactor(tauri_commands): route background recall through load_context"
```

---

### Task 5: Verification

- [ ] **Step 1: Full module tests.** `cargo test --manifest-path src-tauri/Cargo.toml --lib agent::memory_context` → all pass.
- [ ] **Step 2: Regression.** `cargo test --manifest-path src-tauri/Cargo.toml --lib 2>&1 | tail -8` → no NEW failures vs the known-environmental baseline (the 5 browser/shell env failures from PR9's run are pre-existing and unrelated).
- [ ] **Step 3: Clippy on touched files.** `cargo clippy --manifest-path src-tauri/Cargo.toml --lib 2>&1 | grep -E "memory_context\.rs|tauri_commands\.rs" | head` → no new hits attributable to this change.
- [ ] **Step 4: Extraction-fidelity check.** Diff the two new call sites against the pre-refactor blocks (this PR's `git diff`) to confirm the moved logic is byte-equivalent (same source order, same event fields, same `record_used_skills` placement, same set-vs-cache).
- [ ] **Step 5: No-new-deps + dedup confirmation.** `git diff main -- src-tauri/Cargo.toml` empty; net line count down at the call sites.

---

## Self-Review

- **Spec coverage:** module + `compose` helper + `load_context` + both call-site rewrites + tests — all spec sections covered.
- **No placeholders:** every step has concrete code; Task 2 Step 2 flags the real-signature verification (the one place where the inline code must be matched exactly).
- **Type consistency:** `MemoryContextInputs`/`LoadedMemoryContext` field names match across Tasks 1-4; `load_context` signature identical at both call sites.
- **Behavior preservation:** the recall-plan `Err` warn, the `total>0` vs else split, `record_used_skills`, and the event payload all relocated verbatim; site-specific browser fn / app_handle / set-vs-cache stay at the call site.
