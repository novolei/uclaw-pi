//! Cost-query-domain Tauri commands — thin wrappers over the cost-rollup
//! queries in [`crate::services::cost_service`] (the `CostQueryService` trait on
//! [`crate::services::cost_service::PricingCostService`]). No SQL here: lock
//! `state.db`, call the service, return.
//!
//! These power the cost dashboard + monthly-budget views. Relocated from the
//! legacy `tauri_commands.rs` god file (where they were mis-filed under the
//! "Conversation Commands" section) per the code-organization ADR (2026-05-31).

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{DailyCostRollup, ModelCostRollup, SessionCostRollup, WorkspaceCostRollup};
use crate::services::cost_service::{CostQueryService, PricingCostService};

/// Per-UTC-day cost rollup over the last `days_back` days (default 30).
#[tauri::command]
pub async fn get_daily_costs(
    state: State<'_, AppState>,
    days_back: Option<u32>,
) -> Result<Vec<DailyCostRollup>, Error> {
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    PricingCostService.daily(&conn, days_back)
}

/// Per-model cost rollup over the last `days_back` days (costliest first).
#[tauri::command]
pub async fn get_model_costs(
    state: State<'_, AppState>,
    days_back: Option<u32>,
) -> Result<Vec<ModelCostRollup>, Error> {
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    PricingCostService.by_model(&conn, days_back)
}

/// Per-session cost rollup over the last `days_back` days (default limit 50).
#[tauri::command]
pub async fn get_session_costs(
    state: State<'_, AppState>,
    days_back: Option<u32>,
    limit: Option<u32>,
) -> Result<Vec<SessionCostRollup>, Error> {
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    PricingCostService.by_session(&conn, days_back, limit)
}

/// Per-workspace cost rollup of records since `since_ms` (current-month start,
/// computed frontend-side in user-local time).
#[tauri::command]
pub async fn list_workspace_cost_rollup(
    state: State<'_, AppState>,
    since_ms: i64,
) -> Result<Vec<WorkspaceCostRollup>, Error> {
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    PricingCostService.by_workspace(&conn, since_ms)
}

/// Total USD across all cost records since `since_ms`.
#[tauri::command]
pub async fn get_month_cost_total(
    state: State<'_, AppState>,
    since_ms: i64,
) -> Result<f64, Error> {
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    PricingCostService.month_total(&conn, since_ms)
}
