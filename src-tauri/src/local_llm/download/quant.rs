// SPDX-License-Identifier: AGPL-3.0-or-later
//! Quantization variants of the MiniCPM5-1B GGUF.
//!
//! Filenames, expected byte sizes, and SHA256 checksums are sourced from the
//! live repo metadata (HF tree API LFS `oid` + ModelScope `X-Linked-Etag`,
//! confirmed identical across both sources on 2026-06-03):
//!   - HF tree:  https://huggingface.co/api/models/openbmb/MiniCPM5-1B-GGUF/tree/main
//!   - ModelScope HEAD `X-Linked-Etag` agrees with the HF LFS oid for all 3 quants.
//! The LFS `oid` is the SHA256 of the file content, so it doubles as our verify hash.

use serde::{Deserialize, Serialize};

/// The GGUF quantization to download / run. `Q4KM` is the default (smallest,
/// good quality for on-device).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quant {
    /// Q4_K_M — default. ~688 MB.
    #[serde(rename = "q4_k_m", alias = "Q4_K_M", alias = "q4km")]
    Q4KM,
    /// Q8_0 — ~1.15 GB.
    #[serde(rename = "q8_0", alias = "Q8_0")]
    Q8_0,
    /// F16 — full half precision, ~2.17 GB.
    #[serde(rename = "f16", alias = "F16")]
    F16,
}

impl Default for Quant {
    fn default() -> Self {
        Quant::Q4KM
    }
}

impl Quant {
    /// All variants, in display order.
    pub const ALL: [Quant; 3] = [Quant::Q4KM, Quant::Q8_0, Quant::F16];

    /// The GGUF filename in the repo.
    pub fn filename(self) -> &'static str {
        match self {
            Quant::Q4KM => "MiniCPM5-1B-Q4_K_M.gguf",
            Quant::Q8_0 => "MiniCPM5-1B-Q8_0.gguf",
            Quant::F16 => "MiniCPM5-1B-F16.gguf",
        }
    }

    /// Expected total byte size (from repo LFS metadata).
    pub fn expected_size(self) -> u64 {
        match self {
            Quant::Q4KM => 688_065_920,
            Quant::Q8_0 => 1_153_529_216,
            Quant::F16 => 2_166_551_936,
        }
    }

    /// SHA256 of the file content (lowercase hex). Empty `&str` means "no SHA
    /// available — verify size-only" (none today; all three are known).
    pub fn sha256(self) -> &'static str {
        match self {
            Quant::Q4KM => "81b64d05a23b17b34c475f42b3e72fbde62d4b92cc34541f7a8031d0752deafa",
            Quant::Q8_0 => "0dc7638539067268774c275a14a6ec9c7e01f7eeb2cff606c8590361fa527e4c",
            Quant::F16 => "68c40b08b1242754a107b9510af89aa75b10c75843ca7844643c70956b7f1e3d",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_q4km() {
        assert_eq!(Quant::default(), Quant::Q4KM);
    }

    #[test]
    fn filenames_are_distinct_ggufs() {
        for q in Quant::ALL {
            assert!(q.filename().ends_with(".gguf"), "{q:?} not a gguf");
        }
        assert_eq!(Quant::Q4KM.filename(), "MiniCPM5-1B-Q4_K_M.gguf");
        assert_eq!(Quant::Q8_0.filename(), "MiniCPM5-1B-Q8_0.gguf");
        assert_eq!(Quant::F16.filename(), "MiniCPM5-1B-F16.gguf");
    }

    #[test]
    fn sizes_match_repo_metadata() {
        assert_eq!(Quant::Q4KM.expected_size(), 688_065_920);
        assert_eq!(Quant::Q8_0.expected_size(), 1_153_529_216);
        assert_eq!(Quant::F16.expected_size(), 2_166_551_936);
    }

    #[test]
    fn sha256_is_64_hex_chars_for_all() {
        for q in Quant::ALL {
            let s = q.sha256();
            assert_eq!(s.len(), 64, "{q:?} sha not 64 chars");
            assert!(s.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        }
    }

    #[test]
    fn serde_round_trips_via_string_repr() {
        for (q, repr) in [
            (Quant::Q4KM, "\"q4_k_m\""),
            (Quant::Q8_0, "\"q8_0\""),
            (Quant::F16, "\"f16\""),
        ] {
            let json = serde_json::to_string(&q).unwrap();
            assert_eq!(json, repr, "serialize {q:?}");
            let back: Quant = serde_json::from_str(&json).unwrap();
            assert_eq!(back, q, "round-trip {q:?}");
        }
    }

    #[test]
    fn serde_accepts_uppercase_aliases() {
        assert_eq!(serde_json::from_str::<Quant>("\"Q4_K_M\"").unwrap(), Quant::Q4KM);
        assert_eq!(serde_json::from_str::<Quant>("\"Q8_0\"").unwrap(), Quant::Q8_0);
        assert_eq!(serde_json::from_str::<Quant>("\"F16\"").unwrap(), Quant::F16);
    }
}
