//! Background service for `MemoryCore`: keeps the hybrid index in sync with the
//! memory graph + bucket_seal chunks, fetches the embedding / rerank models on
//! first run, and embeds pending docs once the embedder is loaded.
//!
//! Locking rule (Ruling 8): the app DB mutex is never held while embedding.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use super::index::{self, SyncStats};
use super::models::{self, BGE_M3_INT8, BGE_RERANKER_V2_M3_INT8};
use super::segment::segment;
use super::MemoryCore;

const SYNC_INTERVAL: Duration = Duration::from_secs(60);
static CHUNKS_UNREADABLE_WARNED: AtomicBool = AtomicBool::new(false);
const MODEL_RETRY: Duration = Duration::from_secs(30 * 60);

/// One BM25-only sync pass over graph + chunk docs.
///
/// Returns `Ok(None)` (cycle skipped) when the chunks DB cannot be read: `sync`
/// treats its input as the complete corpus, so a partial corpus would delete
/// every chunk doc.
pub fn run_sync_cycle(core: &MemoryCore, chunks_db_path: &Path) -> anyhow::Result<Option<SyncStats>> {
    let chunk_docs = match Connection::open_with_flags(chunks_db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .and_then(|c| index::chunk_docs(&c))
    {
        Ok(d) => d,
        Err(e) => {
            // Warn on the first failure only; debug while it stays broken.
            if CHUNKS_UNREADABLE_WARNED.swap(true, Ordering::Relaxed) {
                tracing::debug!("memory_core: chunks db unreadable, skipping sync cycle: {e}");
            } else {
                tracing::warn!("memory_core: chunks db unreadable, skipping sync cycle: {e}");
            }
            return Ok(None);
        }
    };
    CHUNKS_UNREADABLE_WARNED.store(false, Ordering::Relaxed);
    let conn = core.db().lock().map_err(|_| anyhow::anyhow!("memory db lock poisoned"))?;
    let mut docs = index::graph_docs(&conn)?;
    docs.extend(chunk_docs);
    let stats = index::sync(&conn, &docs, None)?;
    Ok(Some(stats))
}

/// Embed whatever lacks a vector (locks the DB per batch only).
fn embed_cycle(core: &MemoryCore) -> usize {
    let Some(emb) = core.embedder() else { return 0 };
    match index::embed_pending(core.db(), &emb) {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!("memory_core: embed_pending failed: {e:#}");
            0
        }
    }
}

async fn cycle(core: &Arc<MemoryCore>, chunks_db_path: &Arc<PathBuf>) {
    let c = core.clone();
    let p = chunks_db_path.clone();
    let res = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<(SyncStats, usize)>> {
        let Some(stats) = run_sync_cycle(&c, &p)? else { return Ok(None) };
        let embedded = embed_cycle(&c);
        Ok(Some((stats, embedded)))
    })
    .await;
    match res {
        Ok(Ok(Some((s, embedded)))) => {
            if s.upserted + s.removed + embedded > 0 {
                tracing::info!(
                    "memory_core: sync upserted={} removed={} embedded={}",
                    s.upserted, s.removed, embedded
                );
            } else {
                tracing::debug!("memory_core: sync no changes");
            }
        }
        Ok(Ok(None)) => {}
        Ok(Err(e)) => tracing::warn!("memory_core: sync cycle failed: {e:#}"),
        Err(e) => tracing::warn!("memory_core: sync task join error: {e}"),
    }
}

async fn ensure_models(core: &Arc<MemoryCore>, client: &reqwest::Client) -> bool {
    let root = core.models_root().to_path_buf();
    for spec in [&BGE_M3_INT8, &BGE_RERANKER_V2_M3_INT8] {
        if let Err(e) = models::ensure_model(client, &root, spec).await {
            tracing::warn!("memory_core: model {} unavailable: {e:#}", spec.id);
            return false;
        }
    }
    true
}

/// Download (retrying every 30 min on failure) and then load the models.
/// Runs independently of the sync loop so BM25 indexing never waits on it.
async fn model_task(core: Arc<MemoryCore>) {
    let client = match reqwest::Client::builder().connect_timeout(Duration::from_secs(15)).build() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("memory_core: http client build failed, no model download: {e}");
            return;
        }
    };
    while !ensure_models(&core, &client).await {
        tokio::time::sleep(MODEL_RETRY).await;
    }
    load_models(&core).await;
}

async fn load_models(core: &Arc<MemoryCore>) {
    let c = core.clone();
    match tokio::task::spawn_blocking(move || c.try_load_models()).await {
        Ok(true) => tracing::info!("memory_core: models loaded"),
        Ok(false) => tracing::warn!("memory_core: model load did not produce an embedder"),
        Err(e) => tracing::warn!("memory_core: model load task failed: {e}"),
    }
}

/// Read the `memory_core_models_enabled` setting under a short DB lock
/// (default on; a poisoned lock also falls back to on).
fn models_enabled(core: &MemoryCore) -> bool {
    use crate::services::settings_service::{DbSettings, SettingsService as _};
    core.db().lock().map(|c| DbSettings.memory_core_models_enabled(&c)).unwrap_or(true)
}

/// Spawn the background tasks (called from a Tauri/Tokio async context).
/// Model download/load runs in its own task; the sync loop never waits on it
/// and embeds as soon as `core.embedder()` becomes available.
///
/// The `memory_core_models_enabled` setting is read once here: when off, no model
/// is downloaded or loaded and the index stays BM25-only.
pub fn spawn(core: Arc<MemoryCore>, chunks_db_path: PathBuf) {
    let models_on = models_enabled(&core);
    if models_on {
        tauri::async_runtime::spawn(model_task(core.clone()));
    } else {
        tracing::info!("MemoryCore models disabled by setting");
    }
    tauri::async_runtime::spawn(async move {
        let chunks_db_path = Arc::new(chunks_db_path);
        // Warm up jieba once so the first sync doesn't pay for dictionary load mid-lock.
        let _ = tokio::task::spawn_blocking(|| segment("预热")).await;

        let mut last_load_retry = tokio::time::Instant::now();
        loop {
            cycle(&core, &chunks_db_path).await;
            tokio::time::sleep(SYNC_INTERVAL).await;
            // Models on disk but embedder missing (a load failed): retry at most every 30 min.
            if models_on
                && core.embedder().is_none()
                && models::is_ready(core.models_root(), &BGE_M3_INT8)
                && last_load_retry.elapsed() >= MODEL_RETRY
            {
                last_load_retry = tokio::time::Instant::now();
                load_models(&core).await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_core::MemoryCoreConfig;
    use std::sync::Mutex;

    fn core() -> MemoryCore {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations_up_to(&conn, 61).unwrap();
        MemoryCore::new(Arc::new(Mutex::new(conn)), PathBuf::from("/nonexistent"), MemoryCoreConfig::default())
    }

    fn make_chunks_db(path: &Path, rows: &[(&str, &str)]) {
        let c = Connection::open(path).unwrap();
        c.execute_batch(
            "CREATE TABLE mem_tree_chunks (
                id TEXT PRIMARY KEY, source_kind TEXT NOT NULL, source_id TEXT NOT NULL,
                source_ref TEXT, owner TEXT NOT NULL, timestamp_ms INTEGER NOT NULL,
                time_range_start_ms INTEGER NOT NULL, time_range_end_ms INTEGER NOT NULL,
                tags_json TEXT NOT NULL DEFAULT '[]', content TEXT NOT NULL,
                token_count INTEGER NOT NULL, seq_in_source INTEGER NOT NULL,
                created_at_ms INTEGER NOT NULL, content_path TEXT NOT NULL DEFAULT '',
                content_sha256 TEXT NOT NULL DEFAULT '');",
        )
        .unwrap();
        for (id, content) in rows {
            c.execute(
                "INSERT INTO mem_tree_chunks (id, source_kind, source_id, source_ref, owner,
                 timestamp_ms, time_range_start_ms, time_range_end_ms, content, token_count,
                 seq_in_source, created_at_ms) VALUES (?1,'chat','s','ref','o',1,1,1,?2,1,0,1)",
                rusqlite::params![id, content],
            )
            .unwrap();
        }
    }

    fn doc_count(core: &MemoryCore) -> i64 {
        core.db().lock().unwrap().query_row("SELECT COUNT(*) FROM memory_search_docs", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn models_enabled_reads_setting_default_on() {
        let core = core();
        assert!(models_enabled(&core), "default on");
        {
            let c = core.db().lock().unwrap();
            c.execute_batch("CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
                .unwrap();
            use crate::services::settings_service::{DbSettings, SettingsService as _};
            DbSettings.set_memory_core_models_enabled(&c, false).unwrap();
        }
        assert!(!models_enabled(&core));
    }

    #[test]
    fn cycle_indexes_chunks_and_skips_when_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let chunks = dir.path().join("chunks.db");
        make_chunks_db(&chunks, &[("a", "苹果 手机 发布会"), ("b", "hello world")]);
        let core = core();

        let stats = run_sync_cycle(&core, &chunks).unwrap().expect("cycle ran");
        assert_eq!(stats.upserted, 2);
        assert_eq!(doc_count(&core), 2);

        // Unreadable chunks DB: cycle skipped, existing docs untouched.
        let missing = dir.path().join("nope.db");
        assert!(run_sync_cycle(&core, &missing).unwrap().is_none());
        assert_eq!(doc_count(&core), 2);

        // Idempotent re-run.
        let again = run_sync_cycle(&core, &chunks).unwrap().unwrap();
        assert_eq!((again.upserted, again.removed), (0, 0));
    }
}
