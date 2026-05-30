/// 可观测性模块
/// 提供指标采集（计数器、直方图）和结构化追踪能力

mod metrics;
mod trace;
pub mod shutdown;

pub use metrics::*;
pub use trace::*;

// ---------------------------------------------------------------------------
// Tracing bootstrap: stdout + daily-rotated file logs
// ---------------------------------------------------------------------------
//
// Logs go to `~/.uclaw/logs/uclaw.log.YYYY-MM-DD`. The non-blocking
// writer's `WorkerGuard` MUST be held until process exit or pending
// lines are dropped — `init()` returns it; `main()` binds it to a
// `let _guard = ...;` and lets it drop at shutdown.

use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Registry};

/// Resolve the log directory, creating it if needed.
fn log_dir() -> PathBuf {
    let base = home_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join(".uclaw-pi").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Best-effort home directory lookup. Falls back to current dir if HOME
/// isn't set (unusual on macOS but defensive).
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // chromiumoxide::handler emits WARN per CDP event Chrome sends
        // outside its schema — noise. Silence it. Match the prior main.rs default.
        EnvFilter::new("info,chromiumoxide::handler=error")
    })
}

/// Initialize tracing for the whole process.
///
/// Returns a `WorkerGuard` whose Drop flushes any pending non-blocking
/// file writes. Hold it in `main` for the lifetime of the process.
pub fn init() -> WorkerGuard {
    let dir = log_dir();
    let file_appender = tracing_appender::rolling::daily(&dir, "uclaw.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let stdout_layer = fmt::layer().with_writer(std::io::stdout);
    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false); // no escape codes in the on-disk log

    Registry::default()
        .with(env_filter())
        .with(stdout_layer)
        .with(file_layer)
        .init();

    tracing::info!(?dir, "tracing initialized");

    guard
}

/// Install a process-wide panic hook that:
/// - Writes a crash record under `~/.uclaw/logs/crashes/crash-<timestamp>-<thread>.log`
///   containing the panic message + location + a full backtrace.
/// - Emits an `error!` event so the same info lands in the main rolling log.
///
/// Called once from main, after `init()`.
pub fn install_panic_hook() {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let crash_dir = log_dir().join("crashes");
        let _ = std::fs::create_dir_all(&crash_dir);

        let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unnamed").to_string();
        let safe_thread = thread_name.replace(
            |c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_',
            "_",
        );

        let path = crash_dir.join(format!("crash-{}-{}.log", ts, safe_thread));
        let bt = std::backtrace::Backtrace::force_capture();
        let body = format!(
            "panic at {}\n\nThread: {}\nLocation: {:?}\n\n=== Backtrace ===\n{}\n",
            info,
            thread_name,
            info.location(),
            bt,
        );

        let _ = std::fs::write(&path, &body);

        // Also surface to tracing so the rolling log captures it.
        tracing::error!(
            crash_log = %path.display(),
            thread = %thread_name,
            "panic captured: {}",
            info,
        );

        // Chain to the previous hook so default behavior (printing to
        // stderr) is preserved. Helps dev-mode visibility.
        prev_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke: install the hook and trigger a panic in a spawned thread,
    /// verify a crash log file was written. The hook is global state so
    /// we save/restore it around the test.
    #[test]
    fn panic_hook_writes_crash_log() {
        let prev = std::panic::take_hook();
        install_panic_hook();

        let crash_dir = log_dir().join("crashes");
        let before_count = std::fs::read_dir(&crash_dir)
            .ok()
            .map(|d| d.count())
            .unwrap_or(0);

        let handle = std::thread::Builder::new()
            .name("panic-test-thread".to_string())
            .spawn(|| {
                panic!("test panic — please ignore in CI output");
            })
            .unwrap();
        let _ = handle.join();

        // Give the filesystem a moment.
        std::thread::sleep(std::time::Duration::from_millis(100));

        let after_count = std::fs::read_dir(&crash_dir)
            .ok()
            .map(|d| d.count())
            .unwrap_or(0);

        // Restore prior hook for the rest of the test suite.
        std::panic::set_hook(prev);

        assert!(
            after_count > before_count,
            "expected new crash log; before={}, after={}",
            before_count,
            after_count
        );
    }
}
