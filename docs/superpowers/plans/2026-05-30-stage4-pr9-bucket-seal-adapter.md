# 阶段 4 PR9 — `BucketSealAdapter` impl + FTS5 + default backend flip Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the first non-wrap `MemoryAdapter` implementation — `BucketSealAdapter` orchestrates the PR5-8 stack (chunks store + canonicalize + chunker + score + tree_source) behind the same 8-method trait that wraps LegacyKvAdapter/LegacyStewardAdapter. Add an FTS5 virtual table + sync triggers so `recall` does proper keyword ranking. Wire into `AppState.memory_adapters` and flip the default backend from `"legacy_kv"` (PR4's temporary holding) to `"bucket_seal"`.

**Architecture:** Single `memory_bucket_seal/adapter.rs` file holding `BucketSealAdapter` struct + 8 trait method impls. The adapter owns `Arc<BucketSealStore>`, a `content_root: PathBuf`, an `Arc<dyn Embedder>` (InertEmbedder for now), an `Arc<dyn Summariser>` (InertSummariser for now), and a `tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>` for per-tree append_leaf serialisation (mandatory per PR8's concurrency contract). `store()` flows: canonicalise(Document) → chunk_markdown → score_chunk per chunk → stage admitted ones + upsert_score → append_leaf. `recall()` uses `mem_tree_chunks_fts` virtual table with `MATCH` ranking. AppState::new builds the store + adapter at boot.

**Tech Stack:** Rust, `tokio::sync::Mutex`, `rusqlite` (bundled FTS5), `chrono`, `anyhow`, `async-trait`, `tracing`. No new workspace deps.

---

## Source-of-truth references

uClaw files this PR builds on top of (PR1-8 already merged):
- `src-tauri/src/memory_adapter/traits.rs` (PR1) — the 8-method `MemoryAdapter` trait
- `src-tauri/src/memory_adapter/types.rs` (PR1) — `MemoryEntry`, `MemoryCategory`, `RecallOpts`, `NamespaceSummary`
- `src-tauri/src/memory_adapter/legacy_kv.rs` (PR2) + `legacy_steward.rs` (PR3) — reference patterns for adapter shape
- `src-tauri/src/memory_bucket_seal/store.rs` (PR5) — `BucketSealStore`, `SCHEMA` constant (extend here for FTS5)
- `src-tauri/src/memory_bucket_seal/canonicalize/document.rs` (PR6) — `canonicalise` for Document source kind
- `src-tauri/src/memory_bucket_seal/chunker.rs` (PR6) — `chunk_markdown`, `ChunkerInput`, `ChunkerOptions`
- `src-tauri/src/memory_bucket_seal/score/mod.rs` (PR7) — `score_chunk`, `ScoringConfig`
- `src-tauri/src/memory_bucket_seal/score/embed/inert.rs` (PR7) — `InertEmbedder`
- `src-tauri/src/memory_bucket_seal/tree_source/{bucket_seal, registry, summariser/inert}.rs` (PR8) — `append_leaf`, `get_or_create_source_tree`, `InertSummariser`, `LabelStrategy`, `LeafRef`
- `src-tauri/src/app.rs:958-979` — existing memory adapter registry construction
- `src-tauri/src/app.rs:1010-1013` — `default_memory_backend` literal (currently `"legacy_kv"`)

## File Structure

| File | Purpose | LoC est. |
|---|---|---|
| `src-tauri/src/memory_bucket_seal/store.rs` (modify, +60 lines) | Extend `SCHEMA` with `mem_tree_chunks_fts` virtual table + 3 sync triggers (AFTER INSERT/UPDATE/DELETE on `mem_tree_chunks`). | +60 |
| `src-tauri/src/memory_bucket_seal/adapter.rs` (new) | `BucketSealAdapter` struct + `impl MemoryAdapter` for 8 methods + per-tree mutex helper + inline tests | ~600 |
| `src-tauri/src/memory_bucket_seal/mod.rs` (modify, +3 lines) | `pub mod adapter;` + `pub use adapter::BucketSealAdapter;` | +3 |
| `src-tauri/src/app.rs` (modify, ~25 lines) | Construct `BucketSealStore` at boot (path: `<data_dir>/bucket_seal/chunks.db`, content_root: `<data_dir>/bucket_seal/content/`). Build `Arc<BucketSealAdapter>` with `Arc<InertEmbedder>` + `Arc<InertSummariser>`. Register under `"bucket_seal"` slot. Flip `default_memory_backend` literal from `"legacy_kv"` → `"bucket_seal"`. | ~25 |

**LoC budget**: ~690 source + ~250 tests = **~940 LoC total**. Slightly over Option B's ~700 estimate due to test depth.

---

## Decisions Already Locked (no more questions)

- **Module path**: `src-tauri/src/memory_bucket_seal/adapter.rs` (flat — single file).
- **Adapter struct fields**:
  ```rust
  pub struct BucketSealAdapter {
      store: Arc<BucketSealStore>,
      content_root: PathBuf,
      embedder: Arc<dyn Embedder>,
      summariser: Arc<dyn Summariser>,
      tree_mutexes: tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
  }
  ```
- **All `store()` calls use `SourceKind::Document`** — single content piece per trait call. Conversation/Daily/Core/Custom only affect tags, not canonicalize dispatch. (Trait surface is `content: &str`; Chat would require multi-message structure.)
- **key storage**: in the chunk's `source_ref` column (already exists from PR5). `get(ns, key)` queries `WHERE source_id = ns AND source_ref = key`.
- **session_id storage**: appended to tags JSON array as `"session:{id}"`.
- **MemoryCategory → tags JSON encoding**:
  - `Core` → tag `"category:core"`
  - `Daily` → tag `"category:daily"`
  - `Conversation` → tag `"category:conversation"`
  - `Custom(s)` → tag `"category:custom:{s}"`
- **content_root**: `<data_dir>/bucket_seal/content/` (where chunks go on disk via `stage_chunks`).
- **DB path**: `<data_dir>/bucket_seal/chunks.db` (separate file — no V-number coordination with uClaw's `migrations.rs`).
- **`tree_mutexes` semantics**: outer `tokio::sync::Mutex` protects the HashMap (lookup-then-insert); inner `tokio::sync::Mutex<()>` is held across the `append_leaf` await — serialises per-tree per PR8's contract.
- **FTS5 sync via triggers**: SQLite triggers keep `mem_tree_chunks_fts` in sync with `mem_tree_chunks`. No explicit FTS writes in adapter code.
- **`recall()` ranking**: `ORDER BY rank` (FTS5's built-in BM25-style score). Hydrates joined `mem_tree_chunks` to `MemoryEntry`.
- **`get(ns, key)`**: returns the most-recent chunk by `created_at_ms DESC` when there are multiple matches (chunk_id is content-hash; multiple stores with same `(source_id, source_ref)` but different content produce multiple chunks).
- **`delete(ns, key)`**: hard DELETE from `mem_tree_chunks` (FTS trigger handles sync). Stale summary `child_ids` accepted (summary content stays valid; cleanup deferred to PR12 job).
- **`clear_namespace(ns)`**: hard DELETE WHERE source_id = ns; tree row + summaries + buffers left intact (PR12 cleanup can prune).
- **`list(ns, category, session)`**: SQL SELECT with `source_id = ?` AND `tags_json LIKE %"category:..."%` AND `tags_json LIKE %"session:..."%` filters.
- **`namespace_summaries()`**: `SELECT source_id, COUNT(*), MAX(timestamp_ms) FROM mem_tree_chunks GROUP BY source_id`.
- **No new workspace deps**: tokio (PR1), rusqlite (PR5), chrono, anyhow, async-trait, tracing all already there.
- **Default backend flip**: PR4 set `default_memory_backend` literal to `"legacy_kv"` temporarily. PR9 flips it to `"bucket_seal"`. UI that uses `memory_unified_*` without specifying backend will now route through bucket_seal.

---

## Adaptation responsibilities (DO NOT trust the plan blindly)

For each task:

1. **Re-read the trait file** (`memory_adapter/traits.rs`) and the reference impls (`legacy_kv.rs`, `legacy_steward.rs`) before writing `adapter.rs`. Match their style/error-handling/test patterns.

2. **Verify `tokio::sync::Mutex` is available**: PR1's `MemoryAdapter` trait uses `async_trait`, so tokio is in workspace. `grep -n "tokio" src-tauri/Cargo.toml` should confirm.

3. **Verify `LeafRef` / `LabelStrategy` re-exports**: PR8's `tree_source` re-exports them at `memory_bucket_seal::{LeafRef, LabelStrategy}` per its `mod.rs`. Verify with `grep -n "LeafRef\|LabelStrategy" src-tauri/src/memory_bucket_seal/mod.rs`.

4. **Verify `Document` canonicalise signature**: `canonicalize::document::canonicalise(source_id, owner, tags, doc) -> Result<Option<CanonicalisedSource>, String>` returns `String` errors (not `anyhow`). Convert to `anyhow::Error` at the adapter boundary: `.map_err(|e| anyhow::anyhow!("canonicalise: {}", e))?`.

5. **`tags_json` LIKE matching is fragile**: filtering by tag substring in JSON requires escaping quotes. Use parameterised LIKE: `tags_json LIKE '%"category:core"%'` (quote-wrapped to avoid false matches on substrings). The implementer verifies by reading PR5's `mem_tree_chunks.tags_json` format — should be a JSON array of strings (e.g., `["category:core", "session:abc"]`).

6. **FTS5 triggers**: the trigger SQL must reference NEW/OLD correctly. Test by inserting a chunk and confirming the FTS table receives the row.

7. **`MemoryEntry` hydration**: PR1's `MemoryEntry` struct has fields `id: String, namespace: String, key: String, content: String, category: MemoryCategory, session_id: Option<String>, created_at: String` (RFC3339 timestamp). Read `memory_adapter/types.rs` for the exact shape. Convert chunk's `timestamp_ms` to RFC3339 via `chrono::DateTime::from_timestamp_millis(ms).unwrap().to_rfc3339()`.

8. **Tag round-trip**: `store()` writes `"category:core"` / `"session:abc"` into `tags_json`; `recall()`/`get()`/`list()` parse them back out via `tags.iter().find_map(|t| t.strip_prefix("category:"))`. Custom variant: `"category:custom:my_label"` → strip `"category:custom:"` prefix.

9. **`source_ref` hydration**: PR5's `Chunk.metadata.source_ref: Option<SourceRef>` where `SourceRef::value: String`. Adapter writes `key` as `Some(SourceRef::new(key.to_string()))`. Hydration: `chunk.metadata.source_ref.as_ref().map(|r| r.value.clone()).unwrap_or_default()`.

10. **Per-tree mutex acquisition pattern**:
    ```rust
    let tree_mutex = {
        let mut map = self.tree_mutexes.lock().await;
        map.entry(namespace.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    let _guard = tree_mutex.lock().await; // serialises append_leaf for this tree
    // ... pipeline ...
    ```
    Critical: the outer Mutex guard MUST be dropped before acquiring the inner guard (avoid holding two locks).

11. **`stage_chunks` + `upsert_staged_chunks` separation**: stage admitted chunks to disk first (returns `Vec<StagedChunk>`), then upsert them in one SQL batch. Same for score rows.

12. **`append_leaf` call shape**: `append_leaf(&self.store, &tree, &leaf, &self.summariser, &self.embedder, &LabelStrategy::Empty).await?`. Pass refs (the Arc derefs naturally).

13. **AppState wiring concurrency**: AppState::new is sync (no `.await`). `BucketSealStore::open` and `ensure_schema` are sync. Adapter construction is sync. Good. The async `tokio::sync::Mutex` inside the adapter only locks during `store()` calls.

14. **`default_memory_backend` flip**: in `app.rs:1010-1013`, change the `"legacy_kv"` string literal to `"bucket_seal"`. Update the comment to note the flip is intentional now that PR9 ships the real adapter.

15. **Pre-commit hooks**: same as previous PRs. Don't `--no-verify`.

---

### Task 1: Schema extension — FTS5 virtual table + 3 triggers

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/store.rs` (extend `SCHEMA` constant)

- [ ] **Step 1: Extend `SCHEMA` constant**

In `src-tauri/src/memory_bucket_seal/store.rs`, find the existing `SCHEMA` constant (which now contains 5 tables after PR5+PR7+PR8). After the existing tables, append:

```sql
-- FTS5 virtual table backing keyword search in BucketSealAdapter::recall.
-- Mirrors a subset of mem_tree_chunks columns; kept in sync via triggers.
CREATE VIRTUAL TABLE IF NOT EXISTS mem_tree_chunks_fts USING fts5(
    chunk_id UNINDEXED,
    source_id UNINDEXED,
    content,
    tokenize = 'porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS mem_tree_chunks_fts_insert
    AFTER INSERT ON mem_tree_chunks
    BEGIN
        INSERT INTO mem_tree_chunks_fts (chunk_id, source_id, content)
        VALUES (NEW.id, NEW.source_id, NEW.content);
    END;

CREATE TRIGGER IF NOT EXISTS mem_tree_chunks_fts_update
    AFTER UPDATE ON mem_tree_chunks
    BEGIN
        UPDATE mem_tree_chunks_fts
            SET content = NEW.content, source_id = NEW.source_id
            WHERE chunk_id = NEW.id;
    END;

CREATE TRIGGER IF NOT EXISTS mem_tree_chunks_fts_delete
    AFTER DELETE ON mem_tree_chunks
    BEGIN
        DELETE FROM mem_tree_chunks_fts WHERE chunk_id = OLD.id;
    END;
```

- [ ] **Step 2: Add a test to PR5's store.rs verifying FTS5 sync**

Append to the existing `#[cfg(test)] mod tests` block in `memory_bucket_seal/store.rs`:

```rust
    #[test]
    fn fts5_sync_via_insert_trigger() {
        let (store, dir) = fresh_store();
        let chunks = vec![sample_chunk(0)];
        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();

        // Verify the FTS row was created by the trigger.
        let conn = store.lock_conn().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM mem_tree_chunks_fts WHERE chunk_id = ?1",
                rusqlite::params![chunks[0].id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "FTS insert trigger should have fired");
    }

    #[test]
    fn fts5_sync_via_delete_trigger() {
        let (store, dir) = fresh_store();
        let chunks = vec![sample_chunk(0)];
        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();

        let removed = {
            let conn = store.lock_conn().unwrap();
            conn.execute(
                "DELETE FROM mem_tree_chunks WHERE id = ?1",
                rusqlite::params![chunks[0].id],
            )
            .unwrap()
        };
        assert_eq!(removed, 1);

        let conn = store.lock_conn().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM mem_tree_chunks_fts WHERE chunk_id = ?1",
                rusqlite::params![chunks[0].id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "FTS delete trigger should have fired");
    }
```

- [ ] **Step 3: Build + run new FTS tests**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors. (If SQLite emits a syntax error for the FTS5 virtual table, verify rusqlite's `bundled` feature is on — should be from PR5.)

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::store::tests::fts5 2>&1 | tail -10`
Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/store.rs
git commit -m "feat(memory_bucket_seal): mem_tree_chunks_fts FTS5 + sync triggers (PR9.1 of 阶段 4)"
```

---

### Task 2: `adapter.rs` skeleton + `name()` + struct + `new()`

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/adapter.rs` (skeleton at this step)
- Modify: `src-tauri/src/memory_bucket_seal/mod.rs` (add `pub mod adapter;` + re-export)

- [ ] **Step 1: Skeleton `adapter.rs`**

```rust
//! `BucketSealAdapter` — first non-wrap `MemoryAdapter` impl.
//!
//! Orchestrates the PR5-8 stack into the trait surface:
//! - `store` = canonicalise → chunk → score → append_leaf (per-tree serialised)
//! - `recall` = FTS5 MATCH on `mem_tree_chunks_fts` scoped by namespace
//! - `get`/`list`/`delete`/`clear_namespace`/`namespace_summaries` = direct SQL
//!
//! Embedder + Summariser are injected via `Arc<dyn ...>` so PR12 can swap
//! `InertEmbedder`/`InertSummariser` for `OllamaEmbedder`/`LlmSummariser`
//! without touching this adapter.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use tokio::sync::Mutex;

use crate::memory_adapter::traits::MemoryAdapter;
use crate::memory_adapter::types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};
use crate::memory_bucket_seal::canonicalize::document::{canonicalise, DocumentInput};
use crate::memory_bucket_seal::chunker::{chunk_markdown, ChunkerInput, ChunkerOptions};
use crate::memory_bucket_seal::score::embed::Embedder;
use crate::memory_bucket_seal::score::store::{upsert_score, ScoreRow};
use crate::memory_bucket_seal::score::{score_chunk, ScoringConfig};
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_source::{
    append_leaf, get_or_create_source_tree, LabelStrategy, LeafRef, Summariser,
};
use crate::memory_bucket_seal::{stage_chunks, types::SourceKind, StagedChunk};

const ADAPTER_NAME: &str = "bucket_seal";

pub struct BucketSealAdapter {
    store: Arc<BucketSealStore>,
    content_root: PathBuf,
    embedder: Arc<dyn Embedder>,
    summariser: Arc<dyn Summariser>,
    tree_mutexes: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl BucketSealAdapter {
    pub fn new(
        store: Arc<BucketSealStore>,
        content_root: PathBuf,
        embedder: Arc<dyn Embedder>,
        summariser: Arc<dyn Summariser>,
    ) -> Self {
        Self {
            store,
            content_root,
            embedder,
            summariser,
            tree_mutexes: Mutex::new(HashMap::new()),
        }
    }

    /// Acquire (or create) the per-tree mutex for `namespace`. The returned
    /// Arc holds the inner mutex; calling `.lock().await` on it serialises
    /// `append_leaf` for that tree per PR8's concurrency contract.
    async fn tree_mutex(&self, namespace: &str) -> Arc<Mutex<()>> {
        let mut map = self.tree_mutexes.lock().await;
        map.entry(namespace.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}

#[async_trait]
impl MemoryAdapter for BucketSealAdapter {
    fn name(&self) -> &str {
        ADAPTER_NAME
    }

    // The other 7 methods land in Tasks 3-6.
    async fn store(
        &self,
        _namespace: &str,
        _key: &str,
        _content: &str,
        _category: MemoryCategory,
        _session_id: Option<&str>,
    ) -> Result<()> {
        anyhow::bail!("BucketSealAdapter::store not yet implemented (PR9.3)")
    }

    async fn recall(
        &self,
        _query: &str,
        _limit: usize,
        _opts: RecallOpts<'_>,
    ) -> Result<Vec<MemoryEntry>> {
        anyhow::bail!("BucketSealAdapter::recall not yet implemented (PR9.4)")
    }

    async fn get(&self, _namespace: &str, _key: &str) -> Result<Option<MemoryEntry>> {
        anyhow::bail!("BucketSealAdapter::get not yet implemented (PR9.5)")
    }

    async fn list(
        &self,
        _namespace: Option<&str>,
        _category: Option<&MemoryCategory>,
        _session_id: Option<&str>,
    ) -> Result<Vec<MemoryEntry>> {
        anyhow::bail!("BucketSealAdapter::list not yet implemented (PR9.5)")
    }

    async fn delete(&self, _namespace: &str, _key: &str) -> Result<bool> {
        anyhow::bail!("BucketSealAdapter::delete not yet implemented (PR9.6)")
    }

    async fn clear_namespace(&self, _namespace: &str) -> Result<u64> {
        anyhow::bail!("BucketSealAdapter::clear_namespace not yet implemented (PR9.6)")
    }

    async fn namespace_summaries(&self) -> Result<Vec<NamespaceSummary>> {
        anyhow::bail!("BucketSealAdapter::namespace_summaries not yet implemented (PR9.6)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::score::embed::InertEmbedder;
    use crate::memory_bucket_seal::tree_source::InertSummariser;
    use tempfile::TempDir;

    fn fresh_adapter() -> (BucketSealAdapter, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("chunks.db");
        let store = Arc::new(BucketSealStore::open(&db_path).unwrap());
        store.ensure_schema().unwrap();
        let content_root = dir.path().join("content");
        let embedder: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());
        let summariser: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
        let adapter = BucketSealAdapter::new(store, content_root, embedder, summariser);
        (adapter, dir)
    }

    #[tokio::test]
    async fn name_is_bucket_seal() {
        let (adapter, _dir) = fresh_adapter();
        assert_eq!(adapter.name(), "bucket_seal");
    }

    #[tokio::test]
    async fn tree_mutex_returns_same_arc_for_same_namespace() {
        let (adapter, _dir) = fresh_adapter();
        let m1 = adapter.tree_mutex("ns1").await;
        let m2 = adapter.tree_mutex("ns1").await;
        // Same namespace → same Arc
        assert!(Arc::ptr_eq(&m1, &m2));
        let m3 = adapter.tree_mutex("ns2").await;
        // Different namespace → different Arc
        assert!(!Arc::ptr_eq(&m1, &m3));
    }
}
```

- [ ] **Step 2: Wire `pub mod adapter;` into `memory_bucket_seal/mod.rs`**

```rust
pub mod adapter;

pub use adapter::BucketSealAdapter;
```

(Place near the other `pub mod` and `pub use` blocks.)

- [ ] **Step 3: Build + run skeleton tests**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors.

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::adapter::tests 2>&1 | tail -10`
Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/adapter.rs src-tauri/src/memory_bucket_seal/mod.rs
git commit -m "feat(memory_bucket_seal): BucketSealAdapter skeleton (PR9.2 of 阶段 4)"
```

---

### Task 3: `store()` impl — full ingestion pipeline

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/adapter.rs`

- [ ] **Step 1: Replace `store()` stub with the full impl**

```rust
async fn store(
    &self,
    namespace: &str,
    key: &str,
    content: &str,
    category: MemoryCategory,
    session_id: Option<&str>,
) -> Result<()> {
    if content.trim().is_empty() {
        tracing::debug!(namespace = %namespace, key = %key, "skipping empty content");
        return Ok(());
    }

    // 1. Resolve tree (idempotent get_or_create).
    let tree = get_or_create_source_tree(&self.store, namespace)
        .context("get_or_create_source_tree")?;

    // 2. Acquire per-tree mutex (PR8 contract).
    let tree_mutex = self.tree_mutex(namespace).await;
    let _guard = tree_mutex.lock().await;

    // 3. Build tags (category + session encoded).
    let tags = build_tags(&category, session_id);

    // 4. Canonicalise as Document.
    let canonical = canonicalise(
        namespace,
        "system",
        &tags,
        DocumentInput {
            provider: "uclaw".to_string(),
            title: key.to_string(),
            body: content.to_string(),
            modified_at: Utc::now(),
            source_ref: Some(key.to_string()),
        },
    )
    .map_err(|e| anyhow::anyhow!("canonicalise: {}", e))?;

    let Some(canonical) = canonical else {
        tracing::debug!(namespace = %namespace, key = %key, "canonicalise returned None");
        return Ok(());
    };

    // 5. Chunk.
    let chunker_input = ChunkerInput {
        source_kind: SourceKind::Document,
        source_id: namespace.to_string(),
        markdown: canonical.markdown.clone(),
        metadata: canonical.metadata.clone(),
    };
    let chunks = chunk_markdown(&chunker_input, &ChunkerOptions::default());
    if chunks.is_empty() {
        tracing::debug!(namespace = %namespace, key = %key, "chunker produced no chunks");
        return Ok(());
    }

    // 6. Score each chunk; stage + upsert admitted ones.
    let scoring_config = ScoringConfig::default();
    let mut admitted: Vec<crate::memory_bucket_seal::types::Chunk> = Vec::new();
    let mut score_rows: Vec<ScoreRow> = Vec::new();
    for chunk in &chunks {
        let result = score_chunk(chunk, &scoring_config);
        // Persist the score row regardless of admission (audit trail).
        let row = ScoreRow {
            chunk_id: result.chunk_id.clone(),
            total: result.total,
            signals: result.signals.clone(),
            dropped: !result.kept,
            reason: result.drop_reason.clone(),
            computed_at_ms: Utc::now().timestamp_millis(),
        };
        score_rows.push(row);
        if result.kept {
            admitted.push(chunk.clone());
        }
    }

    // 7. Stage admitted chunks to disk and upsert to mem_tree_chunks.
    if !admitted.is_empty() {
        let staged = stage_chunks(&self.content_root, &admitted)
            .context("stage_chunks")?;
        self.store
            .upsert_staged_chunks(&staged)
            .context("upsert_staged_chunks")?;
    }

    // 8. Persist score rows (FK requires chunks already inserted).
    for row in &score_rows {
        // Only upsert scores for chunks we actually persisted (FK constraint).
        if admitted.iter().any(|c| c.id == row.chunk_id) {
            upsert_score(&self.store, row).context("upsert_score")?;
        }
    }

    // 9. append_leaf each admitted chunk so the seal cascade can fire.
    for chunk in &admitted {
        let leaf = LeafRef {
            chunk_id: chunk.id.clone(),
            token_count: chunk.token_count,
            timestamp: chunk.metadata.timestamp,
            content: chunk.content.clone(),
            entities: chunk.metadata.tags.clone(), // placeholder; extract lands later
            topics: vec![],
            score: score_rows
                .iter()
                .find(|r| r.chunk_id == chunk.id)
                .map(|r| r.total)
                .unwrap_or(0.0),
        };
        append_leaf(
            &self.store,
            &tree,
            &leaf,
            &self.summariser,
            &self.embedder,
            &LabelStrategy::Empty,
        )
        .await
        .context("append_leaf")?;
    }

    Ok(())
}

/// Build the `tags` vec for a chunk based on the trait's category + session_id.
fn build_tags(category: &MemoryCategory, session_id: Option<&str>) -> Vec<String> {
    let mut tags = Vec::with_capacity(2);
    let category_tag = match category {
        MemoryCategory::Core => "category:core".to_string(),
        MemoryCategory::Daily => "category:daily".to_string(),
        MemoryCategory::Conversation => "category:conversation".to_string(),
        MemoryCategory::Custom(s) => format!("category:custom:{}", s),
    };
    tags.push(category_tag);
    if let Some(s) = session_id {
        tags.push(format!("session:{}", s));
    }
    tags
}

#[cfg(test)]
fn parse_tags(tags: &[String]) -> (MemoryCategory, Option<String>) {
    let mut category = MemoryCategory::Custom("unknown".to_string());
    let mut session = None;
    for tag in tags {
        if let Some(rest) = tag.strip_prefix("category:") {
            category = match rest {
                "core" => MemoryCategory::Core,
                "daily" => MemoryCategory::Daily,
                "conversation" => MemoryCategory::Conversation,
                _ => {
                    if let Some(custom) = rest.strip_prefix("custom:") {
                        MemoryCategory::Custom(custom.to_string())
                    } else {
                        MemoryCategory::Custom(rest.to_string())
                    }
                }
            };
        } else if let Some(rest) = tag.strip_prefix("session:") {
            session = Some(rest.to_string());
        }
    }
    (category, session)
}
```

(The `parse_tags` helper is `#[cfg(test)]` for now — it'll be promoted to non-test scope and used by `recall`/`get`/`list` in Tasks 4-5 to hydrate `MemoryEntry`. Drop the `#[cfg(test)]` annotation in Task 4.)

- [ ] **Step 2: Add `store()` tests**

Append to the `#[cfg(test)] mod tests` block:

```rust
    #[tokio::test]
    async fn store_admits_and_appends_a_chunk() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store(
                "test_ns",
                "key_1",
                "Substantive note about a meaningful topic with sufficient signal density.",
                MemoryCategory::Core,
                Some("session_abc"),
            )
            .await
            .unwrap();

        // The tree should exist.
        let tree = get_or_create_source_tree(&adapter.store, "test_ns").unwrap();
        // At least one chunk should be in mem_tree_chunks.
        let count = adapter.store.count_chunks().unwrap();
        assert!(count >= 1, "store should have inserted at least one chunk");
        let _ = tree;
    }

    #[tokio::test]
    async fn store_skips_empty_content() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("test_ns", "key_empty", "   ", MemoryCategory::Core, None)
            .await
            .unwrap();
        assert_eq!(adapter.store.count_chunks().unwrap(), 0);
    }

    #[tokio::test]
    async fn store_serialises_per_tree_via_mutex() {
        let (adapter, _dir) = fresh_adapter();
        // Spawn 5 concurrent stores for the same namespace; verify all 5 land
        // without deadlock or panic.
        let adapter = Arc::new(adapter);
        let mut handles = Vec::new();
        for i in 0..5 {
            let a = adapter.clone();
            handles.push(tokio::spawn(async move {
                a.store(
                    "concurrent_ns",
                    &format!("key_{i}"),
                    &format!("Substantive note number {i} with enough signal to pass admission."),
                    MemoryCategory::Core,
                    None,
                )
                .await
            }));
        }
        for h in handles {
            h.await.unwrap().unwrap();
        }
        // All 5 stores should produce ≥5 chunks total.
        assert!(adapter.store.count_chunks().unwrap() >= 5);
    }
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::adapter::tests::store 2>&1 | tail -15`
Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/adapter.rs
git commit -m "feat(memory_bucket_seal): BucketSealAdapter::store full ingestion pipeline (PR9.3 of 阶段 4)"
```

---

### Task 4: `recall()` impl — FTS5 MATCH

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/adapter.rs`

- [ ] **Step 1: Replace `recall()` stub**

```rust
async fn recall(
    &self,
    query: &str,
    limit: usize,
    opts: RecallOpts<'_>,
) -> Result<Vec<MemoryEntry>> {
    let conn = self.store.lock_conn()?;
    let mut stmt = conn.prepare(
        "SELECT c.id, c.source_id, c.source_ref, c.content, c.timestamp_ms, c.tags_json
           FROM mem_tree_chunks_fts AS fts
           JOIN mem_tree_chunks    AS c ON c.id = fts.chunk_id
          WHERE fts.content MATCH ?1
            AND (?2 IS NULL OR fts.source_id = ?2)
          ORDER BY rank
          LIMIT ?3",
    )?;

    let ns_param = opts.namespace.map(|s| s.to_string());
    let rows = stmt.query_map(
        rusqlite::params![query, ns_param, limit as i64],
        row_to_memory_entry,
    )?;

    let mut out: Vec<MemoryEntry> = Vec::new();
    for row in rows {
        let entry = row?;
        // Optional category filter.
        if let Some(filter) = opts.category {
            if entry.category != *filter {
                continue;
            }
        }
        out.push(entry);
    }
    Ok(out)
}

/// Hydrate a row from the `c.*` columns of the recall query into a MemoryEntry.
fn row_to_memory_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEntry> {
    let id: String = row.get(0)?;
    let source_id: String = row.get(1)?;
    let source_ref: Option<String> = row.get(2)?;
    let content: String = row.get(3)?;
    let timestamp_ms: i64 = row.get(4)?;
    let tags_json: String = row.get(5)?;

    let tags: Vec<String> = serde_json::from_str(&tags_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            Box::new(e),
        )
    })?;
    let (category, session_id) = parse_tags(&tags);

    let created_at = Utc
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Integer,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "invalid timestamp_ms",
                )),
            )
        })?
        .to_rfc3339();

    Ok(MemoryEntry {
        id,
        namespace: source_id,
        key: source_ref.unwrap_or_default(),
        content,
        category,
        session_id,
        created_at,
    })
}
```

Also: **drop the `#[cfg(test)]` annotation on `parse_tags`** (it's now used by production code). Verify the function is `pub(crate)` or higher if needed.

- [ ] **Step 2: Add `recall()` tests**

```rust
    #[tokio::test]
    async fn recall_matches_substring_via_fts() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store(
                "recall_ns",
                "k1",
                "Project Phoenix launch plan with quarterly milestones.",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();
        adapter
            .store(
                "recall_ns",
                "k2",
                "Unrelated note about weather patterns.",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();

        let opts = RecallOpts {
            namespace: Some("recall_ns"),
            category: None,
            session_id: None,
            min_score: None,
        };
        let hits = adapter.recall("Phoenix", 10, opts).await.unwrap();
        assert!(!hits.is_empty(), "FTS should find 'Phoenix'");
        assert!(hits.iter().any(|e| e.content.contains("Phoenix")));
    }

    #[tokio::test]
    async fn recall_respects_namespace_filter() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("ns_a", "k1", "Apple banana cherry common keyword.", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("ns_b", "k2", "Apple banana cherry common keyword.", MemoryCategory::Core, None)
            .await
            .unwrap();

        let opts_a = RecallOpts {
            namespace: Some("ns_a"),
            category: None,
            session_id: None,
            min_score: None,
        };
        let hits_a = adapter.recall("common", 10, opts_a).await.unwrap();
        assert!(hits_a.iter().all(|e| e.namespace == "ns_a"));
    }

    #[tokio::test]
    async fn recall_respects_limit() {
        let (adapter, _dir) = fresh_adapter();
        for i in 0..5 {
            adapter
                .store(
                    "limit_ns",
                    &format!("k{i}"),
                    &format!("Unique repeatable keyword content line {i}."),
                    MemoryCategory::Core,
                    None,
                )
                .await
                .unwrap();
        }
        let opts = RecallOpts {
            namespace: Some("limit_ns"),
            category: None,
            session_id: None,
            min_score: None,
        };
        let hits = adapter.recall("unique", 2, opts).await.unwrap();
        assert!(hits.len() <= 2);
    }
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::adapter::tests::recall 2>&1 | tail -10`
Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/adapter.rs
git commit -m "feat(memory_bucket_seal): BucketSealAdapter::recall via FTS5 (PR9.4 of 阶段 4)"
```

---

### Task 5: `get()` + `list()` + `namespace_summaries()`

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/adapter.rs`

- [ ] **Step 1: Implement `get()`**

```rust
async fn get(&self, namespace: &str, key: &str) -> Result<Option<MemoryEntry>> {
    let conn = self.store.lock_conn()?;
    let mut stmt = conn.prepare(
        "SELECT id, source_id, source_ref, content, timestamp_ms, tags_json
           FROM mem_tree_chunks
          WHERE source_id = ?1 AND source_ref = ?2
          ORDER BY created_at_ms DESC
          LIMIT 1",
    )?;
    let entry: Option<MemoryEntry> = stmt
        .query_row(rusqlite::params![namespace, key], row_to_memory_entry)
        .optional()
        .context("get_chunk")?;
    Ok(entry)
}
```

Add `use rusqlite::OptionalExtension;` at the top of the file.

- [ ] **Step 2: Implement `list()`**

```rust
async fn list(
    &self,
    namespace: Option<&str>,
    category: Option<&MemoryCategory>,
    session_id: Option<&str>,
) -> Result<Vec<MemoryEntry>> {
    let conn = self.store.lock_conn()?;

    let mut sql = String::from(
        "SELECT id, source_id, source_ref, content, timestamp_ms, tags_json
           FROM mem_tree_chunks
          WHERE 1=1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(ns) = namespace {
        sql.push_str(" AND source_id = ?");
        params.push(Box::new(ns.to_string()));
    }
    if let Some(s) = session_id {
        sql.push_str(" AND tags_json LIKE ?");
        params.push(Box::new(format!("%\"session:{}\"%", s)));
    }

    sql.push_str(" ORDER BY timestamp_ms DESC LIMIT 200");

    let mut stmt = conn.prepare(&sql)?;
    let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(&params_refs[..], row_to_memory_entry)?;

    let mut out: Vec<MemoryEntry> = Vec::new();
    for row in rows {
        let entry = row?;
        if let Some(filter) = category {
            if entry.category != *filter {
                continue;
            }
        }
        out.push(entry);
    }
    Ok(out)
}
```

- [ ] **Step 3: Implement `namespace_summaries()`**

```rust
async fn namespace_summaries(&self) -> Result<Vec<NamespaceSummary>> {
    let conn = self.store.lock_conn()?;
    let mut stmt = conn.prepare(
        "SELECT source_id, COUNT(*), MAX(timestamp_ms)
           FROM mem_tree_chunks
          GROUP BY source_id
          ORDER BY source_id",
    )?;
    let rows = stmt.query_map([], |row| {
        let namespace: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        let last_updated_ms: Option<i64> = row.get(2)?;
        let last_updated = last_updated_ms.and_then(|ms| {
            Utc.timestamp_millis_opt(ms)
                .single()
                .map(|dt| dt.to_rfc3339())
        });
        Ok(NamespaceSummary {
            namespace,
            count: count.max(0) as usize,
            last_updated,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}
```

- [ ] **Step 4: Add tests**

```rust
    #[tokio::test]
    async fn get_returns_most_recent_chunk_for_key() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("ns_g", "the_key", "First version content.", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("ns_g", "the_key", "Second version updated content.", MemoryCategory::Core, None)
            .await
            .unwrap();
        let got = adapter.get("ns_g", "the_key").await.unwrap();
        assert!(got.is_some());
        // Most-recent ordering means the second store wins.
        let entry = got.unwrap();
        assert!(entry.content.contains("Second") || entry.content.contains("updated"));
    }

    #[tokio::test]
    async fn list_filters_by_namespace_and_category() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("nslA", "k1", "Note A1 substantive content.", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("nslA", "k2", "Note A2 substantive content.", MemoryCategory::Conversation, None)
            .await
            .unwrap();
        adapter
            .store("nslB", "k3", "Note B substantive content.", MemoryCategory::Core, None)
            .await
            .unwrap();

        let listed = adapter
            .list(Some("nslA"), Some(&MemoryCategory::Core), None)
            .await
            .unwrap();
        assert!(listed.iter().all(|e| e.namespace == "nslA"));
        assert!(listed.iter().all(|e| matches!(e.category, MemoryCategory::Core)));
    }

    #[tokio::test]
    async fn namespace_summaries_groups_by_source() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("nsA", "k1", "Note in nsA with substance.", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("nsB", "k2", "Note in nsB with substance.", MemoryCategory::Core, None)
            .await
            .unwrap();
        let summaries = adapter.namespace_summaries().await.unwrap();
        assert!(summaries.iter().any(|s| s.namespace == "nsA"));
        assert!(summaries.iter().any(|s| s.namespace == "nsB"));
    }
```

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::adapter::tests 2>&1 | tail -15`
Expected: ~11 passed (2 skeleton + 3 store + 3 recall + 3 new).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/adapter.rs
git commit -m "feat(memory_bucket_seal): BucketSealAdapter get/list/namespace_summaries (PR9.5 of 阶段 4)"
```

---

### Task 6: `delete()` + `clear_namespace()`

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/adapter.rs`

- [ ] **Step 1: Implement `delete()`**

```rust
async fn delete(&self, namespace: &str, key: &str) -> Result<bool> {
    let conn = self.store.lock_conn()?;
    let n = conn.execute(
        "DELETE FROM mem_tree_chunks
          WHERE source_id = ?1 AND source_ref = ?2",
        rusqlite::params![namespace, key],
    )?;
    Ok(n > 0)
}
```

- [ ] **Step 2: Implement `clear_namespace()`**

```rust
async fn clear_namespace(&self, namespace: &str) -> Result<u64> {
    let conn = self.store.lock_conn()?;
    let n = conn.execute(
        "DELETE FROM mem_tree_chunks WHERE source_id = ?1",
        rusqlite::params![namespace],
    )?;
    Ok(n as u64)
}
```

- [ ] **Step 3: Add tests**

```rust
    #[tokio::test]
    async fn delete_returns_true_then_false() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("ns_d", "the_key", "Content to delete.", MemoryCategory::Core, None)
            .await
            .unwrap();
        // The store may have produced multiple chunks for one (namespace, key)
        // if re-stored. First delete removes all matching; second returns false.
        let first = adapter.delete("ns_d", "the_key").await.unwrap();
        let second = adapter.delete("ns_d", "the_key").await.unwrap();
        assert!(first);
        assert!(!second);
    }

    #[tokio::test]
    async fn clear_namespace_removes_chunks_in_scope_only() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("ns_keep", "k1", "Content to keep substantively.", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("ns_drop", "k2", "Content to drop substantively.", MemoryCategory::Core, None)
            .await
            .unwrap();

        let removed = adapter.clear_namespace("ns_drop").await.unwrap();
        assert!(removed >= 1, "expected at least one chunk removed");

        // ns_keep entries should still exist.
        let kept = adapter.list(Some("ns_keep"), None, None).await.unwrap();
        assert!(!kept.is_empty());
        let dropped = adapter.list(Some("ns_drop"), None, None).await.unwrap();
        assert!(dropped.is_empty());
    }

    #[tokio::test]
    async fn delete_propagates_to_fts() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("ns_fts", "k1", "Unique searchable keyword payload.", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter.delete("ns_fts", "k1").await.unwrap();

        // The FTS index should no longer return the row.
        let opts = RecallOpts {
            namespace: Some("ns_fts"),
            category: None,
            session_id: None,
            min_score: None,
        };
        let hits = adapter.recall("unique", 10, opts).await.unwrap();
        assert!(hits.is_empty(), "delete trigger should have cleared FTS row");
    }
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::adapter::tests 2>&1 | tail -15`
Expected: ~14 passed (the previous 11 + 3 new).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/adapter.rs
git commit -m "feat(memory_bucket_seal): BucketSealAdapter delete + clear_namespace (PR9.6 of 阶段 4)"
```

---

### Task 7: AppState wiring + default backend flip

**Files:**
- Modify: `src-tauri/src/app.rs` (~25 lines added/changed)

- [ ] **Step 1: Build `BucketSealStore` + adapter at boot**

In `src-tauri/src/app.rs`, find the existing memory_adapters_map construction block (around lines 961-978). After the existing `memory_adapters_map.insert(legacy_steward_adapter...)` line and before the `let memory_adapters = ...` line, add:

```rust
// Task 7 of PR9 (阶段 4): BucketSealAdapter ships the full bucket-seal
// pipeline as a non-wrap MemoryAdapter. Uses InertEmbedder + InertSummariser
// at boot; PR12 (jobs) will swap in real Ollama/LLM backends.
let bucket_seal_dir = data_dir.join("bucket_seal");
std::fs::create_dir_all(&bucket_seal_dir).ok();
let bucket_seal_db_path = bucket_seal_dir.join("chunks.db");
let bucket_seal_content_root = bucket_seal_dir.join("content");
std::fs::create_dir_all(&bucket_seal_content_root).ok();

let bucket_seal_store = std::sync::Arc::new(
    crate::memory_bucket_seal::BucketSealStore::open(&bucket_seal_db_path)
        .expect("open bucket_seal chunks.db"),
);
bucket_seal_store
    .ensure_schema()
    .expect("apply bucket_seal SCHEMA");

let bucket_seal_embedder: std::sync::Arc<dyn crate::memory_bucket_seal::Embedder> =
    std::sync::Arc::new(crate::memory_bucket_seal::InertEmbedder::new());
let bucket_seal_summariser: std::sync::Arc<dyn crate::memory_bucket_seal::Summariser> =
    std::sync::Arc::new(crate::memory_bucket_seal::InertSummariser::new());

let bucket_seal_adapter = std::sync::Arc::new(
    crate::memory_bucket_seal::BucketSealAdapter::new(
        bucket_seal_store,
        bucket_seal_content_root,
        bucket_seal_embedder,
        bucket_seal_summariser,
    ),
) as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;

memory_adapters_map.insert(
    bucket_seal_adapter.name().to_string(),
    bucket_seal_adapter,
);
```

**Adaptation note**: the `Arc<dyn Embedder>` / `Arc<dyn Summariser>` type annotations are needed because `Arc::new(InertEmbedder::new())` would otherwise infer `Arc<InertEmbedder>` which doesn't directly coerce to `Arc<dyn Embedder>`. The `as` cast on `bucket_seal_adapter` ensures it matches the HashMap's value type.

Also verify the import path: `crate::memory_bucket_seal::{BucketSealStore, BucketSealAdapter, Embedder, InertEmbedder, Summariser, InertSummariser}` should all resolve. If `Embedder` and `Summariser` aren't re-exported at the crate root of `memory_bucket_seal/mod.rs`, add them (Task 2 added `BucketSealAdapter`; verify the others are exposed too via PR7/PR8 work).

- [ ] **Step 2: Flip `default_memory_backend` literal**

Find the line in `src-tauri/src/app.rs` (around line 1010-1013):

```rust
default_memory_backend: std::sync::Arc::new(std::sync::RwLock::new(
    "legacy_kv".to_string(),
)),
```

Replace `"legacy_kv"` with `"bucket_seal"`. Update the inline comment to:

```rust
// Default to bucket_seal now that PR9 ships BucketSealAdapter. PR4
// temporarily held this at "legacy_kv" while the bucket-seal stack was
// being built out (PR5-8). The unified IPC family routes through this
// default when callers don't specify a backend explicitly.
default_memory_backend: std::sync::Arc::new(std::sync::RwLock::new(
    "bucket_seal".to_string(),
)),
```

- [ ] **Step 3: Build + run full module tests**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors.

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -10`
Expected: ~175+ passed (160 PR8 baseline + ~15 new from PR9).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/app.rs
git commit -m "feat(app): wire BucketSealAdapter + flip default_memory_backend → bucket_seal (PR9.7 of 阶段 4)"
```

---

### Task 8: End-to-end test via `memory.unified.*` IPC

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/mod.rs` (append to existing `#[cfg(test)]` block)

- [ ] **Step 1: Add e2e test using the adapter directly through the trait surface**

Since the IPC layer requires Tauri's `State<'_, AppState>` injection (not easily testable from a unit test), exercise the adapter as `Arc<dyn MemoryAdapter>` directly — this confirms the trait contract.

Append to the `#[cfg(test)] mod tests` block:

```rust
    #[tokio::test]
    async fn end_to_end_bucket_seal_adapter_via_trait_surface() {
        use crate::memory_adapter::traits::MemoryAdapter;
        use crate::memory_adapter::types::{MemoryCategory, RecallOpts};
        use crate::memory_bucket_seal::adapter::BucketSealAdapter;
        use crate::memory_bucket_seal::score::embed::{Embedder, InertEmbedder};
        use crate::memory_bucket_seal::tree_source::{InertSummariser, Summariser};
        use crate::memory_bucket_seal::store::BucketSealStore;
        use std::sync::Arc;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let store = Arc::new(BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        let embedder: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());
        let summariser: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
        let adapter: Arc<dyn MemoryAdapter> = Arc::new(BucketSealAdapter::new(
            store,
            dir.path().join("content"),
            embedder,
            summariser,
        ));

        // store → recall → get → list → namespace_summaries → delete → clear_namespace
        adapter
            .store("e2e_ns", "k1", "Project Phoenix launch plan and milestones.", MemoryCategory::Core, Some("sess1"))
            .await
            .unwrap();
        adapter
            .store("e2e_ns", "k2", "Unrelated weather note today.", MemoryCategory::Daily, None)
            .await
            .unwrap();

        let opts = RecallOpts { namespace: Some("e2e_ns"), category: None, session_id: None, min_score: None };
        let recalled = adapter.recall("Phoenix", 10, opts).await.unwrap();
        assert!(!recalled.is_empty(), "FTS should match 'Phoenix'");

        let got = adapter.get("e2e_ns", "k1").await.unwrap();
        assert!(got.is_some());

        let listed = adapter.list(Some("e2e_ns"), None, None).await.unwrap();
        assert!(listed.len() >= 2);

        let summaries = adapter.namespace_summaries().await.unwrap();
        assert!(summaries.iter().any(|s| s.namespace == "e2e_ns"));

        let deleted = adapter.delete("e2e_ns", "k1").await.unwrap();
        assert!(deleted);

        let cleared = adapter.clear_namespace("e2e_ns").await.unwrap();
        assert!(cleared >= 1);

        // After clear, list is empty.
        let listed_after = adapter.list(Some("e2e_ns"), None, None).await.unwrap();
        assert!(listed_after.is_empty());
    }
```

- [ ] **Step 2: Run test**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tests::end_to_end_bucket_seal 2>&1 | tail -10`
Expected: 1 passed (plus the existing 3 e2e tests from PR6/PR7/PR8 still pass).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/mod.rs
git commit -m "test(memory_bucket_seal): end-to-end BucketSealAdapter trait round-trip (PR9.8 of 阶段 4)"
```

---

### Task 9: Verification

- [ ] **Step 1: Full module test pass**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -15`
Expected: ~177+ passed (160 PR8 baseline + ~17 new from PR9 incl. FTS sync tests).

- [ ] **Step 2: Broader regression check**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -10`
Expected: net positive (baseline + ~17 new from PR9).

- [ ] **Step 3: Clippy on PR9 files**

Run: `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "adapter\.rs|memory_bucket_seal/store\.rs|app\.rs" | head -20`
Expected: zero hits attributable to PR9.

- [ ] **Step 4: Cargo.toml audit**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr9-bucket-seal-adapter && git diff main -- src-tauri/Cargo.toml`
Expected: empty (no new workspace deps).

- [ ] **Step 5: Stray TODO/FIXME scan**

Run: `cd src-tauri && grep -nE "TODO|FIXME|XXX" src/memory_bucket_seal/adapter.rs`
Expected: zero hits.

- [ ] **Step 6: Confirm `memory_unified_*` IPC routes through bucket_seal**

This is a manual verification (no test runs Tauri's IPC harness from cargo test):
- Confirm `app.rs` `default_memory_backend` literal is now `"bucket_seal"`.
- Confirm `BucketSealAdapter` is registered under key `"bucket_seal"` in `memory_adapters_map`.
- The unified IPC family from PR4 looks up the adapter by name; when no explicit backend is passed, it resolves to the default. So any UI calling `memory_unified_record` without `backend: Some(...)` will route through BucketSealAdapter.

- [ ] **Step 7: If verification surfaces small cleanups**

Apply them and commit:

```bash
git add -A
git commit -m "chore(memory_bucket_seal): PR9 cleanup pass"
```

If nothing to clean, skip.

---

## Test plan summary

| Test type | Count | Module |
|---|---|---|
| FTS5 sync triggers (insert + delete) | 2 | `memory_bucket_seal::store::tests::fts5_*` |
| Adapter skeleton (name + tree_mutex) | 2 | `adapter::tests::name_*`, `tree_mutex_*` |
| `store()` admit/skip/concurrency | 3 | `adapter::tests::store_*` |
| `recall()` FTS substring + namespace filter + limit | 3 | `adapter::tests::recall_*` |
| `get()`/`list()`/`namespace_summaries()` | 3 | `adapter::tests::{get_*, list_*, namespace_*}` |
| `delete()`/`clear_namespace()` + FTS propagation | 3 | `adapter::tests::{delete_*, clear_*}` |
| End-to-end adapter via trait surface | 1 | `memory_bucket_seal::tests::end_to_end_bucket_seal_*` |
| **Total new tests** | **~17** | — |
| **PR8 tests preserved** | 160 | (unchanged) |
| **Module total** | **~177** | — |

---

## Self-Review Checklist

- ✅ **Spec coverage**: Option B from brainstorming → adapter + FTS5 + AppState wiring + default flip. All 8 trait methods implemented.
- ✅ **Scope check**: NO summary cascade on clear_namespace, NO tombstone column on mem_tree_chunks, NO entity_index updates (extract not ported). Hard delete with stale-summary-child_ids accepted.
- ✅ **Trait fidelity**: 8 methods implemented matching PR1's signatures. Async, anyhow::Result, MemoryEntry hydration via `row_to_memory_entry`. Reference patterns from `LegacyKvAdapter`/`LegacyStewardAdapter` followed.
- ✅ **Concurrency contract**: per-tree mutex acquired before `append_leaf` (PR8 requirement). Tests confirm 5 concurrent stores on the same namespace serialise correctly.
- ✅ **FTS5 in-place**: `mem_tree_chunks_fts` virtual table + 3 triggers (INSERT/UPDATE/DELETE) extend PR5's SCHEMA. recall uses MATCH + rank.
- ✅ **AppState integration**: `BucketSealStore` built at boot under `<data_dir>/bucket_seal/`. `InertEmbedder` + `InertSummariser` Arc'd into the adapter. Registered under `"bucket_seal"` slot. Default backend flipped.
- ✅ **No placeholders**: every step shows actual code. Adaptation responsibilities enumerated.
- ✅ **Bisectability**: 9 task commits (FTS schema / skeleton / store / recall / get+list+summaries / delete+clear / AppState / e2e / cleanup). Each compiles standalone.
- ✅ **No new deps**: tokio + rusqlite + chrono + anyhow + async-trait + tracing all already in workspace.
- ✅ **`tracing::*` discipline**: no `log::*` slips.
- ✅ **L0 chunk hydration limitation** (PR8 TODO): inherited — `chunk.content` is the 500-char preview. PR9's recall/get/list return previews. PR12 task: wire `content_store::read::read_chunk_body` before LlmSummariser ships.
