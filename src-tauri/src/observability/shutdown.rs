//! Bundle 27-C — Unclean-shutdown detection.
//!
//! `install_panic_hook` (in mod.rs) covers the case where a Rust panic
//! brings the process down. But the most common ways a Tauri agent
//! actually disappears mid-run are NOT panics:
//!
//! - **macOS launchd / Activity Monitor force-quit** → SIGKILL, no
//!   chance to run any cleanup.
//! - **macOS jetsam memory-pressure killer** → SIGKILL.
//! - **System reboot / shutdown** → SIGTERM that the app didn't catch
//!   in time.
//! - **Tauri main thread blocked > N seconds** → "App not responding"
//!   force-quit by the OS or the user.
//!
//! None of these trigger the panic hook. Without telemetry we just see
//! "process restarted" with no record of how it died.
//!
//! Bundle 27-C uses a **process lock file** to detect this:
//!
//! 1. On boot, `install` writes `~/.uclaw/state/process.lock` containing
//!    the current PID + start time. If the file ALREADY exists and the
//!    PID inside it is dead, the previous instance died unclean — we
//!    log a structured WARN event with the dead PID, lock age, and
//!    pick up Bundle 27-A's `last_active_run.json` (if present) to
//!    surface what conversation was in flight.
//! 2. `mark_clean_exit` is called from main's normal exit path — it
//!    deletes the lock so the next boot won't false-positive.
//! 3. If we boot and find a stale lock with a STILL-LIVING PID (very
//!    rare — another uClaw instance), we abort start with a clear
//!    message rather than fight for the same resources.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Persistent shape of `process.lock`. Versioned so we can extend
/// without breaking forward-recovery from older builds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessLock {
    pub schema_version: u32,
    /// PID of the uClaw process that wrote this lock.
    pub pid: u32,
    /// Unix ms — when the process booted.
    pub started_at: i64,
    /// `CARGO_PKG_VERSION` at boot time. Useful to spot
    /// "downgrade-then-crash" patterns.
    pub version: String,
}

/// Result of `detect_previous_shutdown` for use by callers (main.rs +
/// session-recovery layer).
#[derive(Debug, Clone, PartialEq)]
pub enum PreviousShutdown {
    /// No prior lock file — first run on this machine, or last run
    /// exited cleanly.
    Clean,
    /// Prior lock file existed, its PID is dead → previous instance
    /// died unclean. Carries the parsed lock so callers can surface
    /// the dead PID, started_at, and version.
    Unclean(ProcessLock),
    /// Prior lock file exists AND its PID is STILL ALIVE. Either a
    /// stale lock from a frozen instance, or a real concurrent run.
    /// Caller should refuse to start.
    AnotherInstanceAlive(ProcessLock),
}

/// Default location of the process lock — `~/.uclaw-pi/state/process.lock`.
pub fn default_lock_path() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(".uclaw-pi").join("state").join("process.lock")
}

/// Atomic write of the lock file: tempfile in same dir → fsync → rename.
fn write_lock_atomic(path: &Path, lock: &ProcessLock) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("lock.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        let body = serde_json::to_string_pretty(lock).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("serialize ProcessLock: {}", e),
            )
        })?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Read+parse a lock file. `Ok(None)` if not present; `Err` only for
/// permission/IO/parse errors that callers should log.
pub fn read_lock(path: &Path) -> std::io::Result<Option<ProcessLock>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    let lock: ProcessLock = serde_json::from_str(&raw).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("parse ProcessLock at {}: {}", path.display(), e),
        )
    })?;
    Ok(Some(lock))
}

/// Check whether `pid` is currently running on this system.
///
/// On Unix: `kill(pid, 0)` returns 0 if the process exists (and we have
/// permission to signal it). ESRCH means no such process. EPERM means
/// process exists but we lack permission — treat as alive (the process
/// is real; we just can't signal it).
#[cfg(unix)]
pub fn is_pid_alive(pid: u32) -> bool {
    use std::os::raw::c_int;
    // Convert u32 to pid_t (i32 on Unix). PIDs > i32::MAX don't exist.
    let pid_i: c_int = match c_int::try_from(pid) {
        Ok(n) if n > 0 => n,
        _ => return false,
    };
    // SAFETY: kill(2) with sig=0 is a pure existence check; no side
    // effects on the target process. No memory is touched.
    let ret = unsafe { libc::kill(pid_i, 0) };
    if ret == 0 {
        return true;
    }
    // Distinguish "doesn't exist" from "exists but no permission".
    let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
    // libc::ESRCH = no such process. libc::EPERM = exists but denied.
    errno == libc::EPERM
}

#[cfg(not(unix))]
pub fn is_pid_alive(_pid: u32) -> bool {
    // Conservative on non-Unix: assume the prior PID is dead so we
    // proceed with boot. uClaw is mac-first so this branch is mostly
    // theoretical.
    false
}

/// Classify the previous shutdown state by inspecting `path`.
pub fn detect_previous_shutdown(path: &Path) -> std::io::Result<PreviousShutdown> {
    match read_lock(path)? {
        None => Ok(PreviousShutdown::Clean),
        Some(prev) => {
            if is_pid_alive(prev.pid) && prev.pid != std::process::id() {
                Ok(PreviousShutdown::AnotherInstanceAlive(prev))
            } else {
                Ok(PreviousShutdown::Unclean(prev))
            }
        }
    }
}

/// Install a fresh lock for this process. Returns the path so the
/// caller can pass it to `mark_clean_exit` on shutdown.
///
/// Caller is responsible for first calling `detect_previous_shutdown`
/// and acting on the result (log + last-message recovery for
/// `Unclean`, abort startup for `AnotherInstanceAlive`).
pub fn install(path: &Path) -> std::io::Result<ProcessLock> {
    let lock = ProcessLock {
        schema_version: 1,
        pid: std::process::id(),
        started_at: chrono::Utc::now().timestamp_millis(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    write_lock_atomic(path, &lock)?;
    tracing::info!(
        pid = lock.pid,
        version = %lock.version,
        path = %path.display(),
        "[Bundle 27-C] process lock installed"
    );
    Ok(lock)
}

/// Delete the lock — call from the normal shutdown path so the next
/// boot sees `Clean`. Idempotent: missing file is not an error.
pub fn mark_clean_exit(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {
            tracing::info!(
                path = %path.display(),
                "[Bundle 27-C] process lock removed (clean exit)"
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Already gone; nothing to clean.
        }
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "[Bundle 27-C] failed to remove process lock on exit"
            );
        }
    }
}

/// RAII guard — calls `mark_clean_exit` on Drop. Bind it in `main()`
/// with an `_` prefix so it lives until process exit. If main panics
/// or the runtime tears down, Drop still runs (unwinding) — so the
/// lock gets cleaned up except in true SIGKILL / abort scenarios,
/// which is exactly the case we WANT to detect on next boot.
pub struct CleanExitGuard {
    path: PathBuf,
}

impl CleanExitGuard {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for CleanExitGuard {
    fn drop(&mut self) {
        mark_clean_exit(&self.path);
    }
}

/// Convenience: do the full boot-time check + logging + install in one
/// call. Returns the detected previous state (already logged) so
/// callers can act on `Unclean` for session recovery.
pub fn check_and_install(path: &Path) -> std::io::Result<PreviousShutdown> {
    let prev = detect_previous_shutdown(path)?;
    match &prev {
        PreviousShutdown::Clean => {
            tracing::info!(
                "[Bundle 27-C] previous shutdown: clean (no leftover lock)"
            );
        }
        PreviousShutdown::Unclean(p) => {
            let age_ms = chrono::Utc::now().timestamp_millis() - p.started_at;
            tracing::warn!(
                dead_pid = p.pid,
                started_at = p.started_at,
                age_ms = age_ms,
                version = %p.version,
                "[Bundle 27-C] previous shutdown: UNCLEAN — process \
                 died without removing its lock (SIGKILL / OOM / panic)"
            );
        }
        PreviousShutdown::AnotherInstanceAlive(p) => {
            tracing::error!(
                live_pid = p.pid,
                started_at = p.started_at,
                "[Bundle 27-C] previous shutdown: ANOTHER INSTANCE ALIVE \
                 — refusing to start a concurrent uClaw"
            );
            return Ok(prev);
        }
    }
    let _ = install(path)?;
    Ok(prev)
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn no_prior_lock_returns_clean() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("process.lock");
        match detect_previous_shutdown(&path).unwrap() {
            PreviousShutdown::Clean => {}
            other => panic!("expected Clean, got {other:?}"),
        }
    }

    #[test]
    fn dead_pid_lock_returns_unclean() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("process.lock");
        // PID 0 is never a real process on Unix; using a deliberately
        // high PID would also work but PID 0 is the standard sentinel.
        let prev = ProcessLock {
            schema_version: 1,
            pid: 999_999_999, // implausibly high; should not exist
            started_at: 1_700_000_000_000,
            version: "test".into(),
        };
        write_lock_atomic(&path, &prev).unwrap();
        match detect_previous_shutdown(&path).unwrap() {
            PreviousShutdown::Unclean(p) => assert_eq!(p.pid, 999_999_999),
            other => panic!("expected Unclean, got {other:?}"),
        }
    }

    #[test]
    fn live_pid_lock_returns_another_instance() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("process.lock");
        // Spawn a real child process and use its PID — guaranteed alive
        // during the assert and not our own PID.
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("spawn sleep");
        let lock = ProcessLock {
            schema_version: 1,
            pid: child.id(),
            started_at: 1_700_000_000_000,
            version: "test".into(),
        };
        write_lock_atomic(&path, &lock).unwrap();
        let detected = detect_previous_shutdown(&path).unwrap();
        let _ = child.kill();
        let _ = child.wait();
        match detected {
            PreviousShutdown::AnotherInstanceAlive(p) => assert_eq!(p.pid, lock.pid),
            other => panic!("expected AnotherInstanceAlive, got {other:?}"),
        }
    }

    #[test]
    fn install_writes_current_pid() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("process.lock");
        let lock = install(&path).unwrap();
        assert_eq!(lock.pid, std::process::id());
        let on_disk = read_lock(&path).unwrap().unwrap();
        assert_eq!(on_disk, lock);
    }

    #[test]
    fn mark_clean_exit_removes_lock_idempotent() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("process.lock");
        install(&path).unwrap();
        assert!(path.exists());
        mark_clean_exit(&path);
        assert!(!path.exists());
        // idempotent — second call is fine
        mark_clean_exit(&path);
    }

    #[test]
    fn check_and_install_round_trip_clean_to_unclean() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("process.lock");
        // First boot — Clean.
        match check_and_install(&path).unwrap() {
            PreviousShutdown::Clean => {}
            other => panic!("first call: expected Clean, got {other:?}"),
        }
        // Simulate process death: don't call mark_clean_exit, instead
        // overwrite the lock with a dead PID (own current PID would
        // false-positive as Alive).
        let dead = ProcessLock {
            schema_version: 1,
            pid: 999_999_999,
            started_at: 1_700_000_000_000,
            version: "dead".into(),
        };
        write_lock_atomic(&path, &dead).unwrap();
        // Next boot — Unclean.
        match check_and_install(&path).unwrap() {
            PreviousShutdown::Unclean(p) => assert_eq!(p.pid, 999_999_999),
            other => panic!("second call: expected Unclean, got {other:?}"),
        }
        // Should have re-installed for this process.
        let fresh = read_lock(&path).unwrap().unwrap();
        assert_eq!(fresh.pid, std::process::id());
    }

    #[test]
    fn read_lock_corrupt_returns_err() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("process.lock");
        fs::write(&path, "{ not valid json").unwrap();
        let err = read_lock(&path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
