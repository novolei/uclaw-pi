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
use chrono::{TimeZone, Utc};
use rusqlite::OptionalExtension;
use tokio::sync::Mutex;

use crate::memory_adapter::{
    MemoryAdapter, MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts,
};
use crate::memory_bucket_seal::canonicalize::document::{canonicalise, DocumentInput};
use crate::memory_bucket_seal::chunker::{chunk_markdown, ChunkerInput, ChunkerOptions};
use crate::memory_bucket_seal::score::embed::{cosine_similarity, unpack_embedding, Embedder};
use crate::memory_bucket_seal::score::store::{upsert_score, ScoreRow};
use crate::memory_bucket_seal::score::{score_chunk, ScoringConfig};
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_source::{
    append_leaf, get_or_create_source_tree, LabelStrategy, LeafRef, Summariser,
};
use crate::memory_bucket_seal::{stage_chunks, types::SourceKind};

const ADAPTER_NAME: &str = "bucket_seal";

/// Max rows returned by `list()` (the trait takes no limit). A namespace with
/// more chunks than this is silently truncated — callers needing exhaustive
/// enumeration should page via a future API.
const LIST_LIMIT: usize = 200;

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

    /// Semantic recall over the sealed summaries of `namespace`'s tree: embed the
    /// raw query, cosine-rank the summary embeddings, return the top `limit`.
    /// Best-effort — returns empty (so `recall` falls back to FTS5) on embed
    /// error, missing tree/summaries, or zero-signal (e.g. the inert embedder,
    /// whose zero query vector scores 0 against everything). Never errors.
    async fn recall_vector(&self, query: &str, namespace: &str, limit: usize) -> Vec<MemoryEntry> {
        let query_vec = match self.embedder.embed(query).await {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(error = %e, "bucket_seal: query embed failed; skipping vector recall");
                return Vec::new();
            }
        };

        // Gather (id, content, embedding) for this tree's non-deleted, embedded
        // summaries. Sync block (no await holds the conn lock).
        let cands: Vec<(String, String, Vec<f32>)> = (|| -> Result<Vec<(String, String, Vec<f32>)>> {
            let tree = get_or_create_source_tree(&self.store, namespace)?;
            let conn = self.store.lock_conn()?;
            let mut stmt = conn.prepare(
                "SELECT id, content, embedding FROM mem_tree_summaries
                  WHERE tree_id = ?1 AND embedding IS NOT NULL AND deleted = 0",
            )?;
            let rows = stmt.query_map(rusqlite::params![tree.id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                ))
            })?;
            let mut v = Vec::new();
            for row in rows {
                let (id, content, blob) = row?;
                match unpack_embedding(&blob) {
                    Ok(emb) => v.push((id, content, emb)),
                    // A stale wrong-dim blob (e.g. a pre-384 inert seal) is
                    // skipped, not fatal.
                    Err(e) => tracing::debug!(error = %e, "bucket_seal: skip summary with bad embedding"),
                }
            }
            Ok(v)
        })()
        .unwrap_or_else(|e| {
            tracing::debug!(error = %e, "bucket_seal: vector candidate fetch failed");
            Vec::new()
        });

        rank_by_cosine(&query_vec, cands, limit)
            .into_iter()
            .map(|(id, content, score)| MemoryEntry {
                id,
                key: String::new(),
                content,
                namespace: Some(namespace.to_string()),
                category: MemoryCategory::Core,
                timestamp: String::new(),
                session_id: None,
                score: Some(score as f64),
            })
            .collect()
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
            // Build a score row for every chunk; only admitted (staged) ones are
            // persisted below — the mem_tree_score FK requires the chunk row.
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

    /// Hybrid recall: semantic (cosine over sealed-summary embeddings, when a
    /// real embedder + seals are present) merged with FTS5 keyword over chunk
    /// previews, scoped by `opts.namespace` and (post-filter) `opts.category`.
    /// Vector hits carry `score = Some(cosine)`; FTS5 hits have `score = None`.
    /// `opts.min_score` is still not applied. The vector path degrades to empty
    /// (FTS5-only) on embed error / inert zero-vectors / no seals.
    async fn recall(
        &self,
        query: &str,
        limit: usize,
        opts: RecallOpts<'_>,
    ) -> Result<Vec<MemoryEntry>> {
        // No usable query text ⇒ nothing (both paths need real query text;
        // FTS5 phrase-quoting also can't form a MATCH from empty input).
        let Some(match_query) = sanitize_fts_query(query) else {
            return Ok(Vec::new());
        };

        let mut out: Vec<MemoryEntry> = Vec::new();

        // 1) Semantic recall over the namespace tree's sealed summaries. Embeds
        //    the raw query; degrades to empty on embed error / inert zero-vectors
        //    / no seals, so it's purely additive over the FTS5 floor below.
        if let Some(ns) = opts.namespace {
            out.extend(self.recall_vector(query, ns, limit).await);
        }

        // 2) FTS5 keyword recall over chunk previews; dedup against vector hits.
        {
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
                rusqlite::params![match_query, ns_param, limit as i64],
                row_to_memory_entry,
            )?;
            for row in rows {
                let entry = row?;
                if out.iter().any(|e| e.id == entry.id) {
                    continue; // already surfaced by the vector path
                }
                out.push(entry);
            }
        }

        // 3) Optional category filter (applies to both paths) + cap to `limit`.
        if let Some(filter) = &opts.category {
            out.retain(|e| &e.category == filter);
        }
        out.truncate(limit);
        Ok(out)
    }

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
            params.push(Box::new(format!("%\"session:{s}\"%")));
        }

        sql.push_str(" ORDER BY timestamp_ms DESC LIMIT ?");
        params.push(Box::new(LIST_LIMIT as i64));

        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(&params_refs[..], row_to_memory_entry)?;

        let mut out: Vec<MemoryEntry> = Vec::new();
        for row in rows {
            let entry = row?;
            // Category filter is applied in Rust (encoded in tags_json, not a column).
            if let Some(filter) = category {
                if entry.category != *filter {
                    continue;
                }
            }
            out.push(entry);
        }
        Ok(out)
    }

    async fn delete(&self, namespace: &str, key: &str) -> Result<bool> {
        let mut conn = self.store.lock_conn()?;
        let tx = conn.transaction()?;
        // mem_tree_score has an FK → mem_tree_chunks(id) with no ON DELETE
        // CASCADE (foreign_keys=ON), so clear the score rows before deleting
        // the chunks they reference. The FTS delete trigger then prunes
        // mem_tree_chunks_fts. (Stale L0-buffer item_ids are tolerated — the
        // seal path skips missing chunks.)
        tx.execute(
            "DELETE FROM mem_tree_score
              WHERE chunk_id IN (
                  SELECT id FROM mem_tree_chunks WHERE source_id = ?1 AND source_ref = ?2
              )",
            rusqlite::params![namespace, key],
        )?;
        let n = tx.execute(
            "DELETE FROM mem_tree_chunks WHERE source_id = ?1 AND source_ref = ?2",
            rusqlite::params![namespace, key],
        )?;
        tx.commit()?;
        Ok(n > 0)
    }

    async fn clear_namespace(&self, namespace: &str) -> Result<u64> {
        let mut conn = self.store.lock_conn()?;
        let tx = conn.transaction()?;
        // Same FK ordering as delete(): score rows first, then chunks.
        tx.execute(
            "DELETE FROM mem_tree_score
              WHERE chunk_id IN (SELECT id FROM mem_tree_chunks WHERE source_id = ?1)",
            rusqlite::params![namespace],
        )?;
        let n = tx.execute(
            "DELETE FROM mem_tree_chunks WHERE source_id = ?1",
            rusqlite::params![namespace],
        )?;
        tx.commit()?;
        Ok(n as u64)
    }

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
            let last_updated = last_updated_ms
                .and_then(|ms| Utc.timestamp_millis_opt(ms).single().map(|dt| dt.to_rfc3339()));
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

/// Rank `(id, content, embedding)` candidates by cosine similarity to `query`,
/// descending, dropping non-positive scores (zero-magnitude inert vectors +
/// uncorrelated rows), and take the top `limit`.
fn rank_by_cosine(
    query: &[f32],
    cands: Vec<(String, String, Vec<f32>)>,
    limit: usize,
) -> Vec<(String, String, f32)> {
    let mut scored: Vec<(String, String, f32)> = cands
        .into_iter()
        .map(|(id, content, emb)| {
            let s = cosine_similarity(query, &emb);
            (id, content, s)
        })
        .filter(|(_, _, s)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}

/// Turn arbitrary user text into a safe FTS5 MATCH expression: split on
/// whitespace, escape embedded `"`, wrap each token as a quoted phrase (so any
/// FTS5 operator/colon/paren inside matches literally instead of raising a
/// syntax error), and AND the tokens together. Returns `None` when no usable
/// token remains, so the caller can return an empty result rather than letting
/// an empty MATCH error.
fn sanitize_fts_query(query: &str) -> Option<String> {
    let quoted: Vec<String> = query
        .split_whitespace()
        .filter(|tok| !tok.is_empty())
        .map(|tok| format!("\"{}\"", tok.replace('"', "\"\"")))
        .collect();
    if quoted.is_empty() {
        None
    } else {
        Some(quoted.join(" "))
    }
}

/// Reverse [`build_tags`]: pull the `MemoryCategory` + optional session id back
/// out of a chunk's `tags_json` array. Unknown / missing category tags fall
/// back to `Custom("unknown")`.
fn parse_tags(tags: &[String]) -> (MemoryCategory, Option<String>) {
    let mut category = MemoryCategory::Custom("unknown".to_string());
    let mut session = None;
    for tag in tags {
        if let Some(rest) = tag.strip_prefix("category:") {
            category = match rest {
                "core" => MemoryCategory::Core,
                "daily" => MemoryCategory::Daily,
                "conversation" => MemoryCategory::Conversation,
                _ => match rest.strip_prefix("custom:") {
                    Some(custom) => MemoryCategory::Custom(custom.to_string()),
                    None => MemoryCategory::Custom(rest.to_string()),
                },
            };
        } else if let Some(rest) = tag.strip_prefix("session:") {
            session = Some(rest.to_string());
        }
    }
    (category, session)
}

/// Hydrate a `MemoryEntry` from the `c.*` columns of a recall/get/list query:
/// `(id, source_id, source_ref, content, timestamp_ms, tags_json)`. The
/// chunk's `source_id` becomes the entry namespace and `source_ref` the key
/// (the inverse of `store`'s mapping).
fn row_to_memory_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEntry> {
    let id: String = row.get(0)?;
    let source_id: String = row.get(1)?;
    let source_ref: Option<String> = row.get(2)?;
    let content: String = row.get(3)?;
    let timestamp_ms: i64 = row.get(4)?;
    let tags_json: String = row.get(5)?;

    let tags: Vec<String> = serde_json::from_str(&tags_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let (category, session_id) = parse_tags(&tags);

    let timestamp = Utc
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
        key: source_ref.unwrap_or_default(),
        content,
        namespace: Some(source_id),
        category,
        timestamp,
        session_id,
        score: None,
    })
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
        assert!(hits_a.iter().all(|e| e.namespace.as_deref() == Some("ns_a")));
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
        assert!(listed.iter().all(|e| e.namespace.as_deref() == Some("nslA")));
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

    #[tokio::test]
    async fn recall_tolerates_punctuation_and_empty_queries() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store(
                "punc_ns",
                "k1",
                "Visit https://example.com about the phoenix launch.",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();
        let opts = RecallOpts {
            namespace: Some("punc_ns"),
            category: None,
            session_id: None,
            min_score: None,
        };

        // FTS5 operators / colons / dashes / parens / quotes must NOT raise an error.
        for q in ["https://example.com", "phoenix -launch", "a:b", "well (being", "\""] {
            assert!(
                adapter.recall(q, 10, opts.clone()).await.is_ok(),
                "query {q:?} must not raise an FTS5 error"
            );
        }

        // Empty / whitespace-only query returns an empty result, not an error.
        assert!(adapter.recall("   ", 10, opts.clone()).await.unwrap().is_empty());

        // A normal token still matches.
        let hits = adapter.recall("phoenix", 10, opts.clone()).await.unwrap();
        assert!(!hits.is_empty(), "plain token should still match after sanitisation");
    }

    #[test]
    fn rank_by_cosine_orders_desc_and_drops_nonpositive() {
        // query ≈ A (same direction), orthogonal to B, opposite to C.
        let q = vec![1.0_f32, 0.0, 0.0];
        let cands = vec![
            ("a".to_string(), "near".to_string(), vec![0.9_f32, 0.1, 0.0]),
            ("b".to_string(), "ortho".to_string(), vec![0.0_f32, 1.0, 0.0]),
            ("c".to_string(), "opposite".to_string(), vec![-1.0_f32, 0.0, 0.0]),
        ];
        let ranked = rank_by_cosine(&q, cands, 10);
        // A first (positive cosine); B (cosine 0) and C (negative) dropped.
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].0, "a");
        assert!(ranked[0].2 > 0.0);
    }

    #[test]
    fn rank_by_cosine_respects_limit() {
        let q = vec![1.0_f32, 1.0];
        let cands = vec![
            ("a".to_string(), "x".to_string(), vec![1.0_f32, 1.0]),
            ("b".to_string(), "y".to_string(), vec![0.9_f32, 1.0]),
            ("c".to_string(), "z".to_string(), vec![0.8_f32, 1.0]),
        ];
        assert_eq!(rank_by_cosine(&q, cands, 2).len(), 2);
    }

    #[test]
    fn rank_by_cosine_zero_query_is_empty() {
        // Inert embedder → zero query vector → cosine 0 everywhere → empty.
        let q = vec![0.0_f32, 0.0, 0.0];
        let cands = vec![("a".to_string(), "x".to_string(), vec![1.0_f32, 2.0, 3.0])];
        assert!(rank_by_cosine(&q, cands, 10).is_empty());
    }
}
