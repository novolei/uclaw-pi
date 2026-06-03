//! Filesystem layout for the local MiniCPM model.
use std::path::{Path, PathBuf};

/// The Q4_K_M GGUF filename we download/expect. (Single quant for S1.)
pub const MODEL_FILE: &str = "MiniCPM5-1B-Q4_K_M.gguf";

/// Directory holding the local model, under the uClaw data dir:
/// `<data_dir>/models/minicpm5-1b/`.
pub fn model_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("models").join("minicpm5-1b")
}

/// Full path to the GGUF file.
pub fn model_file_path(data_dir: &Path) -> PathBuf {
    model_dir(data_dir).join(MODEL_FILE)
}

/// True iff the GGUF is present and non-empty.
pub fn is_model_present(data_dir: &Path) -> bool {
    std::fs::metadata(model_file_path(data_dir))
        .map(|m| m.is_file() && m.len() > 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_under_data_dir_models() {
        let d = Path::new("/tmp/uclaw-x");
        assert_eq!(model_dir(d), Path::new("/tmp/uclaw-x/models/minicpm5-1b"));
        assert_eq!(model_file_path(d), Path::new("/tmp/uclaw-x/models/minicpm5-1b/MiniCPM5-1B-Q4_K_M.gguf"));
    }

    #[test]
    fn absent_model_reports_false() {
        assert!(!is_model_present(Path::new("/tmp/uclaw-does-not-exist-zzz")));
    }
}
