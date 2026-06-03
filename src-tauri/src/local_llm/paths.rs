//! Filesystem layout for the local MiniCPM model.
use std::path::{Path, PathBuf};

use crate::local_llm::download::Quant;

/// The default-quant (Q4_K_M) GGUF filename. Back-compat constant for callers
/// that haven't been made quant-aware; equals `Quant::default().filename()`.
pub const MODEL_FILE: &str = "MiniCPM5-1B-Q4_K_M.gguf";

/// Directory holding the local model, under the uClaw data dir:
/// `<data_dir>/models/minicpm5-1b/`. All quants share this directory.
pub fn model_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("models").join("minicpm5-1b")
}

/// Full path to the default-quant (Q4_K_M) GGUF file. Back-compat shim for
/// callers that don't track a quant; delegates to [`model_file_for`].
pub fn model_file_path(data_dir: &Path) -> PathBuf {
    model_file_for(data_dir, Quant::default())
}

/// Full path to the GGUF file for a specific quant.
pub fn model_file_for(data_dir: &Path, quant: Quant) -> PathBuf {
    model_dir(data_dir).join(quant.filename())
}

/// True iff the GGUF for `quant` is present and non-empty.
pub fn is_model_present(data_dir: &Path, quant: Quant) -> bool {
    std::fs::metadata(model_file_for(data_dir, quant))
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_file_const_matches_q4km() {
        assert_eq!(MODEL_FILE, Quant::Q4KM.filename());
    }

    #[test]
    fn paths_are_under_data_dir_models() {
        let d = Path::new("/tmp/uclaw-x");
        assert_eq!(model_dir(d), Path::new("/tmp/uclaw-x/models/minicpm5-1b"));
        assert_eq!(
            model_file_path(d),
            Path::new("/tmp/uclaw-x/models/minicpm5-1b/MiniCPM5-1B-Q4_K_M.gguf")
        );
    }

    #[test]
    fn model_file_for_is_quant_specific() {
        let d = Path::new("/tmp/uclaw-x");
        assert_eq!(
            model_file_for(d, Quant::Q4KM),
            Path::new("/tmp/uclaw-x/models/minicpm5-1b/MiniCPM5-1B-Q4_K_M.gguf")
        );
        assert_eq!(
            model_file_for(d, Quant::Q8_0),
            Path::new("/tmp/uclaw-x/models/minicpm5-1b/MiniCPM5-1B-Q8_0.gguf")
        );
        assert_eq!(
            model_file_for(d, Quant::F16),
            Path::new("/tmp/uclaw-x/models/minicpm5-1b/MiniCPM5-1B-F16.gguf")
        );
    }

    #[test]
    fn absent_model_reports_false() {
        assert!(!is_model_present(
            Path::new("/tmp/uclaw-does-not-exist-zzz"),
            Quant::default()
        ));
    }
}
