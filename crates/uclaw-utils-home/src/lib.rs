// SPDX-License-Identifier: Apache-2.0
//
// Adapted from codex-rs/utils/home-dir/src/lib.rs (https://github.com/openai/codex).
// Modified for uclaw: `CODEX_HOME` env var → `UCLAW_HOME`, default `.codex` → `.uclaw`,
// `find_codex_home` → `uclaw_home`. Copyright (c) OpenAI. Licensed under
// Apache License 2.0. See NOTICE in the repository root for the upstream
// commit pin and the list of modifications.

use dirs::home_dir;
use std::path::PathBuf;
use uclaw_utils_absolute_path::AbsolutePathBuf;

/// Returns the path to the uClaw configuration directory, which can be
/// overridden by the `UCLAW_HOME` environment variable. If not set, defaults
/// to `~/.uclaw`.
///
/// - If `UCLAW_HOME` is set, the value must exist and be a directory. The
///   value will be canonicalized and this function will Err otherwise.
/// - If `UCLAW_HOME` is not set, this function does not verify that the
///   directory exists — call sites that need the directory to exist should
///   create it themselves (e.g. `std::fs::create_dir_all`).
///
/// Per `BEHAVIOR.md` *uClaw-specific rules*, NEW code must reach `~/.uclaw`
/// through this function rather than constructing
/// `dirs::home_dir().join(".uclaw")` directly — the git pre-commit hook
/// (`scripts/git-hooks/checks/check-dirs-home-dir-uclaw.sh`) + Claude Code
/// PreToolUse hook (`.claude/hooks/check-uclaw-home.sh`) enforce this at
/// commit / edit time.
pub fn uclaw_home() -> std::io::Result<AbsolutePathBuf> {
    let env_override = std::env::var("UCLAW_HOME")
        .ok()
        .filter(|val| !val.is_empty());
    uclaw_home_from_env(env_override.as_deref())
}

fn uclaw_home_from_env(env_override: Option<&str>) -> std::io::Result<AbsolutePathBuf> {
    match env_override {
        Some(val) => {
            let path = PathBuf::from(val);
            let metadata = std::fs::metadata(&path).map_err(|err| match err.kind() {
                std::io::ErrorKind::NotFound => std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("UCLAW_HOME points to {val:?}, but that path does not exist"),
                ),
                _ => std::io::Error::new(
                    err.kind(),
                    format!("failed to read UCLAW_HOME {val:?}: {err}"),
                ),
            })?;

            if !metadata.is_dir() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("UCLAW_HOME points to {val:?}, but that path is not a directory"),
                ))
            } else {
                let canonical = path.canonicalize().map_err(|err| {
                    std::io::Error::new(
                        err.kind(),
                        format!("failed to canonicalize UCLAW_HOME {val:?}: {err}"),
                    )
                })?;
                AbsolutePathBuf::from_absolute_path(canonical)
            }
        }
        None => {
            let mut p = home_dir().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not find home directory",
                )
            })?;
            // uclaw-pi fork: home is ~/.uclaw-pi (separate from the original uClaw's
            // ~/.uclaw so the two never clobber each other's DB/config/skills).
            p.push(".uclaw-pi");
            AbsolutePathBuf::from_absolute_path(p)
        }
    }
}

/// Convenience wrapper: same as [`uclaw_home`] but returns a `PathBuf`
/// rather than an `AbsolutePathBuf`. Most call sites in `src-tauri/` historically
/// returned a `PathBuf` from `dirs::home_dir().join(".uclaw")` and this helper
/// preserves their downstream type signature during the Phase 0.5-T6 sweep.
pub fn uclaw_home_pathbuf() -> std::io::Result<PathBuf> {
    uclaw_home().map(|abs| abs.into_path_buf())
}

#[cfg(test)]
mod tests {
    use super::uclaw_home_from_env;
    use dirs::home_dir;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::io::ErrorKind;
    use tempfile::TempDir;
    use uclaw_utils_absolute_path::AbsolutePathBuf;

    #[test]
    fn missing_env_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let missing = temp_home.path().join("missing-uclaw-home");
        let missing_str = missing
            .to_str()
            .expect("missing uclaw home path should be valid utf-8");

        let err = uclaw_home_from_env(Some(missing_str)).expect_err("missing UCLAW_HOME");
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert!(
            err.to_string().contains("UCLAW_HOME"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn env_pointing_to_file_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let file_path = temp_home.path().join("uclaw-home.txt");
        fs::write(&file_path, "not a directory").expect("write temp file");
        let file_str = file_path
            .to_str()
            .expect("file uclaw home path should be valid utf-8");

        let err = uclaw_home_from_env(Some(file_str)).expect_err("file UCLAW_HOME");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("not a directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn env_valid_dir_canonicalizes() {
        let temp_home = TempDir::new().expect("temp home");
        let temp_str = temp_home
            .path()
            .to_str()
            .expect("temp uclaw home path should be valid utf-8");

        let resolved = uclaw_home_from_env(Some(temp_str)).expect("valid UCLAW_HOME");
        let expected = temp_home
            .path()
            .canonicalize()
            .expect("canonicalize temp home");
        let expected = AbsolutePathBuf::from_absolute_path(expected).expect("absolute home");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn no_env_uses_dot_uclaw_pi_under_home() {
        let resolved = uclaw_home_from_env(/*env*/ None).expect("default UCLAW_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(".uclaw-pi");
        let expected = AbsolutePathBuf::from_absolute_path(expected).expect("absolute home");
        assert_eq!(resolved, expected);
    }
}
