//! Observable state of the `MemoryCore` engine: retrieval mode, loaded models,
//! model download progress and index counts. Pure data + SQL counts; the
//! holder lives in `MemoryCore` (`status()` / `update_status()`).

use std::sync::Arc;

use rusqlite::Connection;
use serde::Serialize;

/// Known int8 onnx sizes (verified 2026-11-09); small tokenizer/config files are
/// ignored in the overall percentage.
pub const BGE_M3_ONNX_BYTES: u64 = 568_456_694;
pub const RERANKER_ONNX_BYTES: u64 = 570_727_094;
pub const TOTAL_DOWNLOAD_BYTES: u64 = BGE_M3_ONNX_BYTES + RERANKER_ONNX_BYTES;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RetrievalMode {
    Hybrid,
    Keyword,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadPhase {
    Queued,
    Downloading,
    Failed,
    Done,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadStatus {
    pub phase: DownloadPhase,
    /// Model spec id.
    pub model: String,
    pub file: String,
    /// Source host currently used.
    pub source: Option<String>,
    pub downloaded_bytes: u64,
    /// Sum over both specs' onnx files.
    pub total_bytes: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub docs: u64,
    pub vectors: u64,
    pub pending_embeddings: u64,
    /// RFC3339.
    pub last_sync_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCoreStatus {
    /// Monotonically increasing per applied change; the UI drops older events.
    pub seq: u64,
    pub models_enabled: bool,
    pub mode: RetrievalMode,
    pub embedder_loaded: bool,
    pub reranker_loaded: bool,
    pub download: Option<DownloadStatus>,
    /// Last model load error (files on disk but onnx/tokenizer failed to load).
    /// Set by `try_load_models` on failure, cleared on success and on unload.
    pub load_error: Option<String>,
    /// Both model specs are fully on disk (refreshed at startup, on toggle and
    /// after a download), so the UI can tell "load" from "download" before any
    /// download status exists.
    pub models_on_disk: bool,
    pub index: IndexStatus,
}

impl Default for MemoryCoreStatus {
    fn default() -> Self {
        Self {
            seq: 0,
            models_enabled: true,
            mode: RetrievalMode::Keyword,
            embedder_loaded: false,
            reranker_loaded: false,
            download: None,
            load_error: None,
            models_on_disk: false,
            index: IndexStatus { docs: 0, vectors: 0, pending_embeddings: 0, last_sync_at: None },
        }
    }
}

pub type StatusNotifier = Arc<dyn Fn(&MemoryCoreStatus) + Send + Sync>;

/// Doc / vector / pending counts. `model_id` is the loaded embedder's id; with no
/// embedder, all vectors count and pending is the docs lacking any vector.
/// `last_sync_at` is left `None` for the caller to fill.
pub fn index_counts(conn: &Connection, model_id: Option<&str>) -> rusqlite::Result<IndexStatus> {
    let docs: i64 = conn.query_row("SELECT COUNT(*) FROM memory_search_docs", [], |r| r.get(0))?;
    let (vectors, pending): (i64, i64) = match model_id {
        Some(m) => (
            conn.query_row("SELECT COUNT(*) FROM memory_search_vectors WHERE model_id = ?1", [m], |r| r.get(0))?,
            conn.query_row(
                "SELECT COUNT(*) FROM memory_search_docs d
                 WHERE NOT EXISTS (SELECT 1 FROM memory_search_vectors v
                                   WHERE v.doc_id = d.doc_id AND v.model_id = ?1)",
                [m],
                |r| r.get(0),
            )?,
        ),
        None => (
            conn.query_row("SELECT COUNT(*) FROM memory_search_vectors", [], |r| r.get(0))?,
            conn.query_row(
                "SELECT COUNT(*) FROM memory_search_docs d
                 WHERE NOT EXISTS (SELECT 1 FROM memory_search_vectors v WHERE v.doc_id = d.doc_id)",
                [],
                |r| r.get(0),
            )?,
        ),
    };
    Ok(IndexStatus {
        docs: docs as u64,
        vectors: vectors as u64,
        pending_embeddings: pending as u64,
        last_sync_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_core::{index, MemoryCore, MemoryCoreConfig};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    fn core() -> MemoryCore {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations_up_to(&conn, 61).unwrap();
        MemoryCore::new(Arc::new(Mutex::new(conn)), PathBuf::from("/nonexistent"), MemoryCoreConfig::default())
    }

    #[test]
    fn serializes_camel_case_keyword_mode() {
        let v = serde_json::to_value(MemoryCoreStatus::default()).unwrap();
        assert_eq!(v["mode"], "keyword");
        assert_eq!(v["modelsEnabled"], true);
        assert_eq!(v["embedderLoaded"], false);
        assert_eq!(v["index"]["pendingEmbeddings"], 0);
        assert!(v["download"].is_null());
        assert!(v["loadError"].is_null());
        assert_eq!(v["modelsOnDisk"], false);
        let d = DownloadStatus {
            phase: DownloadPhase::Downloading,
            model: "m".into(),
            file: "f".into(),
            source: Some("hf-mirror.com".into()),
            downloaded_bytes: 1,
            total_bytes: 2,
            error: None,
        };
        let v = serde_json::to_value(d).unwrap();
        assert_eq!(v["phase"], "downloading");
        assert_eq!(v["downloadedBytes"], 1);
        assert_eq!(v["totalBytes"], 2);
    }

    #[test]
    fn update_status_notifies_once_per_change() {
        let core = core();
        let n = Arc::new(AtomicUsize::new(0));
        let n2 = n.clone();
        core.set_notifier(Some(Arc::new(move |_s: &MemoryCoreStatus| {
            n2.fetch_add(1, Ordering::SeqCst);
        })));
        assert_eq!(core.status().seq, 0);
        assert!(core.update_status(|s| s.models_enabled = false));
        assert_eq!(n.load(Ordering::SeqCst), 1);
        assert_eq!(core.status().seq, 1);
        // identical -> no notify, no seq bump
        assert!(!core.update_status(|s| s.models_enabled = false));
        assert_eq!(n.load(Ordering::SeqCst), 1);
        assert_eq!(core.status().seq, 1);
        assert!(core.update_status(|s| s.index.docs = 5));
        assert_eq!(n.load(Ordering::SeqCst), 2);
        assert_eq!(core.status().seq, 2);
    }

    #[test]
    fn progress_updates_are_throttled() {
        let core = core();
        let n = Arc::new(AtomicUsize::new(0));
        let n2 = n.clone();
        core.set_notifier(Some(Arc::new(move |_s: &MemoryCoreStatus| {
            n2.fetch_add(1, Ordering::SeqCst);
        })));
        for i in 1..=10u64 {
            core.update_status_progress(|s| s.index.docs = i);
        }
        // first notifies, the rest within 250 ms are stored silently
        assert_eq!(n.load(Ordering::SeqCst), 1);
        // ...but the state is always stored, so the final tick is never lost
        assert_eq!(core.status().index.docs, 10);
        // a non-progress change always goes through
        assert!(core.update_status(|s| s.index.docs = 99));
        assert_eq!(n.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn index_counts_reflect_docs_vectors_pending() {
        let core = core();
        let conn = core.db().lock().unwrap();
        let docs: Vec<index::SourceDoc> = ["a", "b", "c"]
            .iter()
            .map(|id| index::SourceDoc {
                doc_id: format!("graph:{id}"),
                source: "graph",
                source_id: id.to_string(),
                title: "t".into(),
                content: format!("content {id}"),
                updated_at: None,
            })
            .collect();
        index::sync(&conn, &docs, None).unwrap();
        let c = index_counts(&conn, None).unwrap();
        assert_eq!((c.docs, c.vectors, c.pending_embeddings), (3, 0, 3));
        conn.execute(
            "INSERT INTO memory_search_vectors (doc_id, model_id, dim, embedding) VALUES ('graph:a','m1',1,x'00000000')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memory_search_vectors (doc_id, model_id, dim, embedding) VALUES ('graph:b','old',1,x'00000000')",
            [],
        )
        .unwrap();
        let c = index_counts(&conn, Some("m1")).unwrap();
        assert_eq!((c.docs, c.vectors, c.pending_embeddings), (3, 1, 2));
        let c = index_counts(&conn, None).unwrap();
        assert_eq!((c.docs, c.vectors, c.pending_embeddings), (3, 2, 1));
    }
}
