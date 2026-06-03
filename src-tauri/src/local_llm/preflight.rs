// SPDX-License-Identifier: AGPL-3.0-or-later
//! First-launch environment preflight for the local MiniCPM model (S3).
//!
//! Reports four checks the onboarding step renders as a ✅/⚠️/❌ checklist:
//!   - **disk**: free bytes on the volume holding the uClaw data dir, vs the
//!     quant's expected size + 50% headroom (hard requirement; download is
//!     blocked when this fails).
//!   - **ram**: available system memory vs a ~2 GiB heuristic for the 1B Q4
//!     model (warn-only — swap/CPU can still run it).
//!   - **metal**: GPU acceleration availability. On macOS we report `true`
//!     (every Metal-capable Mac since 2012; mistral.rs uses Metal there). On
//!     other platforms `false` → CPU fallback, which is a warning, not a fail.
//!   - **network**: per-source reachability (reuses the S2 download-source
//!     latency probe), plus the fastest reachable source if any.
//!
//! Threshold logic ([`disk_ok`], [`ram_ok`]) is pure and unit-tested with
//! injected numbers; the `sysinfo`/network plumbing is exercised live, not in
//! unit tests.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::local_llm::download::source::Source;
use crate::local_llm::download::Quant;

/// Probe budget per source for the preflight network check. Kept short so the
/// onboarding "环境检查中" spinner stays snappy.
const PREFLIGHT_PROBE_TIMEOUT: Duration = Duration::from_secs(6);

/// Minimum available RAM heuristic for the 1B Q4 model: ~2 GiB.
const MIN_RAM_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Tri-state status for a single check, mirrored by the frontend checklist.
/// Currently informational on the Rust side (the booleans drive blocking
/// logic); serialized so the UI can render a per-item icon directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

/// Per-source reachability + the fastest reachable source (if any).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkReport {
    /// `modelscope` reachable within the probe budget.
    pub modelscope_reachable: bool,
    /// `huggingface` reachable within the probe budget.
    pub huggingface_reachable: bool,
    /// Label of the fastest reachable source (`"modelscope"` / `"huggingface"`),
    /// or `None` when neither responded.
    pub fastest: Option<String>,
    /// True iff at least one source is reachable (download is possible now).
    pub any_reachable: bool,
}

/// The full environment report the onboarding step renders. Booleans drive the
/// frontend's block/warn logic; raw byte counts let it show "需要 ~1GB / 可用 X".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvReport {
    pub disk_free_bytes: u64,
    /// `disk_free_bytes >= quant.expected_size() * 3/2`.
    pub disk_ok: bool,
    /// Bytes of disk headroom required (`quant.expected_size() * 3/2`), so the
    /// UI can show "需要 vs 可用" without re-deriving the rule.
    pub disk_required_bytes: u64,
    pub ram_total_bytes: u64,
    pub ram_available_bytes: u64,
    /// `ram_available_bytes >= ~2 GiB`.
    pub ram_ok: bool,
    /// GPU acceleration available (macOS → true; else CPU fallback).
    pub metal_available: bool,
    pub network: NetworkReport,
}

/// Pure disk threshold: free space must cover the quant plus 50% headroom
/// (download writes a `.part` then renames, and the runtime needs a little
/// slack). `expected_size() * 3 / 2` avoids float rounding.
pub fn disk_ok(free_bytes: u64, quant: Quant) -> bool {
    free_bytes >= disk_required(quant)
}

/// The required-free-bytes for `quant` (quant size + 50% headroom).
pub fn disk_required(quant: Quant) -> u64 {
    // Saturating to be safe; expected sizes are well under u64::MAX / 2.
    quant.expected_size().saturating_mul(3) / 2
}

/// Pure RAM threshold: a 1B Q4 model needs ~2 GiB of available memory to run
/// comfortably. Warn-only at the UI layer.
pub fn ram_ok(available_bytes: u64) -> bool {
    available_bytes >= MIN_RAM_BYTES
}

/// Metal/GPU availability. On macOS every supported device has Metal and
/// mistral.rs uses it, so we report `true` without a device probe (a real
/// `MTLCreateSystemDefaultDevice` query is optional for S3). Elsewhere the
/// runtime falls back to CPU, reported as `false` (a warning, not a failure).
pub fn metal_available() -> bool {
    cfg!(target_os = "macos")
}

/// Free bytes on the volume holding `data_dir`. Picks the disk whose mount
/// point is the longest prefix of `data_dir` (the most specific mount), falling
/// back to the largest total-space disk, then 0 if `sysinfo` lists none.
fn disk_free_for(data_dir: &Path) -> u64 {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut best: Option<(usize, u64)> = None; // (mount_len, available)
    let mut largest: Option<(u64, u64)> = None; // (total, available)
    for disk in disks.list() {
        let mount = disk.mount_point();
        let avail = disk.available_space();
        if data_dir.starts_with(mount) {
            let len = mount.as_os_str().len();
            if best.map(|(l, _)| len > l).unwrap_or(true) {
                best = Some((len, avail));
            }
        }
        if largest.map(|(t, _)| disk.total_space() > t).unwrap_or(true) {
            largest = Some((disk.total_space(), avail));
        }
    }
    best.map(|(_, a)| a)
        .or(largest.map(|(_, a)| a))
        .unwrap_or(0)
}

/// Available system memory (bytes) — refreshes only memory for speed.
fn ram_snapshot() -> (u64, u64) {
    use sysinfo::{MemoryRefreshKind, RefreshKind};
    let sys = sysinfo::System::new_with_specifics(
        RefreshKind::new().with_memory(MemoryRefreshKind::everything()),
    );
    (sys.total_memory(), sys.available_memory())
}

/// Probe both download sources concurrently for reachability.
async fn network_report(quant: Quant) -> NetworkReport {
    let ms = Source::ModelScope.reachable_latency(quant, PREFLIGHT_PROBE_TIMEOUT);
    let hf = Source::HuggingFace.reachable_latency(quant, PREFLIGHT_PROBE_TIMEOUT);
    let (ms, hf) = tokio::join!(ms, hf);
    let fastest = crate::local_llm::download::source::pick_fastest(ms, hf);
    NetworkReport {
        modelscope_reachable: ms.is_some(),
        huggingface_reachable: hf.is_some(),
        fastest: fastest.map(|s| s.label().to_string()),
        any_reachable: ms.is_some() || hf.is_some(),
    }
}

/// Run all four environment checks for `quant` against `data_dir`.
pub async fn check_environment(data_dir: &Path, quant: Quant) -> EnvReport {
    let disk_free_bytes = disk_free_for(data_dir);
    let (ram_total_bytes, ram_available_bytes) = ram_snapshot();
    let network = network_report(quant).await;
    EnvReport {
        disk_free_bytes,
        disk_ok: disk_ok(disk_free_bytes, quant),
        disk_required_bytes: disk_required(quant),
        ram_total_bytes,
        ram_available_bytes,
        ram_ok: ram_ok(ram_available_bytes),
        metal_available: metal_available(),
        network,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_required_is_quant_plus_half() {
        // Q4KM is 688_065_920; * 3/2 = 1_032_098_880.
        assert_eq!(disk_required(Quant::Q4KM), 688_065_920u64 * 3 / 2);
        assert_eq!(disk_required(Quant::Q8_0), 1_153_529_216u64 * 3 / 2);
        assert_eq!(disk_required(Quant::F16), 2_166_551_936u64 * 3 / 2);
    }

    #[test]
    fn disk_ok_table() {
        let req = disk_required(Quant::Q4KM);
        // exactly at the threshold → ok
        assert!(disk_ok(req, Quant::Q4KM));
        // one byte over → ok
        assert!(disk_ok(req + 1, Quant::Q4KM));
        // one byte under → fail
        assert!(!disk_ok(req - 1, Quant::Q4KM));
        // zero free → fail
        assert!(!disk_ok(0, Quant::Q4KM));
        // plenty → ok
        assert!(disk_ok(50 * 1024 * 1024 * 1024, Quant::F16));
    }

    #[test]
    fn ram_ok_table() {
        assert!(!ram_ok(0));
        assert!(!ram_ok(MIN_RAM_BYTES - 1));
        assert!(ram_ok(MIN_RAM_BYTES));
        assert!(ram_ok(MIN_RAM_BYTES + 1));
        assert!(ram_ok(16 * 1024 * 1024 * 1024));
    }

    #[test]
    fn metal_matches_target_os() {
        assert_eq!(metal_available(), cfg!(target_os = "macos"));
    }

    #[test]
    fn env_report_serializes_camel_case() {
        let report = EnvReport {
            disk_free_bytes: 10_000_000_000,
            disk_ok: true,
            disk_required_bytes: 1_032_098_880,
            ram_total_bytes: 17_179_869_184,
            ram_available_bytes: 8_589_934_592,
            ram_ok: true,
            metal_available: true,
            network: NetworkReport {
                modelscope_reachable: true,
                huggingface_reachable: false,
                fastest: Some("modelscope".to_string()),
                any_reachable: true,
            },
        };
        let json = serde_json::to_value(&report).unwrap();
        // camelCase keys the frontend bridge expects.
        assert_eq!(json["diskFreeBytes"], 10_000_000_000u64);
        assert_eq!(json["diskOk"], true);
        assert_eq!(json["diskRequiredBytes"], 1_032_098_880u64);
        assert_eq!(json["ramTotalBytes"], 17_179_869_184u64);
        assert_eq!(json["ramAvailableBytes"], 8_589_934_592u64);
        assert_eq!(json["ramOk"], true);
        assert_eq!(json["metalAvailable"], true);
        assert_eq!(json["network"]["modelscopeReachable"], true);
        assert_eq!(json["network"]["huggingfaceReachable"], false);
        assert_eq!(json["network"]["fastest"], "modelscope");
        assert_eq!(json["network"]["anyReachable"], true);

        // round-trips
        let back: EnvReport = serde_json::from_value(json).unwrap();
        assert_eq!(back.disk_free_bytes, report.disk_free_bytes);
        assert_eq!(back.network.fastest.as_deref(), Some("modelscope"));
    }

    #[test]
    fn check_status_serializes_lowercase() {
        assert_eq!(serde_json::to_value(CheckStatus::Ok).unwrap(), "ok");
        assert_eq!(serde_json::to_value(CheckStatus::Warn).unwrap(), "warn");
        assert_eq!(serde_json::to_value(CheckStatus::Fail).unwrap(), "fail");
    }
}
