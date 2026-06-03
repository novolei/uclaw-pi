// SPDX-License-Identifier: AGPL-3.0-or-later
//! Download sources for the MiniCPM GGUF + concurrent latency probe.
//!
//! Two mirrors host identical files (same SHA256, confirmed 2026-06-03):
//!   - HuggingFace: `…/resolve/main/<file>`  (302 -> xet CDN; supports Range)
//!   - ModelScope:  `…/resolve/master/<file>` (200; supports Range)
//! `probe_fastest` fires a tiny Range GET at both concurrently and picks the
//! lower-latency reachable one, so first-time setup works regardless of region.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::quant::Quant;

/// A mirror that serves the MiniCPM GGUF files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// modelscope.cn — typically fastest inside mainland China.
    ModelScope,
    /// huggingface.co — typically fastest elsewhere.
    HuggingFace,
}

impl Source {
    /// Both sources, in probe order.
    pub const ALL: [Source; 2] = [Source::ModelScope, Source::HuggingFace];

    /// Stable label for events / logs.
    pub fn label(self) -> &'static str {
        match self {
            Source::ModelScope => "modelscope",
            Source::HuggingFace => "huggingface",
        }
    }

    /// The raw-file URL for `quant` on this source.
    pub fn file_url(self, quant: Quant) -> String {
        let file = quant.filename();
        match self {
            Source::ModelScope => format!(
                "https://www.modelscope.cn/models/OpenBMB/MiniCPM5-1B-GGUF/resolve/master/{file}"
            ),
            Source::HuggingFace => {
                format!("https://huggingface.co/openbmb/MiniCPM5-1B-GGUF/resolve/main/{file}")
            }
        }
    }

    /// Measure round-trip latency to this source's file URL with a 1-byte Range
    /// GET (follows redirects). `None` if unreachable / non-2xx within `timeout`.
    pub async fn reachable_latency(self, quant: Quant, timeout: Duration) -> Option<Duration> {
        let client = reqwest::Client::builder().timeout(timeout).build().ok()?;
        let start = std::time::Instant::now();
        let resp = client
            .get(self.file_url(quant))
            .header(reqwest::header::RANGE, "bytes=0-0")
            .send()
            .await
            .ok()?;
        // 206 (partial) or 200 (range ignored) both mean reachable.
        if resp.status().is_success() {
            Some(start.elapsed())
        } else {
            None
        }
    }
}

/// Probe both sources concurrently and return the lower-latency reachable one.
/// Delegates measurement to `Source::reachable_latency` (real network).
pub async fn probe_fastest(quant: Quant, timeout: Duration) -> Result<Source, ProbeError> {
    let ms = Source::ModelScope.reachable_latency(quant, timeout);
    let hf = Source::HuggingFace.reachable_latency(quant, timeout);
    let (ms, hf) = tokio::join!(ms, hf);
    pick_fastest(ms, hf).ok_or(ProbeError::NoSource)
}

/// Pure selection logic, factored out so it's unit-testable with injected
/// latencies (no network). Picks the lower reachable latency; ties go to
/// ModelScope (listed first). `None` if neither reachable.
pub(crate) fn pick_fastest(
    modelscope: Option<Duration>,
    huggingface: Option<Duration>,
) -> Option<Source> {
    match (modelscope, huggingface) {
        (Some(m), Some(h)) => Some(if m <= h {
            Source::ModelScope
        } else {
            Source::HuggingFace
        }),
        (Some(_), None) => Some(Source::ModelScope),
        (None, Some(_)) => Some(Source::HuggingFace),
        (None, None) => None,
    }
}

/// Probe failure: neither mirror responded within the timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProbeError {
    #[error("no download source reachable")]
    NoSource,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modelscope_url_uses_resolve_master() {
        let u = Source::ModelScope.file_url(Quant::Q4KM);
        assert_eq!(
            u,
            "https://www.modelscope.cn/models/OpenBMB/MiniCPM5-1B-GGUF/resolve/master/MiniCPM5-1B-Q4_K_M.gguf"
        );
    }

    #[test]
    fn huggingface_url_uses_resolve_main() {
        let u = Source::HuggingFace.file_url(Quant::Q8_0);
        assert_eq!(
            u,
            "https://huggingface.co/openbmb/MiniCPM5-1B-GGUF/resolve/main/MiniCPM5-1B-Q8_0.gguf"
        );
    }

    #[test]
    fn urls_vary_by_quant() {
        for q in Quant::ALL {
            assert!(Source::HuggingFace.file_url(q).ends_with(q.filename()));
            assert!(Source::ModelScope.file_url(q).ends_with(q.filename()));
        }
    }

    #[test]
    fn pick_lower_latency_of_two() {
        let fast = Duration::from_millis(50);
        let slow = Duration::from_millis(500);
        assert_eq!(pick_fastest(Some(fast), Some(slow)), Some(Source::ModelScope));
        assert_eq!(pick_fastest(Some(slow), Some(fast)), Some(Source::HuggingFace));
    }

    #[test]
    fn pick_falls_back_to_the_only_reachable() {
        let d = Duration::from_millis(100);
        assert_eq!(pick_fastest(Some(d), None), Some(Source::ModelScope));
        assert_eq!(pick_fastest(None, Some(d)), Some(Source::HuggingFace));
    }

    #[test]
    fn pick_none_when_neither_reachable() {
        assert_eq!(pick_fastest(None, None), None);
    }

    #[test]
    fn tie_goes_to_modelscope() {
        let d = Duration::from_millis(100);
        assert_eq!(pick_fastest(Some(d), Some(d)), Some(Source::ModelScope));
    }

    /// Live network — run manually with `--ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_probe_picks_a_reachable_source() {
        let s = probe_fastest(Quant::Q4KM, Duration::from_secs(10)).await;
        assert!(s.is_ok(), "expected a reachable source: {s:?}");
    }
}
