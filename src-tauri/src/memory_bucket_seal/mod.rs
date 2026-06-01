//! Bucket-seal memory backend (openhuman port).
//!
//! Standalone storage layer for chunks: atomic file writes under
//! `<content_root>/{chat,email,document}/<slug>/<chunk_id>.md` indexed by a
//! SQLite catalog at `<bucket_seal_dir>/chunks.db`. Build target for the
//! BucketSealAdapter in PR9; no AppState wiring or IPC at this stage.
//!
//! Faithful port of `openhuman::memory::tree` (atomic + paths + chunks-only
//! SQLite). Summaries, scoring, entity index, jobs, and the topic/global
//! trees follow in later PRs.

pub mod adapter;
pub mod atomic;
pub mod canonicalize;
pub mod chunker;
pub mod paths;
pub mod score;
pub mod store;
pub mod tree_source;
pub mod types;
pub mod util;

pub use adapter::BucketSealAdapter;
pub use canonicalize::{CanonicalisedSource, CanonicaliseRequest, normalize_source_ref};
pub use chunker::{chunk_markdown, ChunkerInput, ChunkerOptions, DEFAULT_CHUNK_MAX_TOKENS};
pub use score::embed::{Embedder, InertEmbedder, MemUEmbedder, EMBEDDING_DIM};
pub use score::{score_chunk, ScoreResult, ScoringConfig, DEFAULT_DROP_THRESHOLD};
pub use store::BucketSealStore;
pub use tree_source::{
    append_leaf, get_or_create_source_tree, Buffer, InertSummariser, LabelStrategy, LeafRef,
    Summariser, SummaryNode, Tree, TreeKind, TreeStatus, INPUT_TOKEN_BUDGET, OUTPUT_TOKEN_BUDGET,
    SUMMARY_FANOUT,
};
pub use types::{approx_token_count, chunk_id, Chunk, DataSource, Metadata, SourceKind, SourceRef};

use std::path::Path;

/// A chunk that has been written to disk and is ready for SQLite upsert.
///
/// Callers build a `Vec<StagedChunk>` from `stage_chunks`, then pass it to
/// `BucketSealStore::upsert_staged_chunks` in a single transaction.
#[derive(Debug, Clone)]
pub struct StagedChunk {
    /// The original chunk (metadata + content).
    pub chunk: Chunk,
    /// Relative content path (forward-slash, e.g. `"chat/slack-eng/0.md"`).
    pub content_path: String,
    /// SHA-256 hex digest over the body bytes only.
    pub content_sha256: String,
}

/// Write all chunks in `chunks` to disk and return `StagedChunk` records
/// ready for SQLite upsert.
///
/// Each chunk file is written atomically via a sibling temp-file + rename.
/// Already-existing files are skipped (immutable-body contract). Parent
/// directories are created on demand.
///
/// **Email chunks skip the disk write** — their body lives in the raw archive
/// (deferred to PR8+); we still emit a `StagedChunk` row with an empty
/// `content_path` so the SQLite upsert proceeds.
///
/// **Note**: at PR5 the chunk body is written as plain bytes (`chunk.content`
/// as-is), no YAML front-matter envelope. PR6 (`canonicalize + chunker`)
/// brings in the `compose_chunk_file` step that wraps the body with front-matter.
/// Until then, the SHA-256 is computed over the raw chunk content bytes.
pub fn stage_chunks(
    content_root: &Path,
    chunks: &[Chunk],
) -> anyhow::Result<Vec<StagedChunk>> {
    let mut staged = Vec::with_capacity(chunks.len());

    for chunk in chunks {
        if chunk.metadata.source_kind == SourceKind::Email {
            // Body lives in raw/<source>/<ts>_<id>.md — no chunk file at PR5.
            staged.push(StagedChunk {
                chunk: chunk.clone(),
                content_path: String::new(),
                content_sha256: String::new(),
            });
            continue;
        }

        let source_kind = chunk.metadata.source_kind.as_str();
        let source_id = &chunk.metadata.source_id;

        let rel_path = paths::chunk_rel_path(source_kind, source_id, &chunk.id);
        let abs_path = paths::chunk_abs_path(content_root, source_kind, source_id, &chunk.id);

        let body_bytes = chunk.content.as_bytes();
        let sha256 = atomic::sha256_hex(body_bytes);

        match atomic::write_if_new(&abs_path, body_bytes) {
            Ok(true) => {
                tracing::debug!(
                    chunk_id = %chunk.id,
                    rel_path = %rel_path,
                    "memory_bucket_seal: wrote chunk"
                );
            }
            Ok(false) => {
                tracing::debug!(
                    chunk_id = %chunk.id,
                    rel_path = %rel_path,
                    "memory_bucket_seal: chunk already on disk"
                );
            }
            Err(e) => {
                tracing::error!(
                    chunk_id = %chunk.id,
                    rel_path = %rel_path,
                    error = %e,
                    "memory_bucket_seal: failed to write chunk"
                );
                return Err(e);
            }
        }

        staged.push(StagedChunk {
            chunk: chunk.clone(),
            content_path: rel_path,
            content_sha256: sha256,
        });
    }

    Ok(staged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::TempDir;

    fn sample_chunk(seq: u32) -> Chunk {
        let ts = chrono::Utc
            .timestamp_millis_opt(1_700_000_000_000 + seq as i64)
            .unwrap();
        Chunk {
            id: format!("chunk_{seq:02}"),
            content: format!("## ts — alice\nMessage {seq}"),
            metadata: Metadata {
                source_kind: SourceKind::Chat,
                source_id: "slack:#eng".into(),
                owner: "alice".into(),
                timestamp: ts,
                time_range: (ts, ts),
                tags: vec![],
                source_ref: None,
            },
            token_count: 5,
            seq_in_source: seq,
            created_at: ts,
            partial_message: false,
        }
    }

    #[test]
    fn stage_chunks_writes_files_and_returns_staged() {
        let dir = TempDir::new().unwrap();
        let chunks = vec![sample_chunk(0), sample_chunk(1)];
        let staged = stage_chunks(dir.path(), &chunks).unwrap();

        assert_eq!(staged.len(), 2);
        for s in &staged {
            let abs = paths::chunk_abs_path(
                dir.path(),
                s.chunk.metadata.source_kind.as_str(),
                &s.chunk.metadata.source_id,
                &s.chunk.id,
            );
            assert!(abs.exists(), "file must exist: {}", abs.display());
            assert!(!s.content_path.is_empty());
            assert_eq!(s.content_sha256.len(), 64);
            assert!(!s.content_path.starts_with('/'));
            assert!(s.content_path.contains('/'));
        }
    }

    #[test]
    fn stage_chunks_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let chunks = vec![sample_chunk(0)];
        let first = stage_chunks(dir.path(), &chunks).unwrap();
        let second = stage_chunks(dir.path(), &chunks).unwrap();
        assert_eq!(first[0].content_sha256, second[0].content_sha256);
        assert_eq!(first[0].content_path, second[0].content_path);
    }

    #[test]
    fn stage_chunks_email_skips_disk_write() {
        let dir = TempDir::new().unwrap();
        let mut chunk = sample_chunk(0);
        chunk.metadata.source_kind = SourceKind::Email;
        chunk.metadata.source_id = "gmail:alice@x.com|bob@y.com".into();
        let staged = stage_chunks(dir.path(), &[chunk]).unwrap();
        assert_eq!(staged.len(), 1);
        assert!(staged[0].content_path.is_empty());
        assert!(staged[0].content_sha256.is_empty());
        // No file was written for email
        let email_dir = dir.path().join("email");
        assert!(!email_dir.exists(), "no email/ tree should be created at PR5");
    }

    #[test]
    fn end_to_end_chat_batch_to_score_admission() {
        use crate::memory_bucket_seal::canonicalize::chat::{canonicalise, ChatBatch, ChatMessage};
        use crate::memory_bucket_seal::chunker::{chunk_markdown, ChunkerInput, ChunkerOptions};
        use crate::memory_bucket_seal::score::store::{count_scores, get_score, upsert_score, ScoreRow};
        use crate::memory_bucket_seal::score::{score_chunk, ScoringConfig};
        use crate::memory_bucket_seal::store::BucketSealStore;
        use chrono::{TimeZone, Utc};

        // 1. Build a chat batch with substantive content
        let ts = Utc.timestamp_millis_opt(1_700_000_000_000).unwrap();
        let batch = ChatBatch {
            platform: "slack".to_string(),
            channel_label: "eng".to_string(),
            messages: vec![ChatMessage {
                author: "alice".to_string(),
                timestamp: ts,
                text: "Detailed technical message about the migration plan with sufficient \
                       signal density to produce a non-trivial score from the cheap-signals \
                       pipeline even without entity extraction or LLM scoring."
                    .to_string(),
                source_ref: None,
            }],
        };

        // 2. Canonicalise → chunk
        let canonical = canonicalise("slack:#eng", "alice", &[], batch)
            .unwrap()
            .expect("non-empty batch should produce CanonicalisedSource");
        let chunker_input = ChunkerInput {
            source_kind: canonical.metadata.source_kind,
            source_id: canonical.metadata.source_id.clone(),
            markdown: canonical.markdown.clone(),
            metadata: canonical.metadata.clone(),
        };
        let chunks = chunk_markdown(&chunker_input, &ChunkerOptions::default());
        assert_eq!(chunks.len(), 1);

        // 3. Score via cheap-signals path
        let result = score_chunk(&chunks[0], &ScoringConfig::default());
        assert_eq!(result.chunk_id, chunks[0].id);
        // entity_density is always 0 in PR7 (no extract)
        assert_eq!(result.signals.entity_density, 0.0);
        // llm_importance is always 0 in PR7 (no LLM)
        assert_eq!(result.signals.llm_importance, 0.0);

        // 4. Stage chunks + persist score row
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("chunks.db");
        let store = BucketSealStore::open(&db_path).unwrap();
        store.ensure_schema().unwrap();

        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();

        let row = ScoreRow {
            chunk_id: result.chunk_id.clone(),
            total: result.total,
            signals: result.signals.clone(),
            dropped: !result.kept,
            reason: result.drop_reason.clone(),
            computed_at_ms: chrono::Utc::now().timestamp_millis(),
        };
        upsert_score(&store, &row).unwrap();
        assert_eq!(count_scores(&store).unwrap(), 1);

        // 5. Round-trip via get_score
        let got = get_score(&store, &result.chunk_id)
            .unwrap()
            .expect("score should round-trip");
        assert_eq!(got.chunk_id, result.chunk_id);
        assert!((got.total - result.total).abs() < 1e-6);
        assert_eq!(got.dropped, !result.kept);
    }

    #[test]
    fn end_to_end_chat_batch_to_chunks_to_disk_to_sql() {
        use crate::memory_bucket_seal::canonicalize::chat::{canonicalise, ChatBatch, ChatMessage};
        use crate::memory_bucket_seal::chunker::{chunk_markdown, ChunkerInput, ChunkerOptions};
        use crate::memory_bucket_seal::store::BucketSealStore;
        use chrono::{TimeZone, Utc};

        // 1. Build a chat batch
        let ts = Utc.timestamp_millis_opt(1_700_000_000_000).unwrap();
        let batch = ChatBatch {
            platform: "slack".to_string(),
            channel_label: "eng".to_string(),
            messages: vec![
                ChatMessage {
                    author: "alice".to_string(),
                    timestamp: ts,
                    text: "first message".to_string(),
                    source_ref: None,
                },
                ChatMessage {
                    author: "bob".to_string(),
                    timestamp: ts,
                    text: "second message".to_string(),
                    source_ref: None,
                },
            ],
        };

        // 2. Canonicalise
        let canonical = canonicalise("slack:#eng", "alice", &[], batch)
            .unwrap()
            .expect("non-empty batch should produce CanonicalisedSource");
        assert!(canonical.markdown.contains("first message"));
        assert!(canonical.markdown.contains("second message"));

        // 3. Chunk
        let chunker_input = ChunkerInput {
            source_kind: canonical.metadata.source_kind,
            source_id: canonical.metadata.source_id.clone(),
            markdown: canonical.markdown.clone(),
            metadata: canonical.metadata.clone(),
        };
        let chunks = chunk_markdown(&chunker_input, &ChunkerOptions::default());
        assert!(!chunks.is_empty(), "should produce at least one chunk");
        // Two messages should fit in one chunk under DEFAULT_CHUNK_MAX_TOKENS = 3_000.
        assert_eq!(chunks.len(), 1);

        // 4. Stage to disk
        let dir = TempDir::new().unwrap();
        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        assert_eq!(staged.len(), 1);
        assert!(!staged[0].content_path.is_empty(), "chat chunks must have content_path");

        // 5. Upsert to SQLite
        let db_path = dir.path().join("chunks.db");
        let store = BucketSealStore::open(&db_path).unwrap();
        store.ensure_schema().unwrap();
        let n = store.upsert_staged_chunks(&staged).unwrap();
        assert_eq!(n, 1);
        assert_eq!(store.count_chunks().unwrap(), 1);

        // 6. Round-trip via get_chunk
        let got = store
            .get_chunk(&chunks[0].id)
            .unwrap()
            .expect("chunk should be retrievable by deterministic id");
        assert_eq!(got.metadata.source_id, "slack:#eng");
    }

    /// End-to-end: canonical chat batch → chunk → stage → `append_leaf` ×11
    /// → cascade-seal fires at L0 budget → L1 summary stored with embedding.
    ///
    /// This exercises the full PR1-PR8 stack without AppState or IPC:
    /// `stage_chunks` → `upsert_staged_chunks` → `get_or_create_source_tree`
    /// → `append_leaf` → `mem_tree_summaries` populated with embedding.
    #[tokio::test]
    async fn end_to_end_chat_batch_to_l1_seal() {
        use crate::memory_bucket_seal::score::embed::InertEmbedder;
        use crate::memory_bucket_seal::tree_source::{
            self as ts, store as ts_store, InertSummariser, LabelStrategy, LeafRef,
            INPUT_TOKEN_BUDGET,
        };
        use std::sync::Arc;

        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("chunks.db");
        let store = BucketSealStore::open(&db_path).unwrap();
        store.ensure_schema().unwrap();

        let tree = ts::get_or_create_source_tree(&store, "slack:#eng").unwrap();
        let summariser: Arc<dyn ts::Summariser> = Arc::new(InertSummariser::new());
        let embedder: Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> =
            Arc::new(InertEmbedder::new());

        // Seed 11 chunks into the store so hydrate_leaf_inputs can look them up.
        // Each gets token_count = INPUT_TOKEN_BUDGET / 6 so two together exceed
        // the budget. After 11 chunks at that size the token_sum is well over
        // INPUT_TOKEN_BUDGET, guaranteeing at least one L0→L1 seal.
        let per_chunk_tokens = INPUT_TOKEN_BUDGET / 6 + 1; // ~8334 tokens
        let mut sealed_ids: Vec<String> = Vec::new();
        for seq in 0u32..11 {
            let ts = chrono::Utc
                .timestamp_millis_opt(1_700_000_000_000 + seq as i64 * 1000)
                .unwrap();
            let chunk = Chunk {
                id: format!("e2e_chunk_{seq:04}"),
                content: format!("end-to-end test chunk {seq} with enough content"),
                metadata: Metadata {
                    source_kind: SourceKind::Chat,
                    source_id: "slack:#eng".into(),
                    owner: "alice".into(),
                    timestamp: ts,
                    time_range: (ts, ts),
                    tags: vec![],
                    source_ref: None,
                },
                token_count: per_chunk_tokens,
                seq_in_source: seq,
                created_at: ts,
                partial_message: false,
            };
            let staged = stage_chunks(dir.path(), &[chunk.clone()]).unwrap();
            store.upsert_staged_chunks(&staged).unwrap();

            let leaf = LeafRef {
                chunk_id: chunk.id,
                token_count: per_chunk_tokens,
                timestamp: ts,
                content: chunk.content,
                entities: vec![],
                topics: vec![],
                score: 0.8,
            };
            let result = append_leaf(&store, &tree, &leaf, &summariser, &embedder, &LabelStrategy::Empty)
                .await
                .unwrap();
            sealed_ids.extend(result);
        }

        // At least one L1 summary must have sealed.
        assert!(
            !sealed_ids.is_empty(),
            "cascade-seal should fire at least one L1 summary"
        );
        assert_eq!(
            ts_store::count_summaries(&store, &tree.id).unwrap(),
            sealed_ids.len() as u64
        );

        // All summaries should be at level 1.
        let l1 = ts_store::list_summaries_at_level(&store, &tree.id, 1).unwrap();
        assert_eq!(l1.len(), sealed_ids.len(), "all seals should land at L1");

        // Each summary must have embedding populated (InertEmbedder always returns 1024 zeros).
        for s in &l1 {
            assert!(s.embedding.is_some(), "PR8 summaries must have embedding populated");
            assert_eq!(
                s.embedding.as_ref().unwrap().len(),
                EMBEDDING_DIM,
                "embedding dimension must match EMBEDDING_DIM"
            );
        }

        // Tree metadata must reflect the seal.
        let refreshed = ts_store::get_tree(&store, &tree.id).unwrap().unwrap();
        assert!(refreshed.last_sealed_at.is_some(), "last_sealed_at must be set after seal");
        assert!(refreshed.max_level >= 1, "max_level must reach at least 1");
        assert!(refreshed.root_id.is_some(), "root_id must be set after first seal");
    }

    /// End-to-end round trip through the `Arc<dyn MemoryAdapter>` trait surface:
    /// store → recall → get → list → namespace_summaries → delete → clear.
    /// This is the contract the unified `memory.unified.*` IPC layer drives.
    #[tokio::test]
    async fn end_to_end_bucket_seal_adapter_via_trait_surface() {
        use crate::memory_adapter::{MemoryAdapter, MemoryCategory, RecallOpts};
        use crate::memory_bucket_seal::adapter::BucketSealAdapter;
        use crate::memory_bucket_seal::score::embed::{Embedder, InertEmbedder};
        use crate::memory_bucket_seal::store::BucketSealStore;
        use crate::memory_bucket_seal::tree_source::{InertSummariser, Summariser};
        use std::sync::Arc;

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

        // store → recall → get → list → namespace_summaries → delete → clear
        adapter
            .store(
                "e2e_ns",
                "k1",
                "Project Phoenix launch plan and milestones.",
                MemoryCategory::Core,
                Some("sess1"),
            )
            .await
            .unwrap();
        adapter
            .store(
                "e2e_ns",
                "k2",
                "Unrelated weather note today.",
                MemoryCategory::Daily,
                None,
            )
            .await
            .unwrap();

        let opts = RecallOpts {
            namespace: Some("e2e_ns"),
            category: None,
            session_id: None,
            min_score: None,
        };
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
}
