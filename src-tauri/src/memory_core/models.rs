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

/// Download one URL to `part`, returning bytes written.
async fn fetch_to(client: &reqwest::Client, url: &str, part: &Path) -> anyhow::Result<u64> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let resp = client
        .get(url)
        .timeout(std::time::Duration::from_secs(60 * 60 * 6))
        .send()
        .await?
        .error_for_status()?;
    let mut file = tokio::fs::File::create(part).await?;
    let mut stream = resp.bytes_stream();
    let mut total = 0u64;
    loop {
        // 60 s read timeout between chunks.
        let next = tokio::time::timeout(std::time::Duration::from_secs(60), stream.next())
            .await
            .map_err(|_| anyhow::anyhow!("read timeout"))?;
        match next {
            Some(chunk) => {
                let chunk = chunk?;
                total += chunk.len() as u64;
                file.write_all(&chunk).await?;
            }
            None => break,
        }
    }
    file.flush().await?;
    file.sync_all().await?;
    Ok(total)
}

/// Ensure all files of `spec` exist under `<models_root>/<id>`; downloads the
/// missing ones, trying each source in order. Returns the model dir.
///
/// Callers should build `client` with `connect_timeout(15s)`; a per-chunk read
/// timeout (60 s) is enforced here. No overall timeout is applied.
pub async fn ensure_model(
    client: &reqwest::Client,
    models_root: &Path,
    spec: &ModelSpec,
) -> anyhow::Result<PathBuf> {
    let dir = model_dir(models_root, spec);
    for f in spec.files {
        if file_ready(&dir, f) {
            continue;
        }
        let dest = dir.join(f.path);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let part = PathBuf::from(format!("{}.part", dest.display()));
        let mut last_err = anyhow::anyhow!("no sources");
        let mut done = false;
        for url in source_urls(spec, f.path) {
            let res = fetch_to(client, &url, &part).await.and_then(|n| {
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
        let dir = ensure_model(&client, t.path(), &spec).await.unwrap();
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
            ensure_model(&client, &root, spec).await.unwrap();
            assert!(is_ready(&root, spec), "{} not ready", spec.id);
        }
    }
}
