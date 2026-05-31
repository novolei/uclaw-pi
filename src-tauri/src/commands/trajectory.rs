//! Trajectory-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), this domain has **no service**:
//! both commands delegate to the in-memory
//! [`crate::agent::trajectory::TrajectoryStore`] held in
//! `state.trajectory_store` (an `Arc<TrajectoryStore>`). That store *is* the
//! logic holder — it owns turn recording and search in memory, with **no
//! `state.db` SQL to lift** — so the JUDGMENT RULE resolves to a thin move.
//!
//! Relocated verbatim from the legacy `tauri_commands.rs` god file.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;

#[tauri::command]
pub async fn get_session_trajectory(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<crate::agent::trajectory::TurnRecord>, Error> {
    Ok(state.trajectory_store.get_session_turns(&session_id))
}

#[tauri::command]
pub async fn search_trajectories(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<crate::agent::trajectory::TrajectorySearchHit>, Error> {
    Ok(state.trajectory_store.search(&query, limit.unwrap_or(20)))
}
