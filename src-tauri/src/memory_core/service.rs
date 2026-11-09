//! Background service for `MemoryCore`: keeps the hybrid index in sync with the
//! memory graph + bucket_seal chunks, fetches the embedding / rerank models on
//! first run, and embeds pending docs once the embedder is loaded.
//!
//! Locking rule (Ruling 8): the app DB mutex is never held while embedding.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use super::index::{self, SyncStats};
use super::models::{self, BGE_M3_INT8, BGE_RERANKER_V2_M3_INT8};
use super::segment::segment;
use super::status::{
    DownloadPhase, DownloadStatus, MemoryCoreStatus, StatusNotifier, BGE_M3_ONNX_BYTES, TOTAL_DOWNLOAD_BYTES,
};
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
        // Counts query takes the DB mutex: keep it off the async worker.
        c.refresh_index_status(true);
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

fn download_status(
    phase: DownloadPhase,
    model: &str,
    file: &str,
    source: Option<String>,
    downloaded: u64,
    error: Option<String>,
) -> DownloadStatus {
    DownloadStatus {
        phase,
        model: model.to_string(),
        file: file.to_string(),
        source,
        downloaded_bytes: downloaded.min(TOTAL_DOWNLOAD_BYTES),
        total_bytes: TOTAL_DOWNLOAD_BYTES,
        error,
    }
}

/// Status write for the download path: a no-op once `keep_going` is false (toggle
/// off), so a download still streaming never resurrects `download` after "off".
fn guarded_update(
    core: &MemoryCore,
    keep_going: &(dyn Fn() -> bool + Send + Sync),
    progress: bool,
    f: impl FnOnce(&mut MemoryCoreStatus),
) {
    if !keep_going() {
        return;
    }
    if progress {
        core.update_status_progress(f);
    } else {
        core.update_status(f);
    }
}

/// Download both model specs. `keep_going` is polled between specs and per
/// streamed chunk: once it returns false the download in flight is cancelled
/// (its `.part` deleted), the remaining specs are skipped, and `false` is
/// returned without a Failed status (and no further status writes).
async fn ensure_models(
    core: &Arc<MemoryCore>,
    client: &reqwest::Client,
    keep_going: &(dyn Fn() -> bool + Send + Sync),
) -> bool {
    let root = core.models_root().to_path_buf();
    // Bytes of onnx files already accounted for (done specs); small files are ignored.
    let mut base = 0u64;
    for (spec, onnx_bytes) in [
        (&BGE_M3_INT8, BGE_M3_ONNX_BYTES),
        (&BGE_RERANKER_V2_M3_INT8, super::status::RERANKER_ONNX_BYTES),
    ] {
        if models::is_ready(&root, spec) {
            base += onnx_bytes;
            continue; // nothing to download: no download phase at all
        }
        if !keep_going() {
            return false;
        }
        let file0 = spec.files.first().map(|f| f.path).unwrap_or("");
        guarded_update(core, keep_going, false, |s| {
            s.download = Some(download_status(DownloadPhase::Queued, spec.id, file0, None, base, None));
        });
        let c = core.clone();
        let on_progress = move |t: models::ProgressTick| {
            // The onnx file is first in the manifest, so for other files it is complete.
            let downloaded = if t.file.ends_with(".onnx") { base + t.downloaded } else { base + onnx_bytes };
            let d = download_status(
                DownloadPhase::Downloading,
                spec.id,
                &t.file,
                Some(t.source_host),
                downloaded,
                None,
            );
            guarded_update(&c, keep_going, true, |s| s.download = Some(d));
        };
        if let Err(e) = models::ensure_model(client, &root, spec, &on_progress, keep_going).await {
            if models::is_cancelled(&e) || !keep_going() {
                tracing::info!("memory_core: model download cancelled ({})", spec.id);
                return false;
            }
            tracing::warn!("memory_core: model {} unavailable: {e:#}", spec.id);
            let d = download_status(DownloadPhase::Failed, spec.id, file0, None, base, Some(format!("{e:#}")));
            guarded_update(core, keep_going, false, |s| s.download = Some(d));
            return false;
        }
        base += onnx_bytes;
    }
    // Only reached with a download in flight or everything already on disk.
    guarded_update(core, keep_going, false, |s| {
        if let Some(d) = s.download.as_mut() {
            d.phase = DownloadPhase::Done;
            d.downloaded_bytes = d.total_bytes;
            d.error = None;
        }
    });
    true
}

/// Seam between the runtime-toggle state machine and the real world (network,
/// onnx loading), so the state machine is unit-testable.
pub trait ModelOps: Send + Sync + 'static {
    /// Both model specs are fully on disk.
    fn models_ready(&self) -> bool;
    /// Fetch the model files; `true` on success. `keep_going` turning false cancels
    /// the download (mid-stream too), suppresses further status writes and
    /// returns `false`.
    fn download<'a>(
        &'a self,
        keep_going: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;
    /// Blocking, idempotent: load whatever is on disk. True iff the embedder is loaded.
    fn load(&self) -> bool;
    /// Blocking: drop the loaded models.
    fn unload(&self);
}

/// Production `ModelOps` backed by `MemoryCore` + `models::ensure_model`.
pub struct RealModelOps {
    core: Arc<MemoryCore>,
}

impl RealModelOps {
    pub fn new(core: Arc<MemoryCore>) -> Self {
        Self { core }
    }
}

impl ModelOps for RealModelOps {
    fn models_ready(&self) -> bool {
        let root = self.core.models_root();
        models::is_ready(root, &BGE_M3_INT8) && models::is_ready(root, &BGE_RERANKER_V2_M3_INT8)
    }

    fn download<'a>(
        &'a self,
        keep_going: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let client = match reqwest::Client::builder().connect_timeout(Duration::from_secs(15)).build() {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("memory_core: http client build failed, no model download: {e}");
                    return false;
                }
            };
            ensure_models(&self.core, &client, &*keep_going).await
        })
    }

    fn load(&self) -> bool {
        let loaded = self.core.try_load_models();
        // Download (if any) is finished either way; clear it from the status.
        self.core.update_status(|s| s.download = None);
        if loaded {
            tracing::info!("memory_core: models loaded");
        } else {
            tracing::warn!("memory_core: model load did not produce an embedder");
        }
        loaded
    }

    fn unload(&self) {
        self.core.unload_models();
    }
}

#[derive(Default)]
struct Ctl {
    /// Desired state of the models toggle.
    desired: bool,
    /// A `model_task` is alive (single-flight guard).
    running: bool,
    /// The task is parked in its retry wait (only then is a wake-up useful).
    waiting: bool,
    /// Models are currently loaded by us (skips redundant loads; guarded by `apply`).
    loaded: bool,
}

/// Runtime control of the model download/load, stored in `AppState`.
///
/// Invariants: at most one `model_task` at a time; a load only happens while the
/// desired state is "on" (checked under `apply`, which also serialises unloads),
/// so an off→on→on burst leaves exactly one loaded model set and an "off" can
/// never be followed by a stale load.
pub struct ServiceHandle {
    core: Arc<MemoryCore>,
    ops: Arc<dyn ModelOps>,
    ctl: Mutex<Ctl>,
    /// Serialises load/unload application against the desired state.
    apply: Mutex<()>,
    /// Serialises whole toggles (persist + desired + status) so DB and runtime agree.
    toggle: tokio::sync::Mutex<()>,
    /// Wakes the retry wait of a failed download.
    wake: tokio::sync::Notify,
    retry: Duration,
}

impl ServiceHandle {
    pub fn new(core: Arc<MemoryCore>, ops: Arc<dyn ModelOps>) -> Arc<Self> {
        Self::with_retry(core, ops, MODEL_RETRY)
    }

    fn with_retry(core: Arc<MemoryCore>, ops: Arc<dyn ModelOps>, retry: Duration) -> Arc<Self> {
        Arc::new(Self {
            core,
            ops,
            ctl: Mutex::new(Ctl { desired: true, running: false, waiting: false, loaded: false }),
            apply: Mutex::new(()),
            toggle: tokio::sync::Mutex::new(()),
            wake: tokio::sync::Notify::new(),
            retry,
        })
    }

    pub fn core(&self) -> &Arc<MemoryCore> {
        &self.core
    }

    pub fn status(&self) -> MemoryCoreStatus {
        self.core.status()
    }

    pub fn desired(&self) -> bool {
        self.ctl.lock().unwrap_or_else(|p| p.into_inner()).desired
    }

    /// Persist the setting, then apply it: on → download (if needed) + load;
    /// off → stop retrying, unload, keyword mode.
    pub async fn set_models_enabled(self: &Arc<Self>, enabled: bool) -> anyhow::Result<MemoryCoreStatus> {
        let _t = self.toggle.lock().await;
        let core = self.core.clone();
        tokio::task::spawn_blocking(move || persist_models_enabled(&core, enabled))
            .await
            .map_err(|e| anyhow::anyhow!("persist task: {e}"))??;
        self.core.update_status(|s| s.models_enabled = enabled);
        self.apply_desired(enabled).await;
        Ok(self.status())
    }

    /// Retry whatever failed: a failed download (re-download, then load), or a
    /// failed model load with the files on disk (clears `load_error` and re-runs
    /// the load in the background). Otherwise returns the status as is.
    pub async fn retry_download(self: &Arc<Self>) -> MemoryCoreStatus {
        let st = self.status();
        let download_failed = st.download.as_ref().is_some_and(|d| d.phase == DownloadPhase::Failed);
        let load_failed = st.load_error.is_some() && !st.embedder_loaded;
        if load_failed && !download_failed && self.desired() {
            let me = self.clone();
            let ready = tokio::task::spawn_blocking(move || me.ops.models_ready()).await.unwrap_or(false);
            if ready {
                // Back to "loading" right away; the load reports its own outcome.
                self.core.update_status(|s| s.load_error = None);
                let me = self.clone();
                tauri::async_runtime::spawn(async move { me.load_now().await });
                return self.status();
            }
            // Files went missing: fall through to a fresh download (which loads after).
            self.core.update_status(|s| s.load_error = None);
        }
        if download_failed || (load_failed && self.desired()) {
            let (spawn_task, waiting) = {
                let mut c = self.ctl.lock().unwrap_or_else(|p| p.into_inner());
                if c.desired && !c.running {
                    c.running = true;
                    (true, false)
                } else {
                    (false, c.waiting)
                }
            };
            if spawn_task {
                tauri::async_runtime::spawn(self.clone().model_task());
            } else if waiting {
                self.wake.notify_one(); // cut the 30-min wait short
            }
        }
        self.status()
    }

    /// Load models now if wanted and on disk (used by the periodic load retry).
    pub async fn load_now(self: &Arc<Self>) {
        let me = self.clone();
        let _ = tokio::task::spawn_blocking(move || me.apply_load(true)).await;
    }

    async fn apply_desired(self: &Arc<Self>, enabled: bool) {
        let ready = self.ops.models_ready(); // stat calls: not under `ctl`
        let (spawn_task, was_waiting) = {
            let mut c = self.ctl.lock().unwrap_or_else(|p| p.into_inner());
            c.desired = enabled;
            let spawn_task = enabled && !c.running && !ready;
            if spawn_task {
                c.running = true;
            }
            (spawn_task, c.waiting)
        };
        if was_waiting {
            self.wake.notify_one(); // re-evaluate a pending retry wait
        }
        if spawn_task {
            tauri::async_runtime::spawn(self.clone().model_task());
        }
        let me = self.clone();
        let _ = tokio::task::spawn_blocking(move || {
            if enabled {
                if ready {
                    me.apply_load(false);
                }
            } else {
                me.apply_unload();
            }
        })
        .await;
    }

    /// Load iff still wanted; checked under `apply` so it can't race an unload.
    /// `force` re-runs a load even if we think models are loaded (periodic retry).
    fn apply_load(&self, force: bool) {
        let _g = self.apply.lock().unwrap_or_else(|p| p.into_inner());
        if !self.desired() {
            return;
        }
        let c = self.ctl.lock().unwrap_or_else(|p| p.into_inner());
        if c.loaded && !force {
            return;
        }
        drop(c);
        let loaded = self.ops.load();
        self.ctl.lock().unwrap_or_else(|p| p.into_inner()).loaded = loaded;
    }

    fn apply_unload(&self) {
        let _g = self.apply.lock().unwrap_or_else(|p| p.into_inner());
        if !self.desired() {
            self.ops.unload();
            self.ctl.lock().unwrap_or_else(|p| p.into_inner()).loaded = false;
            self.core.update_status(|s| s.download = None);
        }
    }

    /// If the toggle is off, mark the task finished (atomically with the check).
    fn finish_if_off(&self) -> bool {
        let mut c = self.ctl.lock().unwrap_or_else(|p| p.into_inner());
        if c.desired {
            false
        } else {
            c.running = false;
            true
        }
    }

    /// Download (retrying every `retry` on failure) and then load the models.
    /// Runs independently of the sync loop so BM25 indexing never waits on it.
    async fn model_task(self: Arc<Self>) {
        loop {
            if self.finish_if_off() {
                self.core.update_status(|s| s.download = None);
                return;
            }
            let me = self.clone();
            let keep: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || me.desired());
            let ok = self.ops.download(keep).await;
            if self.finish_if_off() {
                self.core.update_status(|s| s.download = None);
                return;
            }
            if ok {
                self.ctl.lock().unwrap_or_else(|p| p.into_inner()).running = false;
                self.load_now().await;
                return;
            }
            self.ctl.lock().unwrap_or_else(|p| p.into_inner()).waiting = true;
            tokio::select! {
                _ = tokio::time::sleep(self.retry) => {}
                _ = self.wake.notified() => {}
            }
            self.ctl.lock().unwrap_or_else(|p| p.into_inner()).waiting = false;
        }
    }
}

/// Persist `memory_core_models_enabled` under a short DB lock.
fn persist_models_enabled(core: &MemoryCore, enabled: bool) -> anyhow::Result<()> {
    use crate::services::settings_service::{DbSettings, SettingsService as _};
    let conn = core.db().lock().map_err(|_| anyhow::anyhow!("memory db lock poisoned"))?;
    DbSettings.set_memory_core_models_enabled(&conn, enabled).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Read the `memory_core_models_enabled` setting under a short DB lock
/// (default on; a poisoned lock also falls back to on).
fn models_enabled(core: &MemoryCore) -> bool {
    use crate::services::settings_service::{DbSettings, SettingsService as _};
    core.db().lock().map(|c| DbSettings.memory_core_models_enabled(&c)).unwrap_or(true)
}

/// Spawn the background tasks (called from a Tauri/Tokio async context).
/// Model download/load runs in its own task; the sync loop (including the first
/// cycle) never waits on it and embeds as soon as the embedder becomes available.
///
/// The `memory_core_models_enabled` setting is read once here to seed the handle;
/// afterwards it is driven at runtime through `ServiceHandle`.
pub fn spawn(handle: Arc<ServiceHandle>, chunks_db_path: PathBuf, notifier: Option<StatusNotifier>) {
    let core = handle.core().clone();
    core.set_notifier(notifier);
    tauri::async_runtime::spawn(async move {
        let models_on = {
            let c = core.clone();
            tokio::task::spawn_blocking(move || {
                let on = models_enabled(&c);
                c.update_status(|s| s.models_enabled = on);
                c.refresh_index_status(false);
                on
            })
            .await
            .unwrap_or(true)
        };
        if !models_on {
            tracing::info!("MemoryCore models disabled by setting");
        }
        // Not awaited: the first BM25 sync must not wait for a model load.
        {
            let h = handle.clone();
            tauri::async_runtime::spawn(async move {
                let _t = h.toggle.lock().await;
                h.apply_desired(models_on).await;
            });
        }
        let chunks_db_path = Arc::new(chunks_db_path);
        // Warm up jieba once so the first sync doesn't pay for dictionary load mid-lock.
        let _ = tokio::task::spawn_blocking(|| segment("预热")).await;

        let mut last_load_retry = tokio::time::Instant::now();
        loop {
            cycle(&core, &chunks_db_path).await;
            tokio::time::sleep(SYNC_INTERVAL).await;
            // Models on disk but embedder missing (a load failed): retry at most every 30 min.
            if handle.desired()
                && core.embedder().is_none()
                && models::is_ready(core.models_root(), &BGE_M3_INT8)
                && last_load_retry.elapsed() >= MODEL_RETRY
            {
                last_load_retry = tokio::time::Instant::now();
                handle.load_now().await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_core::MemoryCoreConfig;
    use std::collections::VecDeque;
    use std::sync::atomic::AtomicUsize;

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

    // ── ServiceHandle state machine (fake ops: no network, no models) ──────────

    struct FakeOps {
        ready: AtomicBool,
        loaded: AtomicBool,
        loads: AtomicUsize,
        unloads: AtomicUsize,
        downloads: AtomicUsize,
        results: Mutex<VecDeque<bool>>,
        gate: Option<Arc<tokio::sync::Semaphore>>,
        /// When set, the download ticks progress through `guarded_update` after the gate.
        core: Mutex<Option<Arc<MemoryCore>>>,
        keep_seen: Mutex<Option<bool>>,
        /// Next loads fail (recording `load_error` on `load_core`, like `try_load_models`).
        fail_load: AtomicBool,
        load_core: Mutex<Option<Arc<MemoryCore>>>,
    }

    impl FakeOps {
        fn new(ready: bool, gated: bool, results: &[bool]) -> Arc<Self> {
            Arc::new(Self {
                ready: AtomicBool::new(ready),
                loaded: AtomicBool::new(false),
                loads: AtomicUsize::new(0),
                unloads: AtomicUsize::new(0),
                downloads: AtomicUsize::new(0),
                results: Mutex::new(results.iter().copied().collect()),
                gate: gated.then(|| Arc::new(tokio::sync::Semaphore::new(0))),
                core: Mutex::new(None),
                keep_seen: Mutex::new(None),
                fail_load: AtomicBool::new(false),
                load_core: Mutex::new(None),
            })
        }
    }

    impl ModelOps for FakeOps {
        fn models_ready(&self) -> bool {
            self.ready.load(Ordering::SeqCst)
        }
        fn download<'a>(
            &'a self,
            keep: Arc<dyn Fn() -> bool + Send + Sync>,
        ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async move {
                self.downloads.fetch_add(1, Ordering::SeqCst);
                if let Some(g) = &self.gate {
                    g.acquire().await.unwrap().forget();
                }
                if let Some(c) = self.core.lock().unwrap().clone() {
                    *self.keep_seen.lock().unwrap() = Some(keep());
                    guarded_update(&c, &*keep, true, |s| {
                        s.download = Some(download_status(DownloadPhase::Downloading, "m", "f", None, 5, None))
                    });
                    guarded_update(&c, &*keep, false, |s| {
                        s.download = Some(download_status(DownloadPhase::Done, "m", "f", None, 9, None))
                    });
                }
                let ok = self.results.lock().unwrap().pop_front().unwrap_or(true);
                if ok {
                    self.ready.store(true, Ordering::SeqCst);
                }
                ok
            })
        }
        fn load(&self) -> bool {
            self.loads.fetch_add(1, Ordering::SeqCst);
            let ok = !self.fail_load.load(Ordering::SeqCst);
            if let Some(c) = self.load_core.lock().unwrap().clone() {
                c.update_status(|s| {
                    s.load_error = (!ok).then(|| "load bge-m3: bad onnx".to_string());
                    s.embedder_loaded = ok;
                });
            }
            self.loaded.store(ok, Ordering::SeqCst);
            ok
        }
        fn unload(&self) {
            self.loaded.store(false, Ordering::SeqCst);
            self.unloads.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn handle_with(ops: Arc<FakeOps>, retry: Duration) -> (Arc<ServiceHandle>, Arc<MemoryCore>) {
        let c = core();
        c.db().lock().unwrap()
            .execute_batch("CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .unwrap();
        let c = Arc::new(c);
        (ServiceHandle::with_retry(c.clone(), ops, retry), c)
    }

    async fn wait_until(what: &str, f: impl Fn() -> bool) {
        for _ in 0..300 {
            if f() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("timed out waiting for {what}");
    }

    #[tokio::test]
    async fn on_with_files_ready_loads_without_downloading() {
        let ops = FakeOps::new(true, false, &[]);
        let (h, _c) = handle_with(ops.clone(), Duration::from_secs(3600));
        let st = h.set_models_enabled(true).await.unwrap();
        assert!(st.models_enabled);
        assert_eq!(ops.loads.load(Ordering::SeqCst), 1);
        assert_eq!(ops.downloads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn off_unloads_persists_and_clears_download() {
        let ops = FakeOps::new(true, false, &[]);
        let (h, c) = handle_with(ops.clone(), Duration::from_secs(3600));
        h.set_models_enabled(true).await.unwrap();
        c.update_status(|s| {
            s.download = Some(download_status(DownloadPhase::Downloading, "m", "f", None, 1, None))
        });
        let st = h.set_models_enabled(false).await.unwrap();
        assert!(!st.models_enabled);
        assert!(st.download.is_none());
        assert!(!ops.loaded.load(Ordering::SeqCst));
        assert!(!models_enabled(&c), "setting persisted");
        assert!(!h.desired());
    }

    #[tokio::test]
    async fn rapid_off_on_on_yields_one_load() {
        let ops = FakeOps::new(true, false, &[]);
        let (h, _c) = handle_with(ops.clone(), Duration::from_secs(3600));
        let (a, b, c) = tokio::join!(
            h.set_models_enabled(false),
            h.set_models_enabled(true),
            h.set_models_enabled(true)
        );
        a.unwrap();
        b.unwrap();
        c.unwrap();
        assert_eq!(ops.loads.load(Ordering::SeqCst), 1);
        assert!(ops.loaded.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn on_is_single_flight_while_downloading_then_loads() {
        let ops = FakeOps::new(false, true, &[]);
        let (h, _c) = handle_with(ops.clone(), Duration::from_secs(3600));
        h.set_models_enabled(true).await.unwrap();
        h.set_models_enabled(true).await.unwrap();
        wait_until("download start", || ops.downloads.load(Ordering::SeqCst) >= 1).await;
        ops.gate.as_ref().unwrap().add_permits(1);
        wait_until("load", || ops.loaded.load(Ordering::SeqCst)).await;
        assert_eq!(ops.downloads.load(Ordering::SeqCst), 1);
        assert_eq!(ops.loads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn off_during_download_never_loads_afterwards() {
        let ops = FakeOps::new(false, true, &[]);
        let (h, c) = handle_with(ops.clone(), Duration::from_secs(3600));
        h.set_models_enabled(true).await.unwrap();
        wait_until("download start", || ops.downloads.load(Ordering::SeqCst) == 1).await;
        h.set_models_enabled(false).await.unwrap();
        ops.gate.as_ref().unwrap().add_permits(1); // download finishes after "off"
        wait_until("task exit", || !h.ctl.lock().unwrap().running).await;
        assert_eq!(ops.loads.load(Ordering::SeqCst), 0);
        assert!(!ops.loaded.load(Ordering::SeqCst));
        assert!(c.status().download.is_none());
    }

    #[tokio::test]
    async fn no_download_status_writes_after_off() {
        let ops = FakeOps::new(false, true, &[]);
        let (h, c) = handle_with(ops.clone(), Duration::from_secs(3600));
        *ops.core.lock().unwrap() = Some(c.clone());
        h.set_models_enabled(true).await.unwrap();
        wait_until("download start", || ops.downloads.load(Ordering::SeqCst) == 1).await;
        h.set_models_enabled(false).await.unwrap();
        ops.gate.as_ref().unwrap().add_permits(1); // ticks + Done arrive after "off"
        wait_until("task exit", || !h.ctl.lock().unwrap().running).await;
        assert_eq!(*ops.keep_seen.lock().unwrap(), Some(false));
        assert!(c.status().download.is_none());
    }

    #[tokio::test]
    async fn retry_download_only_acts_when_failed() {
        let ops = FakeOps::new(false, false, &[false, true]);
        let (h, c) = handle_with(ops.clone(), Duration::from_secs(3600));
        h.set_models_enabled(true).await.unwrap();
        wait_until("first (failing) download", || ops.downloads.load(Ordering::SeqCst) == 1).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Not in failed phase: unchanged, no new download.
        let before = h.status();
        let after = h.retry_download().await;
        assert_eq!(before, after);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(ops.downloads.load(Ordering::SeqCst), 1);

        // Failed phase: wakes the 1h retry wait immediately.
        c.update_status(|s| {
            s.download = Some(download_status(DownloadPhase::Failed, "m", "f", None, 0, Some("x".into())))
        });
        h.retry_download().await;
        wait_until("retry load", || ops.loaded.load(Ordering::SeqCst)).await;
        assert_eq!(ops.downloads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn unload_models_without_models_is_keyword_mode() {
        let c = core();
        c.refresh_model_status();
        c.unload_models();
        let st = c.status();
        assert!(!st.embedder_loaded && !st.reranker_loaded);
        assert_eq!(st.mode, crate::memory_core::status::RetrievalMode::Keyword);
    }

    #[test]
    fn poisoned_status_lock_is_recovered() {
        let c = Arc::new(core());
        let c2 = c.clone();
        let _ = std::thread::spawn(move || {
            let _g = c2.status_lock_for_test();
            panic!("poison");
        })
        .join();
        assert!(c.update_status(|s| s.index.docs = 7));
        assert_eq!(c.status().index.docs, 7);
    }
    #[tokio::test]
    async fn retry_reruns_a_failed_load_when_files_are_ready() {
        let ops = FakeOps::new(true, false, &[]);
        let (h, c) = handle_with(ops.clone(), Duration::from_secs(3600));
        *ops.load_core.lock().unwrap() = Some(c.clone());
        ops.fail_load.store(true, Ordering::SeqCst);
        let st = h.set_models_enabled(true).await.unwrap();
        assert_eq!(ops.loads.load(Ordering::SeqCst), 1);
        assert!(st.load_error.is_some(), "load failure is visible");

        ops.fail_load.store(false, Ordering::SeqCst);
        let st = h.retry_download().await;
        assert!(st.load_error.is_none(), "retry goes back to loading at once");
        wait_until("reload", || ops.loaded.load(Ordering::SeqCst)).await;
        assert_eq!(ops.loads.load(Ordering::SeqCst), 2);
        assert_eq!(ops.downloads.load(Ordering::SeqCst), 0, "no download for a load failure");
        assert!(c.status().load_error.is_none());
    }

    #[tokio::test]
    async fn retry_after_load_failure_redownloads_when_files_are_gone() {
        let ops = FakeOps::new(true, false, &[]);
        let (h, c) = handle_with(ops.clone(), Duration::from_secs(3600));
        *ops.load_core.lock().unwrap() = Some(c.clone());
        ops.fail_load.store(true, Ordering::SeqCst);
        h.set_models_enabled(true).await.unwrap();
        ops.fail_load.store(false, Ordering::SeqCst);
        ops.ready.store(false, Ordering::SeqCst);
        h.retry_download().await;
        wait_until("redownload + load", || ops.loaded.load(Ordering::SeqCst)).await;
        assert_eq!(ops.downloads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn try_load_models_records_and_clears_load_error() {
        use crate::memory_core::models::{model_dir, BGE_M3_INT8, BGE_RERANKER_V2_M3_INT8};
        let t = tempfile::tempdir().unwrap();
        // Both specs "on disk" (sizes pass `is_ready`), but the onnx files are
        // unreadable, so both loads fail fast without reading 500 MB.
        for spec in [&BGE_M3_INT8, &BGE_RERANKER_V2_M3_INT8] {
            for f in spec.files {
                let p = model_dir(t.path(), spec).join(f.path);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::File::create(&p).unwrap().set_len(f.min_bytes).unwrap();
            }
            let onnx = model_dir(t.path(), spec).join("onnx/model_int8.onnx");
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&onnx, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations_up_to(&conn, 61).unwrap();
        let core = MemoryCore::new(Arc::new(Mutex::new(conn)), t.path().to_path_buf(), MemoryCoreConfig::default());
        assert!(!core.try_load_models());
        let st = core.status();
        let err = st.load_error.expect("load_error set");
        assert!(err.contains("bge-m3"), "{err}");
        assert!(!st.embedder_loaded);
        // Unload clears it (back to plain keyword mode).
        core.unload_models();
        assert!(core.status().load_error.is_none());
    }

    #[test]
    fn try_load_models_without_failures_clears_a_stale_load_error() {
        let c = core();
        c.update_status(|s| s.load_error = Some("old".into()));
        assert!(!c.try_load_models()); // nothing on disk: nothing attempted, nothing failed
        let st = c.status();
        assert!(st.load_error.is_none());
    }
}
