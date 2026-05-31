//! Write-side persistence for per-turn LLM cost records.
//!
//! Called from the agent dispatcher's `emit_turn_cost` to capture usage
//! synchronously alongside the IPC event. No frontend dependency — events
//! are fire-and-forget from the listener's POV; persistence is the source
//! of truth for the dashboard.

use crate::app::AppState;
use crate::agent::types::calculate_cost;
use rusqlite::params;

/// Insert one cost record. Errors are logged and swallowed — cost capture
/// is best-effort and must never fail the agent loop.
pub fn record(state: &AppState, session_id: &str, model: &str, input_tokens: u32, output_tokens: u32) {
    let cost = calculate_cost(model, input_tokens, output_tokens);
    record_cost(state, session_id, model, input_tokens, output_tokens, cost);
}

/// Like [`record`] but with a pre-computed `cost` (the pi path computes a
/// cache-aware cost itself, so this avoids the non-cache [`calculate_cost`]).
pub fn record_cost(
    state: &AppState,
    session_id: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cost: f64,
) {
    let now = chrono::Utc::now().timestamp_millis();
    let id = uuid::Uuid::new_v4().to_string();
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("cost_store: DB lock failed: {}", e);
            return;
        }
    };
    if let Err(e) = conn.execute(
        "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, session_id, model, input_tokens as i64, output_tokens as i64, cost, now],
    ) {
        tracing::warn!("cost_store: INSERT failed: {}", e);
    }
}

/// SUM(cost_usd) for cost_records with created_at >= since_ms.
/// Returns 0.0 on any error (best-effort, matches the rest of this module).
pub fn monthly_total(state: &AppState, since_ms: i64) -> f64 {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(_) => return 0.0,
    };
    conn.query_row(
        "SELECT COALESCE(SUM(cost_usd), 0) FROM cost_records WHERE created_at >= ?1",
        params![since_ms],
        |row| row.get::<_, f64>(0),
    ).unwrap_or(0.0)
}

/// Compute the start of the current month (UTC) in epoch ms.
pub fn current_month_start_ms() -> i64 {
    use chrono::{Datelike, TimeZone, Utc};
    let now = Utc::now();
    Utc.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

/// Compute the start of the current day (UTC) in epoch ms.
/// Used by per-day spend caps (Sprint 2.1b learning, Phase 5 lint).
pub fn current_day_start_ms() -> i64 {
    use chrono::{Datelike, TimeZone, Utc};
    let now = Utc::now();
    Utc.with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

/// Memory OS Sprint 2.1b — SUM(input_tokens + output_tokens) for today's
/// cost_records where `model LIKE 'memory_learning%'`. Used by the chat-turn
/// extractor's daily-budget gate before invoking the LLM layer. Returns 0
/// on any error (best-effort; falling back to 0 lets the producer continue
/// running rather than blocking on transient DB issues).
///
/// Standalone function (not on AppState) so the agent dispatcher can call
/// it given only the raw `Arc<Mutex<Connection>>` — `set_learning_pipeline`
/// hands it the same db handle AppState owns.
pub fn today_learning_tokens(
    db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> u32 {
    let since_ms = current_day_start_ms();
    let conn = match db.lock() {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let n: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0)
             FROM cost_records
             WHERE model LIKE 'memory_learning%' AND created_at >= ?1",
            params![since_ms],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    n.max(0) as u32
}

/// Sprint 2.4b — sibling of [`today_learning_tokens`] for the gbrain
/// chat-turn extractor's daily-budget gate. Sums today's `cost_records`
/// where `model LIKE 'gbrain_extract%'` (the prefix
/// `crate::gbrain::chat_extractor` writes via the `"gbrain_extract"`
/// cost_tag passed to `MemoryOsLlm::complete_text`).
///
/// Standalone fn (not on AppState) so the agent dispatcher can call it
/// given only the raw `Arc<Mutex<Connection>>` — matches the shape
/// `set_gbrain_extractor_pipeline` hands the dispatcher.
pub fn today_gbrain_extract_tokens(
    db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> u32 {
    let since_ms = current_day_start_ms();
    let conn = match db.lock() {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let n: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0)
             FROM cost_records
             WHERE model LIKE 'gbrain_extract%' AND created_at >= ?1",
            params![since_ms],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    n.max(0) as u32
}

/// Pure helper: which threshold (if any) was crossed on this turn?
/// Fires 100 OR 80 (preferring 100 when both crossed in one turn), or None.
/// Never fires when budget <= 0.0.
pub fn fired_threshold(total_before: f64, total_after: f64, budget: f64) -> Option<u8> {
    if budget <= 0.0 { return None; }
    let crossed_100 = total_before / budget < 1.00 && total_after / budget >= 1.00;
    let crossed_80 = total_before / budget < 0.80 && total_after / budget >= 0.80;
    if crossed_100 { Some(100) }
    else if crossed_80 { Some(80) }
    else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_threshold_when_under_80() {
        assert_eq!(fired_threshold(10.0, 50.0, 100.0), None);
    }
    #[test]
    fn fires_80_when_crossing_upward() {
        assert_eq!(fired_threshold(75.0, 85.0, 100.0), Some(80));
    }
    #[test]
    fn does_not_refire_80_when_already_above() {
        assert_eq!(fired_threshold(85.0, 90.0, 100.0), None);
    }
    #[test]
    fn fires_100_when_crossing_upward() {
        assert_eq!(fired_threshold(95.0, 105.0, 100.0), Some(100));
    }
    #[test]
    fn fires_100_not_80_when_crossing_both_at_once() {
        assert_eq!(fired_threshold(50.0, 150.0, 100.0), Some(100));
    }
    #[test]
    fn no_fire_when_budget_zero_or_negative() {
        assert_eq!(fired_threshold(50.0, 150.0, 0.0), None);
        assert_eq!(fired_threshold(50.0, 150.0, -10.0), None);
    }

    // ─── Sprint 2.4b — gbrain extractor budget query ─────────────

    fn open_test_db_with_cost_records() -> std::sync::Arc<std::sync::Mutex<rusqlite::Connection>> {
        use std::sync::{Arc, Mutex};
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        // Minimal V13 cost_records schema — just enough to exercise the
        // SUM-by-prefix queries. Real schema has more columns but they
        // don't affect this test.
        conn.execute_batch(
            "CREATE TABLE cost_records (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 model TEXT NOT NULL,
                 input_tokens INTEGER NOT NULL,
                 output_tokens INTEGER NOT NULL,
                 cost_usd REAL,
                 created_at INTEGER NOT NULL
             );",
        )
        .unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn insert_cost(
        db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
        model: &str,
        input: i64,
        output: i64,
        at_ms: i64,
    ) {
        let conn = db.lock().unwrap();
        conn.execute(
            "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)
             VALUES (?1, 'test', ?2, ?3, ?4, 0.0, ?5)",
            params![uuid::Uuid::new_v4().to_string(), model, input, output, at_ms],
        )
        .unwrap();
    }

    #[test]
    fn today_gbrain_extract_tokens_sums_only_gbrain_prefix() {
        let db = open_test_db_with_cost_records();
        let now = chrono::Utc::now().timestamp_millis();
        // Two gbrain rows today → should sum to 350
        insert_cost(&db, "gbrain_extract:claude-haiku-4-5", 100, 50, now);
        insert_cost(&db, "gbrain_extract:other-model", 150, 50, now);
        // Memory_learning row today — must NOT count toward gbrain total
        insert_cost(&db, "memory_learning:claude-haiku-4-5", 999, 999, now);
        // Gbrain row from yesterday — must NOT count
        let yesterday = now - 26 * 60 * 60 * 1000;
        insert_cost(&db, "gbrain_extract:claude-haiku-4-5", 500, 500, yesterday);

        assert_eq!(today_gbrain_extract_tokens(&db), 350);
    }

    #[test]
    fn today_gbrain_extract_tokens_returns_zero_when_no_rows() {
        let db = open_test_db_with_cost_records();
        assert_eq!(today_gbrain_extract_tokens(&db), 0);
    }
}
