//! SQLite catalog for bucket-seal chunks.
//!
//! Faithful port of `openhuman::memory::tree::store` slimmed to chunks-only:
//! drops summary trees, score, entity index, jobs, raw refs, embeddings,
//! lifecycle status, ingest-source gate. PR5 builds the foundation; PR6-12
//! restore the deferred surface in their own slices.
//!
//! Schema is applied lazily on `ensure_schema()`. The DB lives at a path
//! given by the caller (typically `<app_data_dir>/bucket_seal/chunks.db`).
//! The store wraps `Arc<Mutex<Connection>>` so multiple async tasks can
//! call into it.

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::memory_bucket_seal::types::{Chunk, Metadata, SourceKind, SourceRef};
use crate::memory_bucket_seal::StagedChunk;

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(15);

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS mem_tree_chunks (
    id                     TEXT PRIMARY KEY,
    source_kind            TEXT NOT NULL,
    source_id              TEXT NOT NULL,
    source_ref             TEXT,
    owner                  TEXT NOT NULL,
    timestamp_ms           INTEGER NOT NULL,
    time_range_start_ms    INTEGER NOT NULL,
    time_range_end_ms      INTEGER NOT NULL,
    tags_json              TEXT NOT NULL DEFAULT '[]',
    content                TEXT NOT NULL,
    token_count            INTEGER NOT NULL,
    seq_in_source          INTEGER NOT NULL,
    created_at_ms          INTEGER NOT NULL,
    content_path           TEXT NOT NULL DEFAULT '',
    content_sha256         TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_mem_tree_chunks_source
    ON mem_tree_chunks(source_kind, source_id);
CREATE INDEX IF NOT EXISTS idx_mem_tree_chunks_timestamp
    ON mem_tree_chunks(timestamp_ms);
CREATE INDEX IF NOT EXISTS idx_mem_tree_chunks_owner
    ON mem_tree_chunks(owner);
CREATE INDEX IF NOT EXISTS idx_mem_tree_chunks_source_seq
    ON mem_tree_chunks(source_kind, source_id, seq_in_source);

CREATE TABLE IF NOT EXISTS mem_tree_score (
    chunk_id               TEXT PRIMARY KEY,
    total                  REAL NOT NULL,
    token_count_signal     REAL NOT NULL,
    unique_words_signal    REAL NOT NULL,
    metadata_weight        REAL NOT NULL,
    source_weight          REAL NOT NULL,
    interaction_weight     REAL NOT NULL,
    entity_density         REAL NOT NULL,
    llm_importance         REAL NOT NULL DEFAULT 0.0,
    dropped                INTEGER NOT NULL DEFAULT 0,
    reason                 TEXT,
    computed_at_ms         INTEGER NOT NULL,
    FOREIGN KEY (chunk_id) REFERENCES mem_tree_chunks(id)
);

CREATE INDEX IF NOT EXISTS idx_mem_tree_score_total
    ON mem_tree_score(total);
CREATE INDEX IF NOT EXISTS idx_mem_tree_score_dropped
    ON mem_tree_score(dropped);

CREATE TABLE IF NOT EXISTS mem_tree_trees (
    id                     TEXT PRIMARY KEY,
    kind                   TEXT NOT NULL,
    scope                  TEXT NOT NULL,
    root_id                TEXT,
    max_level              INTEGER NOT NULL DEFAULT 0,
    status                 TEXT NOT NULL DEFAULT 'active',
    created_at_ms          INTEGER NOT NULL,
    last_sealed_at_ms      INTEGER
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_mem_tree_trees_kind_scope
    ON mem_tree_trees(kind, scope);
CREATE INDEX IF NOT EXISTS idx_mem_tree_trees_status
    ON mem_tree_trees(status);

CREATE TABLE IF NOT EXISTS mem_tree_summaries (
    id                     TEXT PRIMARY KEY,
    tree_id                TEXT NOT NULL,
    tree_kind              TEXT NOT NULL,
    level                  INTEGER NOT NULL,
    parent_id              TEXT,
    child_ids_json         TEXT NOT NULL DEFAULT '[]',
    content                TEXT NOT NULL,
    token_count            INTEGER NOT NULL,
    entities_json          TEXT NOT NULL DEFAULT '[]',
    topics_json            TEXT NOT NULL DEFAULT '[]',
    time_range_start_ms    INTEGER NOT NULL,
    time_range_end_ms      INTEGER NOT NULL,
    score                  REAL NOT NULL DEFAULT 0.0,
    sealed_at_ms           INTEGER NOT NULL,
    deleted                INTEGER NOT NULL DEFAULT 0,
    embedding              BLOB,
    FOREIGN KEY (tree_id) REFERENCES mem_tree_trees(id)
);

CREATE INDEX IF NOT EXISTS idx_mem_tree_summaries_tree_level
    ON mem_tree_summaries(tree_id, level);
CREATE INDEX IF NOT EXISTS idx_mem_tree_summaries_parent
    ON mem_tree_summaries(parent_id);
CREATE INDEX IF NOT EXISTS idx_mem_tree_summaries_sealed_at
    ON mem_tree_summaries(sealed_at_ms);
CREATE INDEX IF NOT EXISTS idx_mem_tree_summaries_deleted
    ON mem_tree_summaries(deleted);

CREATE TABLE IF NOT EXISTS mem_tree_buffers (
    tree_id                TEXT NOT NULL,
    level                  INTEGER NOT NULL,
    item_ids_json          TEXT NOT NULL DEFAULT '[]',
    token_sum              INTEGER NOT NULL DEFAULT 0,
    oldest_at_ms           INTEGER,
    updated_at_ms          INTEGER NOT NULL,
    PRIMARY KEY (tree_id, level),
    FOREIGN KEY (tree_id) REFERENCES mem_tree_trees(id)
);

CREATE INDEX IF NOT EXISTS idx_mem_tree_buffers_oldest
    ON mem_tree_buffers(oldest_at_ms);

-- FTS5 virtual table backing keyword search in BucketSealAdapter::recall.
-- Mirrors a subset of mem_tree_chunks columns; kept in sync via triggers.
-- NOTE: `content` here is the ≤500-char preview persisted to mem_tree_chunks
-- (the full body lives on disk at content_path), so FTS matches previews only.
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
";

const DEFAULT_LIST_LIMIT: usize = 100;
const MAX_LIST_LIMIT: usize = 10_000;

/// Thread-safe SQLite store for bucket-seal chunks.
///
/// Constructed via [`BucketSealStore::open`]; schema must be applied with
/// [`BucketSealStore::ensure_schema`] before the first write. Clone is cheap
/// (wraps `Arc`).
#[derive(Clone)]
pub struct BucketSealStore {
    conn: Arc<Mutex<Connection>>,
}

impl BucketSealStore {
    /// Acquire the connection mutex. Returns an error instead of panicking when
    /// the mutex is poisoned (i.e., a prior holder panicked while holding the
    /// guard).
    pub(crate) fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| anyhow::anyhow!("memory_bucket_seal: connection mutex poisoned"))
    }

    /// Open (or create) the chunks.db at `db_path`. Sets busy_timeout and
    /// returns the store. Schema is NOT applied — call `ensure_schema()`
    /// before the first write.
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create_dir_all {:?}", parent))?;
        }
        let conn = Connection::open(db_path)
            .with_context(|| format!("open {:?}", db_path))?;
        conn.busy_timeout(SQLITE_BUSY_TIMEOUT)
            .context("set busy_timeout")?;
        conn.pragma_update(None, "foreign_keys", true)
            .context("set foreign_keys pragma")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Create the schema if it doesn't exist. Safe to call repeatedly
    /// (`CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS` are idempotent).
    pub fn ensure_schema(&self) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute_batch(SCHEMA).context("apply SCHEMA")?;
        Ok(())
    }

    /// Upsert a batch of staged chunks atomically.
    ///
    /// Returns the number of rows inserted or replaced. Re-running with the
    /// same `chunk.id` is idempotent (UPSERT on PK). The SQL `content`
    /// column stores a ≤500-char plain-text preview; the full body lives at
    /// `content_path` on disk.
    pub fn upsert_staged_chunks(&self, staged: &[StagedChunk]) -> Result<usize> {
        if staged.is_empty() {
            return Ok(0);
        }
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction().context("begin transaction")?;
        let inserted = {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO mem_tree_chunks (
                        id, source_kind, source_id, source_ref, owner,
                        timestamp_ms, time_range_start_ms, time_range_end_ms,
                        tags_json, content, token_count, seq_in_source, created_at_ms,
                        content_path, content_sha256
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                    ON CONFLICT(id) DO UPDATE SET
                        source_kind = excluded.source_kind,
                        source_id = excluded.source_id,
                        source_ref = excluded.source_ref,
                        owner = excluded.owner,
                        timestamp_ms = excluded.timestamp_ms,
                        time_range_start_ms = excluded.time_range_start_ms,
                        time_range_end_ms = excluded.time_range_end_ms,
                        tags_json = excluded.tags_json,
                        content = excluded.content,
                        token_count = excluded.token_count,
                        seq_in_source = excluded.seq_in_source,
                        created_at_ms = excluded.created_at_ms,
                        content_path = excluded.content_path,
                        content_sha256 = excluded.content_sha256",
                )
                .context("prepare upsert")?;

            for s in staged {
                let chunk = &s.chunk;
                let preview: String = chunk.content.chars().take(500).collect();
                stmt.execute(params![
                    chunk.id,
                    chunk.metadata.source_kind.as_str(),
                    chunk.metadata.source_id,
                    chunk.metadata.source_ref.as_ref().map(|r| r.value.as_str()),
                    chunk.metadata.owner,
                    chunk.metadata.timestamp.timestamp_millis(),
                    chunk.metadata.time_range.0.timestamp_millis(),
                    chunk.metadata.time_range.1.timestamp_millis(),
                    serde_json::to_string(&chunk.metadata.tags)
                        .context("serialize tags")?,
                    preview,
                    chunk.token_count,
                    chunk.seq_in_source,
                    chunk.created_at.timestamp_millis(),
                    s.content_path,
                    s.content_sha256,
                ])
                .context("execute upsert")?;
            }
            staged.len()
        };
        tx.commit().context("commit transaction")?;
        Ok(inserted)
    }

    /// Fetch one chunk by its id. Returns `None` if no row matches.
    ///
    /// Note: the returned `Chunk.content` is the SQL-stored preview (≤500
    /// chars). To read the full body, resolve `content_path` against the
    /// content root (PR6+ via the BucketSealAdapter).
    pub fn get_chunk(&self, id: &str) -> Result<Option<Chunk>> {
        let conn = self.lock_conn()?;
        let row = conn
            .query_row(
                "SELECT id, source_kind, source_id, source_ref, owner,
                        timestamp_ms, time_range_start_ms, time_range_end_ms,
                        tags_json, content, token_count, seq_in_source, created_at_ms
                   FROM mem_tree_chunks WHERE id = ?1",
                params![id],
                row_to_chunk,
            )
            .optional()
            .context("query chunk by id")?;
        Ok(row)
    }

    /// List chunks scoped to a specific source, ordered by `seq_in_source`
    /// ascending. `limit` clamps to `MAX_LIST_LIMIT` defensively.
    pub fn list_chunks_by_source(
        &self,
        source_kind: SourceKind,
        source_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Chunk>> {
        let effective_limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).min(MAX_LIST_LIMIT);
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, source_kind, source_id, source_ref, owner,
                        timestamp_ms, time_range_start_ms, time_range_end_ms,
                        tags_json, content, token_count, seq_in_source, created_at_ms
                   FROM mem_tree_chunks
                  WHERE source_kind = ?1 AND source_id = ?2
                  ORDER BY seq_in_source ASC
                  LIMIT ?3",
            )
            .context("prepare list_chunks_by_source")?;
        let rows = stmt
            .query_map(
                params![source_kind.as_str(), source_id, effective_limit as i64],
                row_to_chunk,
            )
            .context("query list_chunks_by_source")?
            .collect::<rusqlite::Result<Vec<Chunk>>>()
            .context("collect chunks")?;
        Ok(rows)
    }

    /// Total chunk count across all sources.
    pub fn count_chunks(&self) -> Result<u64> {
        let conn = self.lock_conn()?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM mem_tree_chunks", [], |r| r.get(0))
            .context("count_chunks")?;
        Ok(n as u64)
    }
}

// row_to_chunk lives at module level so list_chunks_by_source can pass it as
// query_map's row mapper without runtime closure indirection.
fn row_to_chunk(row: &rusqlite::Row<'_>) -> rusqlite::Result<Chunk> {
    let id: String = row.get(0)?;
    let source_kind_str: String = row.get(1)?;
    let source_id: String = row.get(2)?;
    let source_ref_str: Option<String> = row.get(3)?;
    let owner: String = row.get(4)?;
    let timestamp_ms: i64 = row.get(5)?;
    let tr_start_ms: i64 = row.get(6)?;
    let tr_end_ms: i64 = row.get(7)?;
    let tags_json: String = row.get(8)?;
    let content: String = row.get(9)?;
    let token_count: i64 = row.get(10)?;
    let seq_in_source: i64 = row.get(11)?;
    let created_at_ms: i64 = row.get(12)?;

    let source_kind = SourceKind::parse(&source_kind_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            e.into(),
        )
    })?;
    let timestamp = ms_to_utc(timestamp_ms, 5)?;
    let tr_start = ms_to_utc(tr_start_ms, 6)?;
    let tr_end = ms_to_utc(tr_end_ms, 7)?;
    let created_at = ms_to_utc(created_at_ms, 12)?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let source_ref = source_ref_str.map(SourceRef::new);

    Ok(Chunk {
        id,
        content,
        metadata: Metadata {
            source_kind,
            source_id,
            owner,
            timestamp,
            time_range: (tr_start, tr_end),
            tags,
            source_ref,
        },
        token_count: token_count.max(0) as u32,
        seq_in_source: seq_in_source.max(0) as u32,
        created_at,
        // partial_message is not persisted in the PR5 slim schema — it's a
        // transient chunker signal. Chunks read back from DB always get false.
        partial_message: false,
    })
}

fn ms_to_utc(ms: i64, col: usize) -> rusqlite::Result<DateTime<Utc>> {
    Utc.timestamp_millis_opt(ms).single().ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            col,
            rusqlite::types::Type::Integer,
            format!("invalid timestamp ms {ms}").into(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::{stage_chunks, StagedChunk};
    use chrono::TimeZone;
    use tempfile::TempDir;

    fn sample_chunk(seq: u32) -> Chunk {
        let ts = Utc.timestamp_millis_opt(1_700_000_000_000 + seq as i64).unwrap();
        Chunk {
            id: format!("chunk_{seq:02}"),
            content: format!("Message {seq} body"),
            metadata: Metadata {
                source_kind: SourceKind::Chat,
                source_id: "slack:#eng".into(),
                owner: "alice".into(),
                timestamp: ts,
                time_range: (ts, ts),
                tags: vec!["foo".into()],
                source_ref: None,
            },
            token_count: 4,
            seq_in_source: seq,
            created_at: ts,
            partial_message: false,
        }
    }

    fn fresh_store() -> (BucketSealStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("chunks.db");
        let store = BucketSealStore::open(&db_path).unwrap();
        store.ensure_schema().unwrap();
        (store, dir)
    }

    #[test]
    fn ensure_schema_is_idempotent() {
        let (store, _dir) = fresh_store();
        store.ensure_schema().unwrap();
        store.ensure_schema().unwrap();
        assert_eq!(store.count_chunks().unwrap(), 0);
    }

    #[test]
    fn upsert_then_get_round_trip() {
        let (store, dir) = fresh_store();
        let chunks = vec![sample_chunk(0), sample_chunk(1)];
        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        let n = store.upsert_staged_chunks(&staged).unwrap();
        assert_eq!(n, 2);

        let got = store.get_chunk("chunk_00").unwrap().unwrap();
        assert_eq!(got.id, "chunk_00");
        assert_eq!(got.metadata.source_id, "slack:#eng");
        assert_eq!(got.token_count, 4);
        assert_eq!(got.metadata.tags, vec!["foo".to_string()]);
    }

    #[test]
    fn upsert_is_idempotent_on_chunk_id() {
        let (store, dir) = fresh_store();
        let chunks = vec![sample_chunk(0)];
        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();
        assert_eq!(store.count_chunks().unwrap(), 1);
    }

    #[test]
    fn list_chunks_by_source_orders_by_seq() {
        let (store, dir) = fresh_store();
        let chunks = vec![sample_chunk(2), sample_chunk(0), sample_chunk(1)];
        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();

        let listed = store
            .list_chunks_by_source(SourceKind::Chat, "slack:#eng", None)
            .unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].seq_in_source, 0);
        assert_eq!(listed[1].seq_in_source, 1);
        assert_eq!(listed[2].seq_in_source, 2);
    }

    #[test]
    fn list_chunks_respects_limit() {
        let (store, dir) = fresh_store();
        let chunks: Vec<_> = (0..5).map(sample_chunk).collect();
        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();
        let listed = store
            .list_chunks_by_source(SourceKind::Chat, "slack:#eng", Some(2))
            .unwrap();
        assert_eq!(listed.len(), 2);
    }

    #[test]
    fn get_chunk_returns_none_when_missing() {
        let (store, _dir) = fresh_store();
        assert!(store.get_chunk("missing").unwrap().is_none());
    }

    #[test]
    fn count_chunks_reflects_writes() {
        let (store, dir) = fresh_store();
        assert_eq!(store.count_chunks().unwrap(), 0);
        let chunks = vec![sample_chunk(0), sample_chunk(1), sample_chunk(2)];
        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();
        assert_eq!(store.count_chunks().unwrap(), 3);
    }

    #[test]
    fn upsert_replaces_values_on_conflict() {
        let (store, dir) = fresh_store();

        let chunk_v1 = sample_chunk(0);
        let staged_v1 = stage_chunks(dir.path(), &[chunk_v1.clone()]).unwrap();
        store.upsert_staged_chunks(&staged_v1).unwrap();

        // Re-stage with the SAME id but different content + token_count.
        let mut chunk_v2 = chunk_v1.clone();
        chunk_v2.content = "REPLACED CONTENT".to_string();
        chunk_v2.token_count = 99;
        let staged_v2 = stage_chunks(dir.path(), &[chunk_v2]).unwrap();
        store.upsert_staged_chunks(&staged_v2).unwrap();

        // count_chunks shows only ONE row (idempotency on PK), and the SQL row
        // holds the new values. Note: disk content_path is write-if-new
        // (PR6+ concern), so we test the SQL UPSERT semantics here.
        assert_eq!(store.count_chunks().unwrap(), 1);
        let got = store.get_chunk("chunk_00").unwrap().unwrap();
        assert_eq!(got.content, "REPLACED CONTENT");
        assert_eq!(got.token_count, 99);
    }

    #[test]
    fn upsert_get_preserves_time_range_and_source_ref() {
        let (store, dir) = fresh_store();

        let ts_start = Utc.timestamp_millis_opt(1_700_000_000_000).unwrap();
        let ts_end = Utc.timestamp_millis_opt(1_700_000_005_000).unwrap();
        let mut chunk = sample_chunk(0);
        chunk.metadata.time_range = (ts_start, ts_end);
        chunk.metadata.source_ref = Some(SourceRef::new("provider:abc/xyz"));

        let staged = stage_chunks(dir.path(), &[chunk]).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();

        let got = store.get_chunk("chunk_00").unwrap().unwrap();
        assert_eq!(
            got.metadata.time_range.0.timestamp_millis(),
            ts_start.timestamp_millis()
        );
        assert_eq!(
            got.metadata.time_range.1.timestamp_millis(),
            ts_end.timestamp_millis()
        );
        let sref = got.metadata.source_ref.expect("source_ref should round-trip");
        assert_eq!(sref.value, "provider:abc/xyz");
    }

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
}
