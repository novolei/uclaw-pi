// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local in-process LLM runtime (S1): MiniCPM5-1B-GGUF via mistralrs.
pub mod engine;
pub mod paths;
pub mod download;
pub mod preflight;
pub mod provider;
#[cfg(test)]
mod spike_test;
#[cfg(test)]
mod s4_spike_test;

use std::sync::{Arc, OnceLock};
use engine::LocalLlmEngine;

static ENGINE: OnceLock<Arc<LocalLlmEngine>> = OnceLock::new();

/// Initialize the global local engine (once, at startup). Does NOT load the
/// model — only constructs the handle + resolves paths (lazy).
pub fn init_local_engine(data_dir: &std::path::Path) -> Arc<LocalLlmEngine> {
    let e = Arc::new(LocalLlmEngine::new(data_dir.to_path_buf()));
    let _ = ENGINE.set(e.clone());
    e
}
/// Get the initialized engine, or None if `init_local_engine` wasn't called.
pub fn local_engine() -> Option<Arc<LocalLlmEngine>> { ENGINE.get().cloned() }
