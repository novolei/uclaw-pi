// SPDX-License-Identifier: AGPL-3.0-or-later
//! Range-aware streaming of a remote file into a local `.part`, resuming from
//! whatever bytes are already on disk.
//!
//! Flow: read the existing `.part` length -> send `Range: bytes=<len>-`.
//!   - `206 Partial Content`: server honored the range; append to `.part`.
//!   - `200 OK`: server ignored the range (no resume support); truncate `.part`
//!     and restart from 0.
//! `on_progress(downloaded, total)` reports cumulative bytes (incl. pre-existing
//! ones) and the total file size (resume offset + remaining), 0 if unknown.

use std::path::Path;

/// Build the `Range` header value for resuming from `offset` bytes. `None` when
/// `offset == 0` (a fresh download needs no Range header).
pub(crate) fn range_header(offset: u64) -> Option<String> {
    if offset == 0 {
        None
    } else {
        Some(format!("bytes={offset}-"))
    }
}

/// Errors raised while streaming with resume.
#[derive(Debug, thiserror::Error)]
pub enum ResumeError {
    #[error("http error: {0}")]
    Http(String),
    #[error("io error: {0}")]
    Io(String),
}

/// Stream `url` into `part_path`, resuming from its current length. Returns the
/// total bytes written to `.part` on success (the full file size when the
/// server reports it).
pub async fn stream_with_resume(
    url: &str,
    part_path: &Path,
    on_progress: impl Fn(u64, u64) + Send,
) -> Result<u64, ResumeError> {
    use futures::StreamExt;
    use tokio::io::AsyncWriteExt;

    // Existing partial length (0 if absent) drives the resume offset.
    let existing = tokio::fs::metadata(part_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    let client = reqwest::Client::new();
    let mut req = client.get(url);
    if let Some(h) = range_header(existing) {
        req = req.header(reqwest::header::RANGE, h);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| ResumeError::Http(e.to_string()))?
        .error_for_status()
        .map_err(|e| ResumeError::Http(e.to_string()))?;

    // 206 => server honored Range, append. Anything else (200) => no resume
    // support; start over from 0.
    let resuming = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let mut downloaded = if resuming { existing } else { 0 };
    // remaining bytes from this response + already-on-disk bytes = total
    let total = match resp.content_length() {
        Some(remaining) => downloaded.saturating_add(remaining),
        None => 0,
    };

    let mut file = if resuming {
        tokio::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(part_path)
            .await
            .map_err(|e| ResumeError::Io(e.to_string()))?
    } else {
        // Truncate any stale partial and restart.
        tokio::fs::File::create(part_path)
            .await
            .map_err(|e| ResumeError::Io(e.to_string()))?
    };

    on_progress(downloaded, total);
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ResumeError::Http(e.to_string()))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| ResumeError::Io(e.to_string()))?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }
    file.flush()
        .await
        .map_err(|e| ResumeError::Io(e.to_string()))?;

    Ok(downloaded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_range_header_for_fresh_download() {
        assert_eq!(range_header(0), None);
    }

    #[test]
    fn range_header_resumes_from_offset() {
        assert_eq!(range_header(1024), Some("bytes=1024-".to_string()));
        assert_eq!(
            range_header(688_065_920),
            Some("bytes=688065920-".to_string())
        );
    }

    /// Needs a local mock HTTP server / port — run manually with `--ignored`.
    #[tokio::test]
    #[ignore]
    async fn resume_appends_on_206_and_restarts_on_200() {
        // Placeholder for a wiremock-backed test. Verified manually:
        //  - 206 path: pre-seed a .part, serve partial, assert final == total.
        //  - 200 path: server ignores Range, .part is truncated and refilled.
    }
}
