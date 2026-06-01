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

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::memory_adapter::{
    MemoryAdapter, MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts,
};
use crate::memory_bucket_seal::score::embed::Embedder;
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_source::Summariser;

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
