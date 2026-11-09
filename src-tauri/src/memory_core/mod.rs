//! Hybrid memory retrieval core (P1): segmentation, FTS index, vector recall,
//! rank fusion and reranking.

pub mod segment;
pub mod models;
pub mod embed;
pub mod index;
pub mod search;
pub mod service;
pub mod status;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use rusqlite::Connection;

use embed::{Embedder, Reranker};
use status::{MemoryCoreStatus, RetrievalMode, StatusNotifier};

/// Minimum gap between progress-driven status notifications.
const PROGRESS_THROTTLE: std::time::Duration = std::time::Duration::from_millis(250);

/// Max chars of the query fed to the embedder and the reranker, so a long pasted
/// message neither dominates the cross-encoder input nor slows the query embed.
const MAX_MODEL_QUERY_CHARS: usize = 256;

/// First `max` chars of `s` (char-safe).
fn truncate_chars(s: &str, max: usize) -> &str {
    s.char_indices().nth(max).map_or(s, |(i, _)| &s[..i])
}

#[derive(Debug, Clone)]
pub struct MemoryCoreConfig {
    pub bm25_top: usize,
    pub vector_top: usize,
    pub rrf_k: f32,
    pub rerank_top_n: usize,
    pub min_rerank_score: f32,
    pub limit: usize,
}

impl Default for MemoryCoreConfig {
    fn default() -> Self {
        Self { bm25_top: 50, vector_top: 50, rrf_k: 60.0, rerank_top_n: 6, min_rerank_score: 0.05, limit: 6 }
    }
}

#[derive(Debug, Clone)]
/// `score` is a sigmoid probability in 0..1 when the reranker ran, otherwise the RRF score.
pub struct MemoryHit {
    pub doc_id: String,
    pub source: String,
    pub title: String,
    pub content: String,
    pub score: f32,
    /// True only when `score` came from the cross-encoder reranker (sigmoid);
    /// false for fusion-only / reranker-busy / rerank-error paths (RRF score).
    pub reranked: bool,
}

pub struct MemoryCore {
    db: Arc<Mutex<Connection>>,
    models_root: PathBuf,
    embedder: RwLock<Option<Arc<Embedder>>>,
    reranker: RwLock<Option<Arc<Reranker>>>,
    /// Held for the whole of `try_load_models` so concurrent callers never load the
    /// same model twice (each load briefly costs ~1.2 GB extra peak memory).
    load_lock: Mutex<()>,
    config: MemoryCoreConfig,
    status: RwLock<MemoryCoreStatus>,
    notifier: Mutex<Option<StatusNotifier>>,
    last_progress_notify: Mutex<Option<std::time::Instant>>,
}

impl MemoryCore {
    pub fn new(db: Arc<Mutex<Connection>>, models_root: PathBuf, config: MemoryCoreConfig) -> Self {
        Self {
            db,
            models_root,
            embedder: RwLock::new(None),
            reranker: RwLock::new(None),
            load_lock: Mutex::new(()),
            config,
            status: RwLock::new(MemoryCoreStatus::default()),
            notifier: Mutex::new(None),
            last_progress_notify: Mutex::new(None),
        }
    }

    pub fn db(&self) -> &Arc<Mutex<Connection>> {
        &self.db
    }

    pub fn models_root(&self) -> &std::path::Path {
        &self.models_root
    }

    pub fn config(&self) -> &MemoryCoreConfig {
        &self.config
    }

    /// Snapshot of the observable engine status.
    pub fn status(&self) -> MemoryCoreStatus {
        self.status.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    #[cfg(test)]
    pub(crate) fn status_lock_for_test(&self) -> std::sync::RwLockWriteGuard<'_, MemoryCoreStatus> {
        self.status.write().unwrap()
    }

    /// The notifier runs on whatever thread changed the status, possibly while
    /// `load_lock` is held, so it must be cheap and must NOT call `try_load_models`
    /// (or anything else that takes `load_lock`).
    pub fn set_notifier(&self, n: Option<StatusNotifier>) {
        if let Ok(mut g) = self.notifier.lock() {
            *g = n;
        }
    }

    /// Apply `f` to the status. If anything (other than `seq`) changed, bump `seq`,
    /// store, and invoke the notifier (outside the lock). Returns whether it changed.
    pub fn update_status(&self, f: impl FnOnce(&mut MemoryCoreStatus)) -> bool {
        self.apply_status(f, true)
    }

    fn apply_status(&self, f: impl FnOnce(&mut MemoryCoreStatus), notify: bool) -> bool {
        let new = {
            let mut g = self.status.write().unwrap_or_else(|p| p.into_inner());
            let mut next = g.clone();
            f(&mut next);
            next.seq = g.seq;
            if next == *g {
                return false;
            }
            next.seq += 1;
            *g = next.clone();
            next
        };
        if notify {
            let notifier = self.notifier.lock().ok().and_then(|g| g.clone());
            if let Some(n) = notifier {
                n(&new);
            }
        }
        true
    }

    /// Like `update_status` but for download progress ticks: the new state is
    /// ALWAYS stored (so `status()` never misses a final tick); only the notifier
    /// call is dropped when the previous progress notification was < 250 ms ago.
    pub fn update_status_progress(&self, f: impl FnOnce(&mut MemoryCoreStatus)) -> bool {
        let notify = {
            let mut last = self.last_progress_notify.lock().unwrap_or_else(|p| p.into_inner());
            let now = std::time::Instant::now();
            if last.is_some_and(|t| now.duration_since(t) < PROGRESS_THROTTLE) {
                false
            } else {
                *last = Some(now);
                true
            }
        };
        self.apply_status(f, notify)
    }

    /// Sync `embedderLoaded` / `rerankerLoaded` / `mode` with what is actually loaded.
    pub fn refresh_model_status(&self) {
        let e = self.embedder().is_some();
        let r = self.reranker.read().map(|g| g.is_some()).unwrap_or(false);
        self.update_status(|s| {
            s.embedder_loaded = e;
            s.reranker_loaded = r;
            s.mode = if e { RetrievalMode::Hybrid } else { RetrievalMode::Keyword };
        });
    }

    /// Recompute index counts (short DB lock). `synced` stamps `last_sync_at` now.
    pub fn refresh_index_status(&self, synced: bool) {
        let model_id = self.embedder().map(|e| e.model_id());
        let counts = match self.db.lock() {
            Ok(c) => status::index_counts(&c, model_id),
            Err(_) => return,
        };
        match counts {
            Ok(c) => {
                self.update_status(|s| {
                    let last = s.index.last_sync_at.take();
                    s.index = c;
                    s.index.last_sync_at =
                        if synced { Some(chrono::Utc::now().to_rfc3339()) } else { last };
                });
            }
            Err(e) => tracing::warn!("memory_core: index counts failed: {e}"),
        }
    }

    /// Drop the loaded embedder / reranker (back to keyword mode). Waits for any
    /// in-flight load, so a load can never land after this returns.
    pub fn unload_models(&self) {
        let _load = self.load_lock.lock().unwrap_or_else(|p| p.into_inner());
        if let Ok(mut g) = self.embedder.write() {
            *g = None;
        }
        if let Ok(mut g) = self.reranker.write() {
            *g = None;
        }
        self.update_status(|s| s.load_error = None);
        self.refresh_model_status();
        // Pending/vector counts depend on the loaded embedder's model id.
        self.refresh_index_status(false);
    }

    /// Both model specs are fully on disk (stat calls only).
    pub fn models_on_disk(&self) -> bool {
        models::is_ready(&self.models_root, &models::BGE_M3_INT8)
            && models::is_ready(&self.models_root, &models::BGE_RERANKER_V2_M3_INT8)
    }

    /// Re-stat the model files and store `models_on_disk`.
    pub fn refresh_models_on_disk(&self) -> bool {
        let on_disk = self.models_on_disk();
        self.update_status(|s| s.models_on_disk = on_disk);
        on_disk
    }

    /// Load whichever models are on disk and not yet loaded. Idempotent.
    /// Returns true iff the embedder is loaded afterwards.
    pub fn try_load_models(&self) -> bool {
        // Serialize loads: the second caller waits, then sees the models loaded.
        let _load = self.load_lock.lock().unwrap_or_else(|p| p.into_inner());
        let mut errors: Vec<String> = Vec::new();
        if self.embedder().is_none() && models::is_ready(&self.models_root, &models::BGE_M3_INT8) {
            match Embedder::load(&self.models_root) {
                Ok(e) => {
                    if let Ok(mut g) = self.embedder.write() {
                        *g = Some(Arc::new(e));
                    }
                }
                Err(e) => {
                    tracing::warn!("memory_core: embedder load failed: {e:#}");
                    errors.push(format!("{e:#}"));
                }
            }
        }
        let have_reranker = self.reranker.read().map(|g| g.is_some()).unwrap_or(false);
        if !have_reranker && models::is_ready(&self.models_root, &models::BGE_RERANKER_V2_M3_INT8) {
            match Reranker::load(&self.models_root) {
                Ok(r) => {
                    if let Ok(mut g) = self.reranker.write() {
                        *g = Some(Arc::new(r));
                    }
                }
                Err(e) => {
                    tracing::warn!("memory_core: reranker load failed: {e:#}");
                    errors.push(format!("{e:#}"));
                }
            }
        }
        // Set on any failure, cleared when this attempt had none.
        let load_error = (!errors.is_empty()).then(|| errors.join("\n"));
        let on_disk = self.models_on_disk();
        self.update_status(|s| {
            s.load_error = load_error;
            s.models_on_disk = on_disk;
        });
        self.refresh_model_status();
        // Pending/vector counts depend on the loaded embedder's model id.
        self.refresh_index_status(false);
        self.embedder().is_some()
    }

    pub fn embedder(&self) -> Option<Arc<Embedder>> {
        self.embedder.read().ok().and_then(|g| g.clone())
    }

    /// Hybrid search. Synchronous; callers use `spawn_blocking`. The DB lock is held
    /// only around SQL, never while embedding or reranking.
    ///
    /// If the query has no searchable tokens but an embedder is loaded, the search is
    /// vector-only (a stopword-only query can still carry semantic content).
    pub fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<MemoryHit>> {
        let cfg = &self.config;
        let embedder = self.embedder();
        let reranker = self.reranker.read().ok().and_then(|g| g.clone());
        let match_q = segment::fts_match_query(query);
        if match_q.is_none() && embedder.is_none() {
            return Ok(Vec::new());
        }
        let model_query = truncate_chars(query, MAX_MODEL_QUERY_CHARS);

        // BM25 (lock for SQL only).
        let mut bm25_ids: Vec<String> = Vec::new();
        if let Some(mq) = &match_q {
            let conn = self.db.lock().map_err(|_| anyhow::anyhow!("memory db lock poisoned"))?;
            let mut stmt = conn.prepare(
                "SELECT doc_id FROM memory_search_fts WHERE memory_search_fts MATCH ?1
                 ORDER BY bm25(memory_search_fts) LIMIT ?2",
            )?;
            let rows = stmt.query_map(rusqlite::params![mq, cfg.bm25_top as i64], |r| r.get::<_, String>(0))?;
            for r in rows {
                bm25_ids.push(r?);
            }
        }

        // Vector (embed without lock; load vectors under lock; cosine without lock).
        let mut vec_ids: Vec<String> = Vec::new();
        if let Some(emb) = &embedder {
            // Never queue behind a background backfill batch: if the embedder is busy,
            // skip the vector arm for this query (BM25 + rerank still run).
            let qv = match emb.try_embed(&[model_query.to_string()]) {
                Some(Ok(v)) => v.into_iter().next().unwrap_or_default(),
                Some(Err(e)) => {
                    tracing::warn!("memory_core: query embed failed, BM25 only: {e:#}");
                    Vec::new()
                }
                None => {
                    tracing::debug!("memory_core: embedder busy (backfill), skipping vector arm");
                    Vec::new()
                }
            };
            if !qv.is_empty() {
                let rows: Vec<(String, Vec<u8>)> = {
                    let conn = self.db.lock().map_err(|_| anyhow::anyhow!("memory db lock poisoned"))?;
                    let mut stmt = conn.prepare(
                        "SELECT doc_id, embedding FROM memory_search_vectors WHERE model_id = ?1",
                    )?;
                    let it = stmt.query_map([emb.model_id()], |r| Ok((r.get(0)?, r.get(1)?)))?;
                    it.collect::<rusqlite::Result<_>>()?
                };
                let want = emb.dim() * 4;
                let mut scored: Vec<(String, f32)> = rows
                    .into_iter()
                    .filter(|(_, b)| b.len() == want)
                    .map(|(id, b)| {
                        let s = embed::cosine(&qv, &embed::unpack(&b));
                        (id, s)
                    })
                    .collect();
                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                vec_ids = scored.into_iter().take(cfg.vector_top).map(|(id, _)| id).collect();
            }
        }

        let mut fused = search::rrf_fuse(&[bm25_ids, vec_ids], cfg.rrf_k);
        fused.truncate(cfg.rerank_top_n.max(limit));
        if fused.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch candidate docs (lock for SQL only).
        let mut hits: Vec<MemoryHit> = Vec::with_capacity(fused.len());
        {
            let conn = self.db.lock().map_err(|_| anyhow::anyhow!("memory db lock poisoned"))?;
            let mut stmt = conn.prepare(
                "SELECT source, title, content FROM memory_search_docs WHERE doc_id = ?1",
            )?;
            for (id, score) in &fused {
                let row = stmt
                    .query_row([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)));
                match row {
                    Ok((source, title, content)) => hits.push(MemoryHit {
                        doc_id: id.clone(),
                        source,
                        title,
                        content,
                        score: *score,
                        reranked: false,
                    }),
                    Err(rusqlite::Error::QueryReturnedNoRows) => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }

        if let Some(rr) = reranker {
            hits = self.apply_rerank(model_query, hits, |q, t| rr.try_rerank(q, t).transpose());
        }
        hits.truncate(limit);
        Ok(hits)
    }

    /// Rerank `hits`; when the reranker is busy (`Ok(None)`) or errors, keep the
    /// fused order (scores stay RRF).
    fn apply_rerank<F>(&self, query: &str, mut hits: Vec<MemoryHit>, rerank: F) -> Vec<MemoryHit>
    where
        F: FnOnce(&str, &[String]) -> anyhow::Result<Option<Vec<(usize, f32)>>>,
    {
        let cfg = &self.config;
        hits.truncate(cfg.rerank_top_n);
        let texts: Vec<String> = hits
            .iter()
            .map(|h| if h.title.is_empty() { h.content.clone() } else { format!("{}\n{}", h.title, h.content) })
            .collect();
        let ranked = match rerank(query, &texts) {
            Ok(Some(r)) => r,
            Ok(None) => {
                tracing::debug!("memory_core: reranker busy, keeping RRF order");
                return hits;
            }
            Err(e) => {
                tracing::warn!("memory_core: rerank failed, keeping RRF order: {e:#}");
                return hits;
            }
        };
        let candidates = ranked.len();
        let mut out = Vec::new();
        for (i, logit) in ranked {
            let p = search::sigmoid(logit);
            if p < cfg.min_rerank_score {
                continue;
            }
            if let Some(h) = hits.get(i) {
                let mut h = h.clone();
                h.score = p;
                h.reranked = true;
                out.push(h);
            }
        }
        if out.is_empty() && candidates > 0 {
            tracing::debug!(
                "memory_core: rerank threshold {} dropped all {candidates} candidates",
                cfg.min_rerank_score
            );
        }
        out
    }

    pub fn render_block(hits: &[MemoryHit]) -> Option<String> {
        if hits.is_empty() {
            return None;
        }
        let mut s = String::from("<relevant_memories>\n");
        for h in hits {
            let content: String = h
                .content
                .chars()
                .take(300)
                .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
                .collect();
            // Chunk docs carry `source_ref` as their title (an opaque reference, not a
            // human title), so only graph-style hits get a `[title]` prefix.
            if h.title.is_empty() || h.source == "chunk" {
                s.push_str(&format!("- {content}\n"));
            } else {
                s.push_str(&format!("- [{}] {content}\n", h.title));
            }
        }
        s.push_str("</relevant_memories>\n");
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core_with(docs: &[(&str, &str, &str)]) -> MemoryCore {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations_up_to(&conn, 61).unwrap();
        let sd: Vec<index::SourceDoc> = docs
            .iter()
            .map(|(id, t, c)| index::SourceDoc {
                doc_id: format!("graph:{id}"),
                source: "graph",
                source_id: id.to_string(),
                title: t.to_string(),
                content: c.to_string(),
                updated_at: None,
            })
            .collect();
        index::sync(&conn, &sd, None).unwrap();
        MemoryCore::new(Arc::new(Mutex::new(conn)), PathBuf::from("/nonexistent"), MemoryCoreConfig::default())
    }

    fn hit(title: &str, content: &str) -> MemoryHit {
        MemoryHit { doc_id: "x".into(), source: "graph".into(), title: title.into(), content: content.into(), score: 1.0, reranked: false }
    }

    #[test]
    fn bm25_only_search_ranks_son_doc_first() {
        let core = core_with(&[("a", "家庭", "用户的儿子叫刘书维"), ("b", "饮食", "用户喜欢吃鱼")]);
        let hits = core.search("我儿子叫什么名字", 5).unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].content.contains("刘书维"));
    }

    #[test]
    fn truncate_chars_is_char_safe() {
        assert_eq!(truncate_chars("abc", 5), "abc");
        assert_eq!(truncate_chars("记忆检索", 2), "记忆");
        assert_eq!(truncate_chars(&"字".repeat(300), MAX_MODEL_QUERY_CHARS).chars().count(), 256);
    }

    #[test]
    fn unsearchable_query_is_empty_ok() {
        let core = core_with(&[("a", "家庭", "用户的儿子叫刘书维")]);
        assert!(core.search("？？", 5).unwrap().is_empty());
    }

    #[test]
    fn render_block_formats_and_truncates() {
        assert!(MemoryCore::render_block(&[]).is_none());
        let long = "字".repeat(400);
        let chunk = MemoryHit { source: "chunk".into(), ..hit("ref-123", "from a chunk") };
        let b = MemoryCore::render_block(&[hit("T", "a\nb"), hit("", &long), chunk]).unwrap();
        let lines: Vec<&str> = b.lines().collect();
        assert_eq!(lines[0], "<relevant_memories>");
        assert_eq!(lines[1], "- [T] a b");
        assert_eq!(lines[2], format!("- {}", "字".repeat(300)));
        assert_eq!(lines[3], "- from a chunk", "chunk hits render without their source_ref title");
        assert_eq!(lines[4], "</relevant_memories>");
        assert!(b.ends_with("</relevant_memories>\n"));
    }

    #[test]
    fn try_load_models_without_models_is_false() {
        let core = core_with(&[]);
        assert!(!core.try_load_models());
        assert!(core.embedder().is_none());
        // Serialized: a held load lock blocks a concurrent loader until released.
        let core = Arc::new(core);
        let guard = core.load_lock.lock().unwrap();
        let c2 = core.clone();
        let t = std::thread::spawn(move || c2.try_load_models());
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!t.is_finished(), "second loader must wait for the load lock");
        drop(guard);
        assert!(!t.join().unwrap());
    }

    #[test]
    #[ignore]
    fn search_cross_lingual_real_models() {
        let Ok(root) = std::env::var("UCLAW_TEST_MODELS") else { return };
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations_up_to(&conn, 61).unwrap();
        let mk = |id: &str, c: &str| index::SourceDoc {
            doc_id: format!("graph:{id}"),
            source: "graph",
            source_id: id.into(),
            title: String::new(),
            content: c.into(),
            updated_at: None,
        };
        let docs = vec![
            mk("1", "The user enjoys playing table tennis."),
            mk("2", "用户的儿子叫刘书维"),
            mk("3", "The user ran the 'ls -a' command in a terminal."),
            mk("4", "用户喜欢喝绿茶"),
        ];
        let core = MemoryCore::new(Arc::new(Mutex::new(conn)), PathBuf::from(&root), MemoryCoreConfig::default());
        assert!(core.try_load_models());
        {
            let emb = core.embedder().unwrap();
            let conn = core.db.lock().unwrap();
            index::sync(&conn, &docs, Some(emb.as_ref())).unwrap();
        }
        for (q, want) in [("我喜欢什么运动", "table tennis"), ("What is my son's name?", "刘书维")] {
            let t = std::time::Instant::now();
            let hits = core.search(q, 3).unwrap();
            eprintln!("query {q:?} total {:?}", t.elapsed());
            for h in &hits {
                eprintln!("  {:.4} {}", h.score, h.content);
            }
            assert!(hits[0].content.contains(want), "{q}: {:?}", hits.first());
        }
    }

    #[test]
    fn rerank_error_keeps_rrf_order() {
        let core = core_with(&[]);
        let hits = vec![hit("", "first"), hit("", "second")];
        let out = core.apply_rerank("q", hits, |_, _| Err(anyhow::anyhow!("boom")));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].content, "first");
        assert_eq!(out[0].score, 1.0);
        assert!(out.iter().all(|h| !h.reranked));
    }

    #[test]
    fn rerank_busy_keeps_rrf_order() {
        let core = core_with(&[]);
        let hits = vec![hit("", "first"), hit("", "second")];
        let out = core.apply_rerank("q", hits, |_, _| Ok(None));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].content, "first");
        assert_eq!(out[0].score, 1.0);
        assert!(out.iter().all(|h| !h.reranked));
    }

    #[test]
    fn rerank_reorders_and_thresholds() {
        let core = core_with(&[]);
        let hits = vec![hit("", "a"), hit("", "b"), hit("", "c")];
        let out = core.apply_rerank("q", hits, |_, _| Ok(Some(vec![(2, 2.0), (0, -1.0), (1, -11.0)])));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].content, "c");
        assert!(out[0].score > 0.8 && out[0].score < 1.0);
        assert!(out.iter().all(|h| h.reranked));
        let none = core.apply_rerank("q", vec![hit("", "a")], |_, _| Ok(Some(vec![(0, -11.0)])));
        assert!(none.is_empty());
    }
}
