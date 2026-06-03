// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local in-process LLM runtime (S1): MiniCPM5-1B-GGUF via mistralrs.
pub mod engine;
pub mod paths;
pub mod download;
pub mod persona;
pub mod preflight;
pub mod provider;
#[cfg(test)]
mod spike_test;
#[cfg(test)]
mod s4_spike_test;

use std::sync::{Arc, OnceLock, RwLock};
use engine::LocalLlmEngine;
use download::quant::Quant;

static ENGINE: OnceLock<Arc<LocalLlmEngine>> = OnceLock::new();

/// The active GGUF quant the engine should load. A module static (mirroring the
/// active-persona pattern in `commands/pet.rs`) keeps the single-engine contract
/// obvious — the UI persists the choice in `providers.json`, and `app.rs` seeds
/// this static from there at startup. Defaults to [`Quant::default`] (Q4_K_M).
fn active_quant_cell() -> &'static RwLock<Quant> {
    static QUANT: OnceLock<RwLock<Quant>> = OnceLock::new();
    QUANT.get_or_init(|| RwLock::new(Quant::default()))
}

/// The active GGUF quant the engine loads (defaults to Q4_K_M).
pub fn active_quant() -> Quant {
    *active_quant_cell().read().unwrap()
}

/// Set the active GGUF quant. Takes effect on the next model (re)load — call the
/// engine's [`LocalLlmEngine::unload`] after this to force a reload.
pub fn set_active_quant(quant: Quant) {
    *active_quant_cell().write().unwrap() = quant;
}

/// Initialize the global local engine (once, at startup). Does NOT load the
/// model — only constructs the handle + resolves paths (lazy).
pub fn init_local_engine(data_dir: &std::path::Path) -> Arc<LocalLlmEngine> {
    let e = Arc::new(LocalLlmEngine::new(data_dir.to_path_buf()));
    let _ = ENGINE.set(e.clone());
    e
}
/// Get the initialized engine, or None if `init_local_engine` wasn't called.
pub fn local_engine() -> Option<Arc<LocalLlmEngine>> { ENGINE.get().cloned() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_quant_set_and_get() {
        // The cell defaults to Quant::default() on first init. This is the only
        // test that writes the global static; it restores the default so the
        // rest of the test binary sees a clean value regardless of order.
        set_active_quant(Quant::F16);
        assert_eq!(active_quant(), Quant::F16);
        set_active_quant(Quant::Q8_0);
        assert_eq!(active_quant(), Quant::Q8_0);
        set_active_quant(Quant::default());
        assert_eq!(active_quant(), Quant::default());
    }
}
