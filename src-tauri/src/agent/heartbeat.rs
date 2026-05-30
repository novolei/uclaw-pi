//! Bundle 27-A — Agent loop heartbeat + flight recorder + reply recovery.
//!
//! Three responsibilities, all in one type so they share the
//! `last_activity_at` clock and `partial_text` buffer:
//!
//! 1. **Heartbeat** — every `tick_interval` (default 5s) emit
//!    `agent:heartbeat` with `{conversation_id, iteration, stage,
//!    last_activity_ms_ago, partial_chars}`. The UI shows a live
//!    indicator so the user knows whether the agent is actually doing
//!    work or just spinning.
//!
//! 2. **Stall detection** — if `now - last_activity_at > stall_after`
//!    (default 30s), emit `agent:stalled` exactly once per stall
//!    period with the diagnostic context. The UI surfaces a banner
//!    with [中断并保存] and [继续等待] buttons. This is the user's
//!    forensic record of "卡死那一瞬间的位置 — 真正定位根因".
//!
//! 3. **Flight recorder** — every tick, atomic-rewrite
//!    `~/.uclaw/state/last_active_run.json` with the current
//!    in-flight state (conversation, iteration, stage, partial_text).
//!    On clean exit this is deleted; on SIGKILL/OOM it survives, so
//!    Bundle 27-C's unclean-shutdown detection can hand the
//!    next-boot recovery layer enough context to write a
//!    `[interrupted-recovered]` assistant message into the session
//!    that died — the user's "Agent 的回复补齐" requirement.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Default cadence of `agent:heartbeat` emissions.
const DEFAULT_TICK_INTERVAL: Duration = Duration::from_secs(5);
/// Default "no activity for this long → stalled" threshold.
const DEFAULT_STALL_AFTER: Duration = Duration::from_secs(30);
/// Cap on the in-memory partial buffer (chars). Streaming responses
/// rarely exceed a few KB before they finish; this is a safety valve
/// for runaway models. Hit the cap → we keep the head + tail so the
/// recovery message still makes sense.
const PARTIAL_BUFFER_CAP_CHARS: usize = 64 * 1024;

/// Stage labels the dispatcher uses to identify which sub-step is
/// active when the agent stalls. Free-form on purpose — adding a new
/// stage doesn't require touching this module.
pub mod stages {
    pub const STARTING: &str = "starting";
    pub const LLM_CALL: &str = "llm_call";
    pub const LLM_STREAM: &str = "llm_stream";
    pub const TOOL_CALL: &str = "tool_call";
    pub const THINKING: &str = "thinking";
    pub const DONE: &str = "done";
}

/// On-disk shape of `last_active_run.json` — read on next boot by the
/// recovery layer when Bundle 27-C reports Unclean.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlightRecord {
    pub schema_version: u32,
    pub conversation_id: String,
    pub space_id: String,
    pub iteration: u32,
    /// One of the `stages::*` constants.
    pub stage: String,
    /// Unix ms — boot of this agent run.
    pub started_at: i64,
    /// Unix ms — last `mark_activity` call.
    pub last_activity_at: i64,
    /// Streamed-but-unflushed text. Empty when iteration hasn't
    /// produced any assistant text yet.
    pub partial_text: String,
    /// Process pid that wrote this record — lets the recovery layer
    /// cross-check against Bundle 27-C's process.lock.
    pub pid: u32,
}

/// Live heartbeat supervisor for an in-flight agent run. Dropped /
/// `.shutdown()` clears the flight recorder.
pub struct HeartbeatSupervisor {
    app_handle: tauri::AppHandle,
    conversation_id: String,
    space_id: String,
    flight_path: PathBuf,
    state: Arc<HeartbeatState>,
    cancel: CancellationToken,
    /// Cadence — only set at construction.
    tick_interval: Duration,
    stall_after: Duration,
}

/// Interior mutable state shared between the supervisor and its
/// ticker task. Uses atomics where possible so the dispatcher's
/// hot path never blocks on a mutex.
struct HeartbeatState {
    iteration: AtomicU32,
    /// Unix ms — wall-clock of last activity.
    last_activity_at: AtomicI64,
    /// Unix ms — when the agent run started (immutable after start).
    started_at: AtomicI64,
    /// Latest stage label. `Mutex<String>` (not lock-free) because
    /// stage transitions are rare (≤ a few per second) and a
    /// short-lived sync Mutex is simpler than an arc-swap or RwLock.
    stage: std::sync::Mutex<String>,
    /// Streamed assistant text not yet flushed via stream-complete.
    /// Used for both flight-recorder persistence and post-mortem
    /// recovery via `take_partial`.
    partial: Mutex<String>,
    /// One-shot guard so we only emit `agent:stalled` once per stall
    /// period — flips back to false on the next mark_activity.
    stalled_emitted: std::sync::atomic::AtomicBool,
    /// Bundle 27-A2 (3rd pass) — last time we persisted the flight
    /// file from `append_partial`. Used to throttle per-chunk writes
    /// to once per 200ms while keeping the 5s ticker for stage/event
    /// emission. Closes the "kill 4.9s into streaming → flight has 0
    /// partial chars" window from earlier testing.
    last_flight_persist_at: AtomicI64,
}

impl HeartbeatSupervisor {
    /// Create + spawn the ticker. Caller MUST hold the returned
    /// `Arc<Self>` for the lifetime of the agent run; drop it (or
    /// call `.shutdown()`) when the run completes/aborts.
    pub fn new(
        app_handle: tauri::AppHandle,
        conversation_id: String,
        space_id: String,
        flight_path: PathBuf,
    ) -> Arc<Self> {
        Self::with_tuning(
            app_handle,
            conversation_id,
            space_id,
            flight_path,
            DEFAULT_TICK_INTERVAL,
            DEFAULT_STALL_AFTER,
        )
    }

    pub fn with_tuning(
        app_handle: tauri::AppHandle,
        conversation_id: String,
        space_id: String,
        flight_path: PathBuf,
        tick_interval: Duration,
        stall_after: Duration,
    ) -> Arc<Self> {
        let now = now_ms();
        let state = Arc::new(HeartbeatState {
            iteration: AtomicU32::new(0),
            last_activity_at: AtomicI64::new(now),
            started_at: AtomicI64::new(now),
            stage: std::sync::Mutex::new(stages::STARTING.to_string()),
            partial: Mutex::new(String::new()),
            stalled_emitted: std::sync::atomic::AtomicBool::new(false),
            last_flight_persist_at: AtomicI64::new(0),
        });
        let cancel = CancellationToken::new();
        let sup = Arc::new(Self {
            app_handle: app_handle.clone(),
            conversation_id: conversation_id.clone(),
            space_id: space_id.clone(),
            flight_path: flight_path.clone(),
            state: state.clone(),
            cancel: cancel.clone(),
            tick_interval,
            stall_after,
        });

        // Spawn ticker — owns weak ref to supervisor's state, cancels
        // via the shared CancellationToken.
        {
            let app_handle = app_handle;
            let conversation_id = conversation_id.clone();
            let space_id = space_id.clone();
            let flight_path = flight_path.clone();
            let stall_after_ms = stall_after.as_millis() as i64;
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(tick_interval);
                ticker.set_missed_tick_behavior(
                    tokio::time::MissedTickBehavior::Delay,
                );
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = ticker.tick() => {
                            let now = now_ms();
                            let last = state.last_activity_at.load(Ordering::Relaxed);
                            let ago = (now - last).max(0);
                            let iteration = state.iteration.load(Ordering::Relaxed);
                            let stage = state
                                .stage
                                .lock()
                                .map(|s| s.clone())
                                .unwrap_or_else(|_| "unknown".to_string());
                            let partial_chars = state
                                .partial
                                .lock()
                                .await
                                .chars()
                                .count();

                            // 1. Emit heartbeat.
                            let _ = app_handle.emit("agent:heartbeat", serde_json::json!({
                                "conversationId": conversation_id,
                                "iteration": iteration,
                                "stage": stage,
                                "lastActivityMsAgo": ago,
                                "partialChars": partial_chars,
                                "timestamp": now,
                            }));

                            // 2. Stall detection — fire ONCE per
                            // stall period; subsequent ticks during
                            // the same stall just keep updating
                            // heartbeat. mark_activity resets the
                            // one-shot.
                            if ago >= stall_after_ms {
                                let was_already = state
                                    .stalled_emitted
                                    .swap(true, Ordering::SeqCst);
                                if !was_already {
                                    tracing::warn!(
                                        conversation_id = %conversation_id,
                                        iteration = iteration,
                                        stage = %stage,
                                        stall_for_ms = ago,
                                        "[Bundle 27-A] agent stalled — no activity for {}ms (stage={})",
                                        ago, stage,
                                    );
                                    let _ = app_handle.emit("agent:stalled", serde_json::json!({
                                        "conversationId": conversation_id,
                                        "iteration": iteration,
                                        "stage": stage,
                                        "stalledForMs": ago,
                                        "partialChars": partial_chars,
                                        "timestamp": now,
                                    }));
                                }
                            }

                            // 3. Flight recorder.
                            let partial = state.partial.lock().await.clone();
                            let record = FlightRecord {
                                schema_version: 1,
                                conversation_id: conversation_id.clone(),
                                space_id: space_id.clone(),
                                iteration,
                                stage,
                                started_at: state.started_at.load(Ordering::Relaxed),
                                last_activity_at: last,
                                partial_text: partial,
                                pid: std::process::id(),
                            };
                            if let Err(e) = write_flight_atomic(&flight_path, &record) {
                                // Best-effort — log and continue.
                                tracing::debug!(
                                    path = %flight_path.display(),
                                    error = %e,
                                    "[heartbeat] flight recorder write failed",
                                );
                            }
                        }
                    }
                }
                // Clean shutdown: remove the flight record so next
                // boot doesn't see it as in-flight.
                let _ = std::fs::remove_file(&flight_path);
            });
        }

        sup
    }

    /// Mark activity at a named stage. Resets the stall-emitted
    /// one-shot. Called at every step boundary in the dispatcher.
    pub fn mark_activity(&self, stage: &str) {
        self.state
            .last_activity_at
            .store(now_ms(), Ordering::Relaxed);
        if let Ok(mut s) = self.state.stage.lock() {
            if s.as_str() != stage {
                *s = stage.to_string();
            }
        }
        // If we were stalled, fire a recovery event so the UI knows.
        let was = self
            .state
            .stalled_emitted
            .swap(false, Ordering::SeqCst);
        if was {
            tracing::info!(
                conversation_id = %self.conversation_id,
                stage = %stage,
                "[Bundle 27-A] stall recovered — activity resumed"
            );
            let _ = self
                .app_handle
                .emit("agent:stall-recovered", serde_json::json!({
                    "conversationId": self.conversation_id,
                    "stage": stage,
                    "timestamp": now_ms(),
                }));
        }
    }

    /// Set the iteration counter — called from the agent loop.
    pub fn set_iteration(&self, iter: u32) {
        self.state.iteration.store(iter, Ordering::Relaxed);
    }

    /// Append a streamed text chunk to the in-memory partial buffer.
    /// Caps at `PARTIAL_BUFFER_CAP_CHARS` (truncating middle) so a
    /// runaway model can't OOM us.
    ///
    /// Bundle 27-A2 (3rd pass) — also writes the flight record to
    /// disk if last persist was > 200ms ago. The 5s ticker is fine
    /// for stage updates + UI heartbeat, but for the
    /// "kill -9 during streaming" recovery story we need the flight
    /// file to be at most ~200ms behind reality. Throttled to avoid
    /// fsync on every token.
    pub async fn append_partial(&self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }
        // Phase 1 — update the buffer.
        let snapshot_after_append: String;
        {
            let mut buf = self.state.partial.lock().await;
            buf.push_str(chunk);
            let len = buf.chars().count();
            if len > PARTIAL_BUFFER_CAP_CHARS {
                // Keep head 32K + tail 32K, drop the middle.
                let chars: Vec<char> = buf.chars().collect();
                let keep = PARTIAL_BUFFER_CAP_CHARS / 2;
                let head: String = chars.iter().take(keep).collect();
                let tail: String = chars.iter().rev().take(keep).rev().collect();
                *buf = format!(
                    "{head}\n\n[…{} chars truncated…]\n\n{tail}",
                    len - PARTIAL_BUFFER_CAP_CHARS
                );
            }
            snapshot_after_append = buf.clone();
        }

        // Phase 2 — throttled persist (200ms gate). The flight file
        // write itself is atomic (tempfile + fsync + rename).
        let now = now_ms();
        let last_persist = self.state.last_flight_persist_at.load(Ordering::Relaxed);
        if now - last_persist < 200 {
            return;
        }
        // Race-safe enough: another append_partial may slip through but
        // they all write the same path; last-writer-wins is acceptable
        // because each write contains the full current buffer.
        self.state
            .last_flight_persist_at
            .store(now, Ordering::Relaxed);

        let iteration = self.state.iteration.load(Ordering::Relaxed);
        let stage = self
            .state
            .stage
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|_| "unknown".to_string());
        let started_at = self.state.started_at.load(Ordering::Relaxed);
        let last_activity_at = self.state.last_activity_at.load(Ordering::Relaxed);
        let record = FlightRecord {
            schema_version: 1,
            conversation_id: self.conversation_id.clone(),
            space_id: self.space_id.clone(),
            iteration,
            stage,
            started_at,
            last_activity_at,
            partial_text: snapshot_after_append,
            pid: std::process::id(),
        };
        if let Err(e) = write_flight_atomic(&self.flight_path, &record) {
            tracing::debug!(
                error = %e,
                "[heartbeat] inline flight persist (append_partial) failed"
            );
        }
    }

    /// Drain the partial buffer — caller takes ownership of the
    /// accumulated text. Used by both the normal stream-complete
    /// path (which clears the buffer once the final text is
    /// persisted) and the manual-interrupt path.
    pub async fn take_partial(&self) -> String {
        let mut buf = self.state.partial.lock().await;
        std::mem::take(&mut *buf)
    }

    /// Peek without draining — for diagnostics.
    pub async fn peek_partial(&self) -> String {
        self.state.partial.lock().await.clone()
    }

    /// Cancel the ticker and remove the flight record. Call from the
    /// agent loop's completion path (success or controlled failure).
    /// Idempotent.
    pub fn shutdown(&self) {
        self.cancel.cancel();
        let _ = std::fs::remove_file(&self.flight_path);
    }
}

impl Drop for HeartbeatSupervisor {
    fn drop(&mut self) {
        // Defensive: even if a caller forgot to call shutdown,
        // dropping the Arc<Self> should still tear down the ticker
        // and clean the flight record.
        self.cancel.cancel();
        let _ = std::fs::remove_file(&self.flight_path);
    }
}

// ────────────────────────────────────────────────────────────────────────
// Flight-record persistence + recovery API (called from main.rs at
// boot when Bundle 27-C reports Unclean shutdown)
// ────────────────────────────────────────────────────────────────────────

/// Default path: `~/.uclaw-pi/state/last_active_run.json`.
pub fn default_flight_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".uclaw-pi").join("state").join("last_active_run.json")
}

/// Atomic write — tempfile in same dir → fsync → rename.
fn write_flight_atomic(path: &Path, record: &FlightRecord) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        let body = serde_json::to_string_pretty(record).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("serialize FlightRecord: {}", e),
            )
        })?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Read+parse a flight record. `Ok(None)` if missing.
pub fn read_flight(path: &Path) -> std::io::Result<Option<FlightRecord>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)?;
    let rec: FlightRecord = serde_json::from_str(&raw).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("parse FlightRecord at {}: {}", path.display(), e),
        )
    })?;
    Ok(Some(rec))
}

/// Remove the flight record (called from boot once recovery has
/// consumed it, or from CleanExitGuard sibling path).
pub fn clear_flight(path: &Path) {
    let _ = std::fs::remove_file(path);
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_record() -> FlightRecord {
        FlightRecord {
            schema_version: 1,
            conversation_id: "conv-test".into(),
            space_id: "default".into(),
            iteration: 7,
            stage: stages::LLM_STREAM.into(),
            started_at: 1_700_000_000_000,
            last_activity_at: 1_700_000_005_000,
            partial_text: "hello partial".into(),
            pid: 12345,
        }
    }

    #[test]
    fn flight_record_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("last_active_run.json");
        let rec = sample_record();
        write_flight_atomic(&path, &rec).unwrap();
        let loaded = read_flight(&path).unwrap().unwrap();
        assert_eq!(loaded, rec);
    }

    #[test]
    fn read_flight_returns_none_when_missing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("never_existed.json");
        assert!(read_flight(&path).unwrap().is_none());
    }

    #[test]
    fn clear_flight_idempotent() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("last_active_run.json");
        write_flight_atomic(&path, &sample_record()).unwrap();
        clear_flight(&path);
        assert!(!path.exists());
        // second clear — fine
        clear_flight(&path);
    }

    #[test]
    fn read_corrupt_returns_invalid_data() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("last_active_run.json");
        std::fs::write(&path, "not json").unwrap();
        let err = read_flight(&path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn write_atomic_no_tmp_left_behind() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("last_active_run.json");
        write_flight_atomic(&path, &sample_record()).unwrap();
        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
    }
}
