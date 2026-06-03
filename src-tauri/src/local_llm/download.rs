//! Minimal single-source (ModelScope) downloader for the MiniCPM Q4_K_M GGUF.
//! Smart source selection / HF fallback / resumability is deferred to S2.
use std::path::{Path, PathBuf};
use crate::local_llm::paths::{model_dir, model_file_path, MODEL_FILE};

/// ModelScope raw-file URL for the Q4_K_M GGUF.
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
}

/// Download the GGUF to `<data_dir>/models/minicpm5-1b/`, streaming to a
/// `.part` file then atomically renaming. `on_progress(downloaded, total)` is
/// called as bytes arrive (`total` is 0 if the server omits Content-Length).
pub async fn download_from_modelscope(
    data_dir: &Path,
    on_progress: impl Fn(u64, u64) + Send,
) -> Result<PathBuf, DownloadError> {
    use futures::StreamExt;
    use tokio::io::AsyncWriteExt;

    let dir = model_dir(data_dir);
    tokio::fs::create_dir_all(&dir).await.map_err(|e| DownloadError::Io(e.to_string()))?;
    let final_path = model_file_path(data_dir);
    let part_path = dir.join(format!("{MODEL_FILE}.part"));

    let resp = reqwest::get(modelscope_url()).await.map_err(|e| DownloadError::Http(e.to_string()))?;
    let resp = resp.error_for_status().map_err(|e| DownloadError::Http(e.to_string()))?;
    let total = resp.content_length().unwrap_or(0);

    let mut file = tokio::fs::File::create(&part_path).await.map_err(|e| DownloadError::Io(e.to_string()))?;
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| DownloadError::Http(e.to_string()))?;
        file.write_all(&chunk).await.map_err(|e| DownloadError::Io(e.to_string()))?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }
    file.flush().await.map_err(|e| DownloadError::Io(e.to_string()))?;
    drop(file);

    if total > 0 && downloaded < total {
        return Err(DownloadError::Incomplete { got: downloaded });
    }
    tokio::fs::rename(&part_path, &final_path).await.map_err(|e| DownloadError::Io(e.to_string()))?;
    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_targets_modelscope_q4km() {
        let u = modelscope_url();
        assert!(u.starts_with("https://www.modelscope.cn/models/OpenBMB/MiniCPM5-1B-GGUF/"));
        assert!(u.ends_with(".gguf"));
    }
}
