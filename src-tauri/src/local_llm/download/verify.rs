// SPDX-License-Identifier: AGPL-3.0-or-later
//! Post-download integrity check: byte size + streaming SHA256 against the
//! per-quant metadata in [`super::quant`].
//!
//! SHA is streamed in fixed-size reads so we never hold the (multi-GB) file in
//! memory. If a quant ever has an empty `sha256()` it falls back to size-only
//! and logs (none today — all three SHAs are known).

use std::path::Path;

use sha2::{Digest, Sha256};

use super::quant::Quant;

/// Verification failure.
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("io error: {0}")]
    Io(String),
    #[error("size mismatch: expected {expected} bytes, got {got}")]
    Size { expected: u64, got: u64 },
    #[error("checksum mismatch: expected {expected}, got {got}")]
    Checksum { expected: String, got: String },
}

/// Verify `path` matches `quant`'s expected size and SHA256. Size is checked
/// first (cheap, catches truncation); SHA256 is then streamed. When the quant
/// exposes no SHA (`sha256()` empty), verification is size-only and logs a warn.
pub async fn verify(path: &Path, quant: Quant) -> Result<(), VerifyError> {
    let expected_size = quant.expected_size();
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|e| VerifyError::Io(e.to_string()))?;
    let got_size = meta.len();
    if got_size != expected_size {
        return Err(VerifyError::Size {
            expected: expected_size,
            got: got_size,
        });
    }

    let expected_sha = quant.sha256();
    if expected_sha.is_empty() {
        tracing::warn!(
            quant = ?quant,
            "no SHA256 metadata for quant; verified size-only"
        );
        return Ok(());
    }

    let got_sha = sha256_file(path).await?;
    if !got_sha.eq_ignore_ascii_case(expected_sha) {
        return Err(VerifyError::Checksum {
            expected: expected_sha.to_string(),
            got: got_sha,
        });
    }
    Ok(())
}

/// Stream the file through SHA256, returning the lowercase hex digest.
pub(crate) async fn sha256_file(path: &Path) -> Result<String, VerifyError> {
    use tokio::io::AsyncReadExt;

    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| VerifyError::Io(e.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20]; // 1 MiB
    loop {
        let n = file
            .read(&mut buf)
            .await
            .map_err(|e| VerifyError::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// SHA256("hello world") — known value.
    const HELLO_SHA: &str = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

    fn tmp_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("uclaw-verify-test-{name}"));
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(bytes).unwrap();
        p
    }

    #[tokio::test]
    async fn sha256_file_matches_known_vector() {
        let p = tmp_file("hello", b"hello world");
        let got = sha256_file(&p).await.unwrap();
        assert_eq!(got, HELLO_SHA);
        let _ = std::fs::remove_file(&p);
    }

    #[tokio::test]
    async fn size_mismatch_is_caught_before_hashing() {
        // A 1-byte file can never match a real quant's multi-MB expected size.
        let p = tmp_file("tiny", b"x");
        let err = verify(&p, Quant::Q4KM).await.unwrap_err();
        assert!(matches!(err, VerifyError::Size { .. }), "got {err:?}");
        let _ = std::fs::remove_file(&p);
    }
}
