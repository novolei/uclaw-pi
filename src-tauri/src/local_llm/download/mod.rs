// SPDX-License-Identifier: AGPL-3.0-or-later
//! Smart, network-aware downloader for the MiniCPM GGUF (S2).
//!
//! Pipeline (`download_model`):
//!   1. resolve source — explicit `prefer`, else concurrent latency probe.
//!   2. stream into `<dir>/<file>.part` with HTTP Range resume.
//!   3. on a transport error mid-stream, fail over to the *other* source
//!      (≤2 attempts total); the `.part` is preserved so the retry resumes.
//!   4. verify size + SHA256.
//!   5. atomic rename `.part` -> final path.
//! A back-compat [`download_from_modelscope`] shim keeps the S1 Tauri command
//! compiling.
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use crate::local_llm::paths::{model_dir, model_file_for, MODEL_FILE};

pub mod quant;
pub mod resume;
pub mod source;
pub mod verify;
pub use quant::Quant;
pub use source::Source;

/// How long the latency probe waits for a source to respond before treating it
/// as unreachable.
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

/// ModelScope raw-file URL for the Q4_K_M GGUF (back-compat for the S1 command).
pub fn modelscope_url() -> String {
    format!(
        "https://www.modelscope.cn/models/OpenBMB/MiniCPM5-1B-GGUF/resolve/master/{MODEL_FILE}"
    )
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("http error: {0}")]
    Http(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("download incomplete: got {got} bytes")]
    Incomplete { got: u64 },
    #[error("no download source reachable")]
    NoSource,
    #[error("checksum mismatch: expected {expected}, got {got}")]
    Checksum { expected: String, got: String },
}

impl From<resume::ResumeError> for DownloadError {
    fn from(e: resume::ResumeError) -> Self {
        match e {
            resume::ResumeError::Http(s) => DownloadError::Http(s),
            resume::ResumeError::Io(s) => DownloadError::Io(s),
        }
    }
}

impl From<verify::VerifyError> for DownloadError {
    fn from(e: verify::VerifyError) -> Self {
        match e {
            verify::VerifyError::Io(s) => DownloadError::Io(s),
            // A size mismatch means the .part was the wrong length -> Incomplete.
            verify::VerifyError::Size { got, .. } => DownloadError::Incomplete { got },
            verify::VerifyError::Checksum { expected, got } => {
                DownloadError::Checksum { expected, got }
            }
        }
    }
}

// --- Injectable seams (so orchestration is unit-testable without network) ---

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Resolve which source to use (probe or explicit `prefer`).
type ProbeFn<'a> = Box<dyn Fn() -> BoxFuture<'a, Result<Source, DownloadError>> + Send + 'a>;
/// Stream a source's file into `.part` (resume-aware). Returns bytes written.
type StreamFn<'a> =
    Box<dyn Fn(Source) -> BoxFuture<'a, Result<u64, DownloadError>> + Send + 'a>;
/// Verify the completed `.part`.
type VerifyFn<'a> = Box<dyn Fn() -> BoxFuture<'a, Result<(), DownloadError>> + Send + 'a>;

/// Core orchestration over injectable seams. Tries the resolved source, fails
/// over to the *other* source on a transport (`Http`) error (≤2 attempts), then
/// verifies and atomically renames `.part` -> `final_path`. On verify failure
/// the bad `.part` is removed.
#[allow(clippy::too_many_arguments)]
async fn download_with(
    prefer: Option<Source>,
    part_path: &Path,
    final_path: &Path,
    probe: ProbeFn<'_>,
    stream: StreamFn<'_>,
    verify_part: VerifyFn<'_>,
    rename: impl Fn(&Path, &Path) -> BoxFuture<'static, Result<(), DownloadError>>,
    remove_part: impl Fn(&Path) -> BoxFuture<'static, Result<(), DownloadError>>,
) -> Result<PathBuf, DownloadError> {
    // 1. resolve the first source.
    let first = match prefer {
        Some(s) => s,
        None => probe().await?,
    };
    let second = other_source(first);

    // 2-3. stream with failover to the other source (≤2 attempts).
    let mut last_err: Option<DownloadError> = None;
    for src in [first, second] {
        match stream(src).await {
            Ok(_) => {
                last_err = None;
                break;
            }
            // Only transport errors are retryable on the other source; io
            // errors (disk full, etc.) won't be fixed by switching mirror.
            Err(e @ DownloadError::Http(_)) => {
                last_err = Some(e);
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    if let Some(e) = last_err {
        return Err(e);
    }

    // 4. verify; on failure remove the bad .part.
    if let Err(e) = verify_part().await {
        let _ = remove_part(part_path).await;
        return Err(e);
    }

    // 5. atomic rename into place.
    rename(part_path, final_path).await?;
    Ok(final_path.to_path_buf())
}

fn other_source(s: Source) -> Source {
    match s {
        Source::ModelScope => Source::HuggingFace,
        Source::HuggingFace => Source::ModelScope,
    }
}

/// Download the GGUF for `quant`, picking the faster of HF/ModelScope (or the
/// explicit `prefer`), resuming a partial `.part`, failing over on transport
/// error, verifying size+SHA256, then atomically renaming into place.
/// `on_progress(downloaded, total)` is called as bytes arrive.
pub async fn download_model(
    data_dir: &Path,
    quant: Quant,
    prefer: Option<Source>,
    on_progress: impl Fn(u64, u64) + Send + Sync,
) -> Result<PathBuf, DownloadError> {
    let dir = model_dir(data_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| DownloadError::Io(e.to_string()))?;
    let final_path = model_file_for(data_dir, quant);
    let part_path = dir.join(format!("{}.part", quant.filename()));

    let on_progress = &on_progress;
    let part_for_stream = part_path.clone();
    let part_for_verify = part_path.clone();

    download_with(
        prefer,
        &part_path,
        &final_path,
        Box::new(move || {
            Box::pin(async move {
                source::probe_fastest(quant, PROBE_TIMEOUT)
                    .await
                    .map_err(|_| DownloadError::NoSource)
            })
        }),
        Box::new(move |src: Source| {
            let url = src.file_url(quant);
            let part = part_for_stream.clone();
            Box::pin(async move {
                resume::stream_with_resume(&url, &part, |d, t| on_progress(d, t))
                    .await
                    .map_err(DownloadError::from)
            })
        }),
        Box::new(move || {
            let part = part_for_verify.clone();
            Box::pin(async move { verify::verify(&part, quant).await.map_err(DownloadError::from) })
        }),
        |from, to| {
            let from = from.to_path_buf();
            let to = to.to_path_buf();
            Box::pin(async move {
                tokio::fs::rename(&from, &to)
                    .await
                    .map_err(|e| DownloadError::Io(e.to_string()))
            })
        },
        |p| {
            let p = p.to_path_buf();
            Box::pin(async move {
                let _ = tokio::fs::remove_file(&p).await;
                Ok(())
            })
        },
    )
    .await
}

/// Back-compat S1 shim: download the default-quant (Q4_K_M) GGUF, preferring
/// ModelScope (S1 behavior). Delegates to [`download_model`].
pub async fn download_from_modelscope(
    data_dir: &Path,
    on_progress: impl Fn(u64, u64) + Send + Sync,
) -> Result<PathBuf, DownloadError> {
    download_model(data_dir, Quant::default(), Some(Source::ModelScope), on_progress).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn ok_probe<'a>(s: Source) -> ProbeFn<'a> {
        Box::new(move || Box::pin(async move { Ok(s) }))
    }
    fn noop_rename(_: &Path, _: &Path) -> BoxFuture<'static, Result<(), DownloadError>> {
        Box::pin(async { Ok(()) })
    }
    fn noop_remove(_: &Path) -> BoxFuture<'static, Result<(), DownloadError>> {
        Box::pin(async { Ok(()) })
    }

    #[test]
    fn url_targets_modelscope_q4km() {
        let u = modelscope_url();
        assert!(u.starts_with("https://www.modelscope.cn/models/OpenBMB/MiniCPM5-1B-GGUF/"));
        assert!(u.ends_with(".gguf"));
    }

    #[test]
    fn other_source_flips() {
        assert_eq!(other_source(Source::ModelScope), Source::HuggingFace);
        assert_eq!(other_source(Source::HuggingFace), Source::ModelScope);
    }

    #[tokio::test]
    async fn happy_path_streams_verifies_renames() {
        let part = PathBuf::from("/tmp/x.part");
        let final_p = PathBuf::from("/tmp/x.gguf");
        let renamed = Arc::new(AtomicUsize::new(0));
        let r2 = renamed.clone();
        let out = download_with(
            Some(Source::ModelScope),
            &part,
            &final_p,
            ok_probe(Source::ModelScope),
            Box::new(|_s| Box::pin(async { Ok(100) })),
            Box::new(|| Box::pin(async { Ok(()) })),
            move |_f, _t| {
                r2.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(()) })
            },
            noop_remove,
        )
        .await
        .unwrap();
        assert_eq!(out, final_p);
        assert_eq!(renamed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn fails_over_to_other_source_on_http_error() {
        let part = PathBuf::from("/tmp/x.part");
        let final_p = PathBuf::from("/tmp/x.gguf");
        let attempts = Arc::new(AtomicUsize::new(0));
        let a2 = attempts.clone();
        let out = download_with(
            Some(Source::ModelScope),
            &part,
            &final_p,
            ok_probe(Source::ModelScope),
            Box::new(move |src: Source| {
                let a = a2.clone();
                Box::pin(async move {
                    a.fetch_add(1, Ordering::SeqCst);
                    // First source (ModelScope) fails transport; HF succeeds.
                    if src == Source::ModelScope {
                        Err(DownloadError::Http("connection reset".into()))
                    } else {
                        Ok(100)
                    }
                })
            }),
            Box::new(|| Box::pin(async { Ok(()) })),
            noop_rename,
            noop_remove,
        )
        .await
        .unwrap();
        assert_eq!(out, final_p);
        assert_eq!(attempts.load(Ordering::SeqCst), 2, "should try both sources");
    }

    #[tokio::test]
    async fn checksum_failure_removes_part_and_errors() {
        let part = PathBuf::from("/tmp/x.part");
        let final_p = PathBuf::from("/tmp/x.gguf");
        let removed = Arc::new(AtomicUsize::new(0));
        let renamed = Arc::new(AtomicUsize::new(0));
        let rm2 = removed.clone();
        let rn2 = renamed.clone();
        let err = download_with(
            Some(Source::HuggingFace),
            &part,
            &final_p,
            ok_probe(Source::HuggingFace),
            Box::new(|_s| Box::pin(async { Ok(100) })),
            Box::new(|| {
                Box::pin(async {
                    Err(DownloadError::Checksum {
                        expected: "aaa".into(),
                        got: "bbb".into(),
                    })
                })
            }),
            move |_f, _t| {
                rn2.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(()) })
            },
            move |_p| {
                rm2.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(()) })
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DownloadError::Checksum { .. }));
        assert_eq!(removed.load(Ordering::SeqCst), 1, "bad .part removed");
        assert_eq!(renamed.load(Ordering::SeqCst), 0, "no rename on checksum fail");
    }

    #[tokio::test]
    async fn both_sources_fail_returns_last_http_error() {
        let part = PathBuf::from("/tmp/x.part");
        let final_p = PathBuf::from("/tmp/x.gguf");
        let err = download_with(
            Some(Source::ModelScope),
            &part,
            &final_p,
            ok_probe(Source::ModelScope),
            Box::new(|_s| Box::pin(async { Err(DownloadError::Http("down".into())) })),
            Box::new(|| Box::pin(async { Ok(()) })),
            noop_rename,
            noop_remove,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DownloadError::Http(_)));
    }

    #[tokio::test]
    async fn io_error_does_not_fail_over() {
        let part = PathBuf::from("/tmp/x.part");
        let final_p = PathBuf::from("/tmp/x.gguf");
        let attempts = Arc::new(AtomicUsize::new(0));
        let a2 = attempts.clone();
        let err = download_with(
            Some(Source::ModelScope),
            &part,
            &final_p,
            ok_probe(Source::ModelScope),
            Box::new(move |_s| {
                let a = a2.clone();
                Box::pin(async move {
                    a.fetch_add(1, Ordering::SeqCst);
                    Err(DownloadError::Io("disk full".into()))
                })
            }),
            Box::new(|| Box::pin(async { Ok(()) })),
            noop_rename,
            noop_remove,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DownloadError::Io(_)));
        assert_eq!(attempts.load(Ordering::SeqCst), 1, "io error is not retried");
    }
}
