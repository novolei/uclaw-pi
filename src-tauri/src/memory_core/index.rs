//! Incremental sync of the derived search index (docs + FTS + vectors) in uclaw.db
//! from the memory graph and the bucket_seal chunk store.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use super::embed::{pack, Embedder};
use super::segment::segmented_text;

const MAX_CONTENT_CHARS: usize = 2000;
const EMBED_BATCH: usize = 16;

#[derive(Debug, Clone)]
pub struct SourceDoc {
    pub doc_id: String,
    pub source: &'static str,
    pub source_id: String,
    pub title: String,
    pub content: String,
    pub updated_at: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncStats {
    pub upserted: usize,
    pub removed: usize,
    pub embedded: usize,
}

fn truncate_chars(s: String) -> String {
    if s.chars().count() <= MAX_CONTENT_CHARS {
        s
    } else {
        s.chars().take(MAX_CONTENT_CHARS).collect()
    }
}

/// One doc per graph node (default space), using its latest active version.
pub fn graph_docs(conn: &Connection) -> rusqlite::Result<Vec<SourceDoc>> {
    let mut stmt = conn.prepare(
        "SELECT n.id, n.title, v.content, COALESCE(v.updated_at, v.created_at)
         FROM memory_nodes n
         JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
         WHERE n.space_id = 'default'
         ORDER BY n.id, v.created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for row in rows {
        let (id, title, content, updated) = row?;
        if !seen.insert(id.clone()) {
            continue;
        }
        out.push(SourceDoc {
            doc_id: format!("graph:{id}"),
            source: "graph",
            source_id: id,
            title: title.unwrap_or_default(),
            content: truncate_chars(content.unwrap_or_default()),
            updated_at: updated,
        });
    }
    Ok(out)
}

/// One doc per bucket_seal chunk (pass the chunks.db connection).
pub fn chunk_docs(chunks: &Connection) -> rusqlite::Result<Vec<SourceDoc>> {
    let mut stmt = chunks.prepare(
        "SELECT id, source_ref, content, created_at_ms FROM mem_tree_chunks ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, source_ref, content, ms) = row?;
        out.push(SourceDoc {
            doc_id: format!("chunk:{id}"),
            source: "chunk",
            source_id: id,
            title: source_ref.unwrap_or_default(),
            content: truncate_chars(content),
            updated_at: Some(ms.to_string()),
        });
    }
    Ok(out)
}

fn content_hash(title: &str, content: &str) -> String {
    let mut h = Sha256::new();
    h.update(title.as_bytes());
    h.update(b"\n");
    h.update(content.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Bring the index in line with `docs`.
///
/// `docs` is treated as the COMPLETE corpus: every indexed doc (docs row, FTS row,
/// vector) whose doc_id is not in `docs` is deleted, so callers must pass all
/// sources together. Docs whose content hash changed are rewritten (FTS refreshed,
/// stale vector dropped). If `embedder` is given, every doc lacking a vector (or
/// with one from a different model) is embedded in batches of 16, one transaction
/// per batch.
pub fn sync(
    conn: &Connection,
    docs: &[SourceDoc],
    embedder: Option<&Embedder>,
) -> anyhow::Result<SyncStats> {
    let mut stats = SyncStats::default();

    let existing: HashMap<String, String> = {
        let mut stmt = conn.prepare("SELECT doc_id, content_hash FROM memory_search_docs")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let wanted: HashSet<&str> = docs.iter().map(|d| d.doc_id.as_str()).collect();

    let tx = conn.unchecked_transaction()?;
    for id in existing.keys().filter(|id| !wanted.contains(id.as_str())) {
        tx.execute("DELETE FROM memory_search_docs WHERE doc_id = ?1", [id])?;
        tx.execute("DELETE FROM memory_search_fts WHERE doc_id = ?1", [id])?;
        tx.execute("DELETE FROM memory_search_vectors WHERE doc_id = ?1", [id])?;
        stats.removed += 1;
    }
    for d in docs {
        let hash = content_hash(&d.title, &d.content);
        if existing.get(&d.doc_id) == Some(&hash) {
            continue;
        }
        let seg = segmented_text(&format!("{}\n{}", d.title, d.content));
        tx.execute(
            "INSERT INTO memory_search_docs
               (doc_id, source, source_id, title, content, content_hash, source_updated_at, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))
             ON CONFLICT(doc_id) DO UPDATE SET
               source = excluded.source, source_id = excluded.source_id,
               title = excluded.title, content = excluded.content,
               content_hash = excluded.content_hash,
               source_updated_at = excluded.source_updated_at,
               indexed_at = excluded.indexed_at",
            params![d.doc_id, d.source, d.source_id, d.title, d.content, hash, d.updated_at],
        )?;
        tx.execute("DELETE FROM memory_search_fts WHERE doc_id = ?1", [&d.doc_id])?;
        tx.execute(
            "INSERT INTO memory_search_fts (doc_id, segmented) VALUES (?1, ?2)",
            params![d.doc_id, seg],
        )?;
        tx.execute("DELETE FROM memory_search_vectors WHERE doc_id = ?1", [&d.doc_id])?;
        stats.upserted += 1;
    }
    tx.commit()?;

    if let Some(emb) = embedder {
        let pending: Vec<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT d.doc_id, d.title || char(10) || d.content
                 FROM memory_search_docs d
                 LEFT JOIN memory_search_vectors v ON v.doc_id = d.doc_id
                 WHERE v.doc_id IS NULL OR v.model_id != ?1
                 ORDER BY d.doc_id",
            )?;
            let rows = stmt.query_map([emb.model_id()], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for batch in pending.chunks(EMBED_BATCH) {
            let texts: Vec<String> = batch.iter().map(|(_, t)| t.clone()).collect();
            let vecs = emb.embed(&texts)?;
            let tx = conn.unchecked_transaction()?;
            for ((doc_id, _), v) in batch.iter().zip(vecs.iter()) {
                if v.len() != emb.dim() {
                    tracing::warn!("embedding dim mismatch for {doc_id}: {} != {}", v.len(), emb.dim());
                    continue;
                }
                tx.execute(
                    "INSERT OR REPLACE INTO memory_search_vectors (doc_id, model_id, dim, embedding)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![doc_id, emb.model_id(), emb.dim() as i64, pack(v)],
                )?;
                stats.embedded += 1;
            }
            tx.commit()?;
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations_up_to(&conn, 61).unwrap();
        conn
    }

    fn doc(id: &str, title: &str, content: &str) -> SourceDoc {
        SourceDoc {
            doc_id: format!("graph:{id}"),
            source: "graph",
            source_id: id.into(),
            title: title.into(),
            content: content.into(),
            updated_at: None,
        }
    }

    fn fts_count(conn: &Connection, id: &str) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM memory_search_fts WHERE doc_id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn incremental_sync_upserts_and_removes() {
        let conn = db();
        let mut docs = vec![doc("a", "家庭", "用户的儿子叫刘书维"), doc("b", "工作", "在做记忆检索")];
        assert_eq!(sync(&conn, &docs, None).unwrap().upserted, 2);
        assert_eq!(sync(&conn, &docs, None).unwrap().upserted, 0);

        docs[1].content = "在做混合检索".into();
        assert_eq!(sync(&conn, &docs, None).unwrap().upserted, 1);

        let hit: String = conn
            .query_row(
                "SELECT doc_id FROM memory_search_fts WHERE memory_search_fts MATCH '\"儿子\"'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hit, "graph:a");

        docs.pop();
        let s = sync(&conn, &docs, None).unwrap();
        assert_eq!(s.removed, 1);
        assert_eq!(fts_count(&conn, "graph:b"), 0);
        assert_eq!(fts_count(&conn, "graph:a"), 1);
    }

    #[test]
    fn graph_docs_picks_latest_active_version() {
        let conn = db();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title, metadata_json, created_at, updated_at)
             VALUES ('n1','default','fact','T','{}','2026-01-01','2026-01-01')",
            [],
        )
        .unwrap();
        for (vid, st, c, ts) in [("v1", "active", "old", "2026-01-01"), ("v2", "active", "new", "2026-02-01"), ("v3", "superseded", "x", "2026-03-01")] {
            conn.execute(
                "INSERT INTO memory_versions (id, node_id, status, content, created_at, updated_at)
                 VALUES (?1,'n1',?2,?3,?4,?4)",
                params![vid, st, c, ts],
            )
            .unwrap();
        }
        let docs = graph_docs(&conn).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].doc_id, "graph:n1");
        assert_eq!(docs[0].content, "new");
    }

    #[test]
    fn chunk_docs_reads_chunks_and_truncates() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE mem_tree_chunks (id TEXT PRIMARY KEY, source_kind TEXT, source_id TEXT,
               source_ref TEXT, content TEXT, created_at_ms INTEGER);",
        )
        .unwrap();
        let long = "字".repeat(3000);
        c.execute("INSERT INTO mem_tree_chunks VALUES ('c1','k','s',NULL,?1,5)", [&long]).unwrap();
        let docs = chunk_docs(&c).unwrap();
        assert_eq!(docs[0].doc_id, "chunk:c1");
        assert_eq!(docs[0].title, "");
        assert_eq!(docs[0].content.chars().count(), 2000);
    }

    #[test]
    #[ignore]
    fn sync_embeds_with_real_model() {
        let Ok(root) = std::env::var("UCLAW_TEST_MODELS") else {
            eprintln!("UCLAW_TEST_MODELS unset; skipping");
            return;
        };
        let emb = Embedder::load(std::path::Path::new(&root)).unwrap();
        let conn = db();
        let docs = vec![doc("a", "家庭", "用户的儿子叫刘书维"), doc("b", "工作", "在做记忆检索"), doc("c", "", "hello world")];
        let s = sync(&conn, &docs, Some(&emb)).unwrap();
        assert_eq!(s.embedded, 3);
        let (n, dim): (i64, i64) = conn
            .query_row("SELECT COUNT(*), MIN(dim) FROM memory_search_vectors", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((n, dim), (3, 1024));
        assert_eq!(sync(&conn, &docs, Some(&emb)).unwrap().embedded, 0);
    }
}
