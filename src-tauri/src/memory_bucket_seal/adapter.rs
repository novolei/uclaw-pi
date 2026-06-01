//! `BucketSealAdapter` — first non-wrap `MemoryAdapter` impl.
//!
//! Orchestrates the PR5-8 stack into the trait surface:
//! - `store` = canonicalise → chunk → score → append_leaf (per-tree serialised)
//! - `recall` = FTS5 MATCH on `mem_tree_chunks_fts` scoped by namespace
//! - `get`/`list`/`delete`/`clear_namespace`/`namespace_summaries` = direct SQL
//!
//! Embedder + Summariser are injected via `Arc<dyn ...>` so a later PR can swap
//! `InertEmbedder`/`InertSummariser` for real Ollama/LLM backends without
//! touching this adapter.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::Mutex;

use crate::memory_adapter::{
    MemoryAdapter, MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts,
};
use crate::memory_bucket_seal::canonicalize::document::{canonicalise, DocumentInput};
use crate::memory_bucket_seal::chunker::{chunk_markdown, ChunkerInput, ChunkerOptions};
use crate::memory_bucket_seal::score::embed::Embedder;
use crate::memory_bucket_seal::score::store::{upsert_score, ScoreRow};
use crate::memory_bucket_seal::score::{score_chunk, ScoringConfig};
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_source::{
    append_leaf, get_or_create_source_tree, LabelStrategy, LeafRef, Summariser,
};
use crate::memory_bucket_seal::{stage_chunks, types::SourceKind};

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

    // The other 6 methods land in Tasks 4-6.
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

        // 2. Acquire the per-tree mutex — PR8's append_leaf contract requires
        //    callers to serialise appends per tree.id (bucket_seal.rs:111-118).
        let tree_mutex = self.tree_mutex(namespace).await;
        let _guard = tree_mutex.lock().await;

        // 3. Build tags (category + session encoded into the chunk's tags_json).
        let tags = build_tags(&category, session_id);

        // 4. Canonicalise as a Document (one content piece per trait call).
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

        // 6. Score each chunk; collect admitted chunks + their score rows.
        let scoring_config = ScoringConfig::default();
        let mut admitted: Vec<crate::memory_bucket_seal::types::Chunk> = Vec::new();
        let mut score_rows: Vec<ScoreRow> = Vec::new();
        for chunk in &chunks {
            let result = score_chunk(chunk, &scoring_config);
            // Persist the score row regardless of admission (audit trail).
            score_rows.push(ScoreRow {
                chunk_id: result.chunk_id.clone(),
                total: result.total,
                signals: result.signals.clone(),
                dropped: !result.kept,
                reason: result.drop_reason.clone(),
                computed_at_ms: Utc::now().timestamp_millis(),
            });
            if result.kept {
                admitted.push(chunk.clone());
            }
        }

        // 7. Stage admitted chunks to disk and upsert to mem_tree_chunks.
        if !admitted.is_empty() {
            let staged = stage_chunks(&self.content_root, &admitted).context("stage_chunks")?;
            self.store
                .upsert_staged_chunks(&staged)
                .context("upsert_staged_chunks")?;
        }

        // 8. Persist score rows — only for chunks we actually staged, since
        //    mem_tree_score.chunk_id has an FK to mem_tree_chunks(id).
        for row in &score_rows {
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
                entities: chunk.metadata.tags.clone(), // placeholder until entity extract lands
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

/// Encode the trait's `category` + `session_id` into a chunk's `tags_json`
/// array. `recall`/`get`/`list` reverse this via `parse_tags` (Task 4).
fn build_tags(category: &MemoryCategory, session_id: Option<&str>) -> Vec<String> {
    let mut tags = Vec::with_capacity(2);
    let category_tag = match category {
        MemoryCategory::Core => "category:core".to_string(),
        MemoryCategory::Daily => "category:daily".to_string(),
        MemoryCategory::Conversation => "category:conversation".to_string(),
        MemoryCategory::Custom(s) => format!("category:custom:{s}"),
    };
    tags.push(category_tag);
    if let Some(s) = session_id {
        tags.push(format!("session:{s}"));
    }
    tags
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
}
