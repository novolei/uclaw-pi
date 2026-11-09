//! Int8 ONNX model manifest + mirrored downloader for memory retrieval.
//! Models land in `<models_root>/<spec.id>/…`; sources are tried in order
//! (huggingface.co, hf-mirror.com, modelscope.cn).

use std::path::{Path, PathBuf};

pub struct ModelFile {
    pub path: &'static str,
    pub min_bytes: u64,
}

pub struct ModelSpec {
    pub id: &'static str,
    pub hf_repo: &'static str,
    pub ms_repo: &'static str,
    pub files: &'static [ModelFile],
}

const FILES: &[ModelFile] = &[
    ModelFile { path: "onnx/model_int8.onnx", min_bytes: 500_000_000 },
    ModelFile { path: "tokenizer.json", min_bytes: 1_000_000 },
    ModelFile { path: "config.json", min_bytes: 1 },
    ModelFile { path: "special_tokens_map.json", min_bytes: 1 },
    ModelFile { path: "tokenizer_config.json", min_bytes: 1 },
];

pub const BGE_M3_INT8: ModelSpec = ModelSpec {
    id: "bge-m3-int8",
    hf_repo: "Xenova/bge-m3",
    ms_repo: "Xenova/bge-m3",
    files: FILES,
};

pub const BGE_RERANKER_V2_M3_INT8: ModelSpec = ModelSpec {
    id: "bge-reranker-v2-m3-int8",
    hf_repo: "onnx-community/bge-reranker-v2-m3-ONNX",
    ms_repo: "onnx-community/bge-reranker-v2-m3-ONNX",
    files: FILES,
};

pub fn model_dir(models_root: &Path, spec: &ModelSpec) -> PathBuf {
    models_root.join(spec.id)
}

/// True when every manifest file exists at its final path with `len >= min_bytes`.
/// `.part` files are never considered.
pub fn is_ready(models_root: &Path, spec: &ModelSpec) -> bool {
    let dir = model_dir(models_root, spec);
    spec.files.iter().all(|f| file_ready(&dir, f))
}

fn file_ready(dir: &Path, f: &ModelFile) -> bool {
    std::fs::metadata(dir.join(f.path))
        .map(|m| m.is_file() && m.len() >= f.min_bytes)
        .unwrap_or(false)
}

pub fn source_urls(spec: &ModelSpec, file: &str) -> [String; 3] {
    [
        format!("https://huggingface.co/{}/resolve/main/{}", spec.hf_repo, file),
        format!("https://hf-mirror.com/{}/resolve/main/{}", spec.hf_repo, file),
        format!("https://modelscope.cn/models/{}/resolve/master/{}", spec.ms_repo, file),
    ]
}

/// Pure post-download validation (rejects HTML error pages, non-JSON json,
/// and truncated bodies).
fn validate_download(
    file_name: &str,
    content_type: Option<&str>,
    expected_len: Option<u64>,
    written: u64,
    first_byte: Option<u8>,
) -> anyhow::Result<()> {
    if let Some(ct) = content_type {
        if ct.trim_start().to_ascii_lowercase().starts_with("text/html") {
            anyhow::bail!("unexpected content-type {ct}");
        }
    }
    if file_name.ends_with(".json") && first_byte != Some(b'{') {
        anyhow::bail!("{file_name} does not look like JSON");
    }
    if let Some(exp) = expected_len {
        if written != exp {
            anyhow::bail!("length mismatch: wrote {written}, expected {exp}");
        }
    }
    Ok(())
}

/// Progress of the file currently downloading.
#[derive(Clone, Debug)]
pub struct ProgressTick {
    pub file: String,
    pub source_host: String,
    pub downloaded: u64,
    pub total_for_file: u64,
}

pub type OnProgress<'a> = &'a (dyn Fn(ProgressTick) + Send + Sync);

/// Polled per streamed chunk and between files; returning false cancels the download.
pub type KeepGoing<'a> = &'a (dyn Fn() -> bool + Send + Sync);

/// The download was cancelled through `keep_going` (models toggled off). Distinct
/// from a source failure: no other source is tried and it is not a "failed" download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("model download cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// True when `e` (anywhere in its chain) is a [`Cancelled`].
pub fn is_cancelled(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.is::<Cancelled>())
}

fn host_of(url: &str) -> String {
    url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or(url).to_string()
}

/// Download one URL to `part`, returning bytes written (validated).
/// `on_progress` fires on headers, at most every 250 ms, and at completion.
/// `keep_going` is polled before every chunk; once false the stream is dropped
/// and [`Cancelled`] is returned (the caller deletes the `.part`).
async fn fetch_to(
    client: &reqwest::Client,
    url: &str,
    file_name: &str,
    part: &Path,
    on_progress: OnProgress<'_>,
    keep_going: KeepGoing<'_>,
) -> anyhow::Result<u64> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    // Bound connect + time-to-headers regardless of how the client was built.
    let resp = tokio::time::timeout(std::time::Duration::from_secs(15), client.get(url).send())
        .await
        .map_err(|_| anyhow::anyhow!("connect/headers timeout"))??
        .error_for_status()?;
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let expected_len = resp.content_length();
    let tick = |downloaded: u64| ProgressTick {
        file: file_name.to_string(),
        source_host: host_of(url),
        downloaded,
        total_for_file: expected_len.unwrap_or(0),
    };
    on_progress(tick(0));
    let mut last_tick = std::time::Instant::now();
    let mut file = tokio::fs::File::create(part).await?;
    let mut stream = resp.bytes_stream();
    let mut total = 0u64;
    let mut first_byte: Option<u8> = None;
    loop {
        if !keep_going() {
            return Err(anyhow::Error::new(Cancelled));
        }
        // 60 s read timeout between chunks.
        let next = tokio::time::timeout(std::time::Duration::from_secs(60), stream.next())
            .await
            .map_err(|_| anyhow::anyhow!("read timeout"))?;
        match next {
            Some(chunk) => {
                let chunk = chunk?;
                if first_byte.is_none() {
                    first_byte = chunk.iter().copied().find(|b| !b.is_ascii_whitespace());
                }
                total += chunk.len() as u64;
                file.write_all(&chunk).await?;
                if last_tick.elapsed() >= std::time::Duration::from_millis(250) {
                    last_tick = std::time::Instant::now();
                    on_progress(tick(total));
                }
            }
            None => break,
        }
    }
    file.flush().await?;
    file.sync_all().await?;
    validate_download(file_name, content_type.as_deref(), expected_len, total, first_byte)?;
    on_progress(tick(total));
    Ok(total)
}

fn spec_lock(id: &'static str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static LOCKS: OnceLock<Mutex<HashMap<&'static str, Arc<tokio::sync::Mutex<()>>>>> =
        OnceLock::new();
    LOCKS
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .entry(id)
        .or_default()
        .clone()
}

/// Ensure all files of `spec` exist under `<models_root>/<id>`; downloads the
/// missing ones, trying each source in order. Returns the model dir.
///
/// Time-to-headers (15 s) and per-chunk read (60 s) timeouts are enforced here;
/// there is no overall timeout (files are ~570 MB). Concurrent calls for the
/// same spec are serialised. `keep_going` turning false cancels mid-stream: the
/// `.part` is deleted and an error that [`is_cancelled`] is returned.
pub async fn ensure_model(
    client: &reqwest::Client,
    models_root: &Path,
    spec: &ModelSpec,
    on_progress: OnProgress<'_>,
    keep_going: KeepGoing<'_>,
) -> anyhow::Result<PathBuf> {
    let sources = |file: &str| source_urls(spec, file).to_vec();
    ensure_model_from(client, models_root, spec, &sources, on_progress, keep_going).await
}

/// [`ensure_model`] with the source list injected (tests point it at a local server).
async fn ensure_model_from(
    client: &reqwest::Client,
    models_root: &Path,
    spec: &ModelSpec,
    sources: &(dyn Fn(&str) -> Vec<String> + Send + Sync),
    on_progress: OnProgress<'_>,
    keep_going: KeepGoing<'_>,
) -> anyhow::Result<PathBuf> {
    // Process-wide per-spec guard so concurrent callers don't clobber `.part`.
    let lock = spec_lock(spec.id);
    let _guard = lock.lock().await;
    let dir = model_dir(models_root, spec);
    for f in spec.files {
        if file_ready(&dir, f) {
            continue;
        }
        if !keep_going() {
            return Err(anyhow::Error::new(Cancelled));
        }
        let dest = dir.join(f.path);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let part = PathBuf::from(format!("{}.part", dest.display()));
        let mut last_err = anyhow::anyhow!("no sources");
        let mut done = false;
        for url in sources(f.path) {
            let res = fetch_to(client, &url, f.path, &part, on_progress, keep_going).await.and_then(|n| {
                if n >= f.min_bytes {
                    Ok(n)
                } else {
                    Err(anyhow::anyhow!("{} too small: {} < {}", url, n, f.min_bytes))
                }
            });
            match res {
                Ok(n) => {
                    tokio::fs::rename(&part, &dest).await?;
                    tracing::info!(file = f.path, source = %url, bytes = n, "memory model file downloaded");
                    done = true;
                    break;
                }
                Err(e) if is_cancelled(&e) => {
                    let _ = tokio::fs::remove_file(&part).await;
                    tracing::info!(file = f.path, "memory model download cancelled");
                    return Err(e);
                }
                Err(e) => {
                    tracing::warn!(file = f.path, source = %url, error = %e, "memory model source failed");
                    let _ = tokio::fs::remove_file(&part).await;
                    last_err = e.context(format!("download {} from {}", f.path, url));
                }
            }
        }
        if !done {
            return Err(last_err);
        }
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_sparse(p: &Path, len: u64) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::File::create(p).unwrap().set_len(len).unwrap();
    }

    fn fill(root: &Path, spec: &ModelSpec) {
        for f in spec.files {
            write_sparse(&model_dir(root, spec).join(f.path), f.min_bytes);
        }
    }

    #[test]
    fn source_urls_exact() {
        assert_eq!(
            source_urls(&BGE_M3_INT8, "tokenizer.json"),
            [
                "https://huggingface.co/Xenova/bge-m3/resolve/main/tokenizer.json".to_string(),
                "https://hf-mirror.com/Xenova/bge-m3/resolve/main/tokenizer.json".to_string(),
                "https://modelscope.cn/models/Xenova/bge-m3/resolve/master/tokenizer.json"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn model_dir_is_root_joined_id() {
        let root = Path::new("/x");
        assert_eq!(model_dir(root, &BGE_M3_INT8), Path::new("/x/bge-m3-int8"));
    }

    #[test]
    fn is_ready_lifecycle() {
        let t = tempfile::tempdir().unwrap();
        assert!(!is_ready(t.path(), &BGE_M3_INT8));
        fill(t.path(), &BGE_M3_INT8);
        assert!(is_ready(t.path(), &BGE_M3_INT8));
        // too-small file => not ready
        write_sparse(&model_dir(t.path(), &BGE_M3_INT8).join("onnx/model_int8.onnx"), 10);
        assert!(!is_ready(t.path(), &BGE_M3_INT8));
    }

    #[test]
    fn part_file_is_not_ready() {
        let t = tempfile::tempdir().unwrap();
        fill(t.path(), &BGE_M3_INT8);
        let d = model_dir(t.path(), &BGE_M3_INT8);
        fs::remove_file(d.join("onnx/model_int8.onnx")).unwrap();
        write_sparse(&d.join("onnx/model_int8.onnx.part"), 600_000_000);
        assert!(!is_ready(t.path(), &BGE_M3_INT8));
    }

    #[test]
    fn validate_download_cases() {
        assert!(validate_download("a.json", Some("text/html; charset=utf-8"), None, 5, Some(b'{')).is_err());
        assert!(validate_download("a.json", Some("application/json"), None, 5, Some(b'<')).is_err());
        assert!(validate_download("a.json", None, None, 0, None).is_err());
        assert!(validate_download("m.onnx", None, Some(100), 99, Some(0)).is_err());
        assert!(validate_download("a.json", Some("application/json"), Some(5), 5, Some(b'{')).is_ok());
        assert!(validate_download("m.onnx", Some("application/octet-stream"), Some(7), 7, Some(1)).is_ok());
    }

    #[tokio::test]
    #[ignore]
    async fn downloads_tokenizer_json_live() {
        const SMALL: &[ModelFile] = &[
            ModelFile { path: "config.json", min_bytes: 1 },
            ModelFile { path: "tokenizer.json", min_bytes: 1_000_000 },
        ];
        let spec = ModelSpec { files: SMALL, ..BGE_RERANKER_V2_M3_INT8 };
        let t = tempfile::tempdir().unwrap();
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap();
        let dir = ensure_model(&client, t.path(), &spec, &|_| {}, &|| true).await.unwrap();
        assert!(is_ready(t.path(), &spec));
        assert!(dir.join("tokenizer.json").exists());
    }

    #[tokio::test]
    #[ignore]
    async fn download_all_models_live() {
        let Ok(root) = std::env::var("UCLAW_TEST_MODELS") else {
            println!("UCLAW_TEST_MODELS unset; skipping");
            return;
        };
        let root = PathBuf::from(root);
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap();
        for spec in [&BGE_M3_INT8, &BGE_RERANKER_V2_M3_INT8] {
            ensure_model(&client, &root, spec, &|_| {}, &|| true).await.unwrap();
            assert!(is_ready(&root, spec), "{} not ready", spec.id);
        }
    }

    /// Local HTTP server that streams an endless-ish octet body slowly (one
    /// 16 KiB chunk every 5 ms). Returns its base URL and a "bytes sent" counter.
    async fn slow_server() -> (String, std::sync::Arc<std::sync::atomic::AtomicU64>) {
        use std::sync::atomic::{AtomicU64, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let sent = std::sync::Arc::new(AtomicU64::new(0));
        let sent2 = sent.clone();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let sent = sent2.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = sock.read(&mut buf).await; // request head (small)
                    let len: u64 = 600_000_000;
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {len}\r\n\r\n"
                    );
                    if sock.write_all(head.as_bytes()).await.is_err() {
                        return;
                    }
                    let chunk = vec![1u8; 16 * 1024];
                    loop {
                        if sock.write_all(&chunk).await.is_err() {
                            return; // client went away
                        }
                        sent.fetch_add(chunk.len() as u64, Ordering::SeqCst);
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                });
            }
        });
        (format!("http://{addr}"), sent)
    }

    #[tokio::test]
    async fn keep_going_false_mid_stream_cancels_and_removes_part() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        const ONE: &[ModelFile] = &[ModelFile { path: "onnx/model_int8.onnx", min_bytes: 500_000_000 }];
        let spec = ModelSpec { id: "cancel-test-spec", files: ONE, ..BGE_M3_INT8 };
        let (base, sent) = slow_server().await;
        let t = tempfile::tempdir().unwrap();
        let client = reqwest::Client::new();
        let sources = move |file: &str| vec![format!("{base}/{file}"), format!("{base}/second/{file}")];
        // Flip to false after a few chunks (the toggle turned off mid-stream).
        let polls = AtomicUsize::new(0);
        let keep = move || polls.fetch_add(1, Ordering::SeqCst) < 6;
        let ticks = AtomicUsize::new(0);
        let started = std::time::Instant::now();
        let err = ensure_model_from(&client, t.path(), &spec, &sources, &|_| {
            ticks.fetch_add(1, Ordering::SeqCst);
        }, &keep)
        .await
        .expect_err("cancelled");
        assert!(is_cancelled(&err), "distinct Cancelled error, got {err:#}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10), "stopped promptly");
        let dest = model_dir(t.path(), &spec).join("onnx/model_int8.onnx");
        assert!(!dest.exists(), "no final file");
        assert!(!PathBuf::from(format!("{}.part", dest.display())).exists(), ".part removed");
        // The second source was not tried and the stream stopped: the server stops
        // sending once the connection is dropped (well short of 600 MB).
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let after = sent.load(Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(sent.load(Ordering::SeqCst), after, "server no longer streaming");
        assert!(after < 100_000_000);
        assert!(ticks.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn keep_going_false_before_start_downloads_nothing() {
        const ONE: &[ModelFile] = &[ModelFile { path: "config.json", min_bytes: 1 }];
        let spec = ModelSpec { id: "cancel-before-spec", files: ONE, ..BGE_M3_INT8 };
        let t = tempfile::tempdir().unwrap();
        let client = reqwest::Client::new();
        let sources = |_: &str| -> Vec<String> { panic!("no source should be tried") };
        let err = ensure_model_from(&client, t.path(), &spec, &sources, &|_| {}, &|| false)
            .await
            .expect_err("cancelled");
        assert!(is_cancelled(&err));
        assert!(!model_dir(t.path(), &spec).join("config.json").exists());
    }
}
