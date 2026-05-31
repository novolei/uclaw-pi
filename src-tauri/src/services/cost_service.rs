//! Per-turn cost settlement.
//!
//! Recomputes the cache-aware USD cost for one agent turn and records it to
//! `cost_records` (the dashboard + monthly-budget source), returning the
//! formatted `$x` string for the metadata badge.
//!
//! Extracted out of the `engine_sink` bridge per the code-organization ADR
//! (2026-05-31): the bridge translates events; cost orchestration lives here. The
//! pricing math itself is the pure, unit-tested [`crate::agent::types::calculate_cost_cached`].

use rusqlite::Connection;

use crate::agent::types::{calculate_cost_cached, format_cost};
use crate::app::AppState;
use crate::cost_store;
use crate::error::Error;
use crate::ipc::{
    DailyCostRollup, ModelCostRollup, SessionCostRollup, WorkspaceCostRollup,
};

/// Settle one turn's cost (recompute cache-aware USD + record + format).
pub trait CostService {
    /// Returns the formatted `$x` cost for the badge; records the turn to
    /// `cost_records` as a side effect.
    fn settle_turn(
        &self,
        state: &AppState,
        conversation_id: &str,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        cache_read_tokens: u32,
    ) -> String;
}

/// The production implementation: uses the model pricing table + `cost_store`.
pub struct PricingCostService;

impl CostService for PricingCostService {
    fn settle_turn(
        &self,
        state: &AppState,
        conversation_id: &str,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        cache_read_tokens: u32,
    ) -> String {
        let cost =
            calculate_cost_cached(model, input_tokens, output_tokens, cache_read_tokens);
        cost_store::record_cost(state, conversation_id, model, input_tokens, output_tokens, cost);
        format_cost(cost)
    }
}

// ─── Cost-rollup queries ────────────────────────────────────────────────
//
// The read side of the cost domain: SUM / GROUP BY aggregates over
// `cost_records` powering the cost dashboard + monthly-budget views. Lifted out
// of the legacy `tauri_commands.rs` god file (mis-filed under "Conversation
// Commands") per the code-organization ADR (2026-05-31). Operates on a borrowed
// `&Connection` (never `AppState`) so it unit-tests without Tauri; the thin
// `commands::cost` wrappers lock `state.db` and call into this.

/// Cost-record rollup queries (`cost_records`, joined to
/// `agent_sessions`/`conversations`/`spaces` for titles + workspace names).
pub trait CostQueryService {
    /// Per-UTC-day rollup over the last `days_back` days (default 30, clamped
    /// 1..=365). `created_at` is epoch-ms; bucketed by `YYYY-MM-DD`.
    fn daily(&self, conn: &Connection, days_back: Option<u32>)
        -> Result<Vec<DailyCostRollup>, Error>;
    /// Per-model rollup over the last `days_back` days, costliest first.
    fn by_model(
        &self,
        conn: &Connection,
        days_back: Option<u32>,
    ) -> Result<Vec<ModelCostRollup>, Error>;
    /// Per-session rollup over the last `days_back` days (default limit 50,
    /// clamped 1..=500), most-recently-used first. Titles COALESCE the agent and
    /// chat sources.
    fn by_session(
        &self,
        conn: &Connection,
        days_back: Option<u32>,
        limit: Option<u32>,
    ) -> Result<Vec<SessionCostRollup>, Error>;
    /// Per-workspace rollup of records since `since_ms` (current-month start,
    /// computed frontend-side in user-local time), costliest first.
    fn by_workspace(
        &self,
        conn: &Connection,
        since_ms: i64,
    ) -> Result<Vec<WorkspaceCostRollup>, Error>;
    /// Total USD across all records since `since_ms`.
    fn month_total(&self, conn: &Connection, since_ms: i64) -> Result<f64, Error>;
}

/// `days_back` → cutoff epoch-ms (default 30 days, clamped to 1..=365).
fn days_back_cutoff_ms(days_back: Option<u32>) -> i64 {
    let days = days_back.unwrap_or(30).clamp(1, 365);
    chrono::Utc::now().timestamp_millis() - (days as i64) * 86_400_000
}

impl CostQueryService for PricingCostService {
    fn daily(
        &self,
        conn: &Connection,
        days_back: Option<u32>,
    ) -> Result<Vec<DailyCostRollup>, Error> {
        let cutoff_ms = days_back_cutoff_ms(days_back);

        // SQLite stores created_at as epoch-ms. Group by UTC YYYY-MM-DD.
        let mut stmt = conn
            .prepare(
                "SELECT
                    strftime('%Y-%m-%d', created_at / 1000, 'unixepoch') AS day,
                    SUM(input_tokens) AS in_tok,
                    SUM(output_tokens) AS out_tok,
                    SUM(cost_usd) AS cost,
                    COUNT(*) AS turns
                 FROM cost_records
                 WHERE created_at >= ?1
                 GROUP BY day
                 ORDER BY day ASC",
            )
            .map_err(|e| Error::Internal(format!("prepare daily: {}", e)))?;

        let rows = stmt
            .query_map(rusqlite::params![cutoff_ms], |row| {
                Ok(DailyCostRollup {
                    day: row.get(0)?,
                    input_tokens: row.get(1)?,
                    output_tokens: row.get(2)?,
                    cost_usd: row.get(3)?,
                    turn_count: row.get(4)?,
                })
            })
            .map_err(|e| Error::Internal(format!("daily query: {}", e)))?;

        Ok(rows.flatten().collect())
    }

    fn by_model(
        &self,
        conn: &Connection,
        days_back: Option<u32>,
    ) -> Result<Vec<ModelCostRollup>, Error> {
        let cutoff_ms = days_back_cutoff_ms(days_back);

        let mut stmt = conn
            .prepare(
                "SELECT model,
                        SUM(input_tokens), SUM(output_tokens),
                        SUM(cost_usd), COUNT(*)
                 FROM cost_records
                 WHERE created_at >= ?1
                 GROUP BY model
                 ORDER BY cost_usd DESC",
            )
            .map_err(|e| Error::Internal(format!("prepare model: {}", e)))?;

        let rows = stmt
            .query_map(rusqlite::params![cutoff_ms], |row| {
                Ok(ModelCostRollup {
                    model: row.get(0)?,
                    input_tokens: row.get(1)?,
                    output_tokens: row.get(2)?,
                    cost_usd: row.get(3)?,
                    turn_count: row.get(4)?,
                })
            })
            .map_err(|e| Error::Internal(format!("model query: {}", e)))?;

        Ok(rows.flatten().collect())
    }

    fn by_session(
        &self,
        conn: &Connection,
        days_back: Option<u32>,
        limit: Option<u32>,
    ) -> Result<Vec<SessionCostRollup>, Error> {
        let cutoff_ms = days_back_cutoff_ms(days_back);
        let lim = limit.unwrap_or(50).clamp(1, 500);

        // session_id may live in either `agent_sessions` (agent runs) or
        // `conversations` (chat runs). Use COALESCE on the two title sources.
        let mut stmt = conn
            .prepare(
                "SELECT
                    cr.session_id,
                    COALESCE(s.title, c.title, '') AS title,
                    SUM(cr.input_tokens), SUM(cr.output_tokens),
                    SUM(cr.cost_usd), COUNT(*),
                    MAX(cr.created_at) AS last_used
                 FROM cost_records cr
                 LEFT JOIN agent_sessions s ON s.id = cr.session_id
                 LEFT JOIN conversations  c ON c.id = cr.session_id
                 WHERE cr.created_at >= ?1
                 GROUP BY cr.session_id
                 ORDER BY last_used DESC
                 LIMIT ?2",
            )
            .map_err(|e| Error::Internal(format!("prepare session: {}", e)))?;

        let rows = stmt
            .query_map(rusqlite::params![cutoff_ms, lim as i64], |row| {
                Ok(SessionCostRollup {
                    session_id: row.get(0)?,
                    title: row.get(1)?,
                    input_tokens: row.get(2)?,
                    output_tokens: row.get(3)?,
                    cost_usd: row.get(4)?,
                    turn_count: row.get(5)?,
                    last_used_at: row.get(6)?,
                })
            })
            .map_err(|e| Error::Internal(format!("session query: {}", e)))?;

        Ok(rows.flatten().collect())
    }

    fn by_workspace(
        &self,
        conn: &Connection,
        since_ms: i64,
    ) -> Result<Vec<WorkspaceCostRollup>, Error> {
        let mut stmt = conn
            .prepare(
                "SELECT
                     s.space_id AS workspace_id,
                     COALESCE(sp.name, '默认工作区') AS workspace_name,
                     COALESCE(sp.icon, 'Folder') AS workspace_icon,
                     COALESCE(SUM(c.cost_usd), 0) AS total_cost_usd,
                     COALESCE(SUM(c.input_tokens + c.output_tokens), 0) AS total_tokens
                 FROM cost_records c
                 JOIN agent_sessions s ON c.session_id = s.id
                 LEFT JOIN spaces sp ON sp.id = s.space_id
                 WHERE c.created_at >= ?1
                 GROUP BY s.space_id
                 ORDER BY total_cost_usd DESC",
            )
            .map_err(|e| Error::Internal(format!("prepare workspace rollup: {}", e)))?;
        let rows = stmt
            .query_map(rusqlite::params![since_ms], |row| {
                Ok(WorkspaceCostRollup {
                    workspace_id: row.get(0)?,
                    workspace_name: row.get(1)?,
                    workspace_icon: row.get(2)?,
                    total_cost_usd: row.get(3)?,
                    total_tokens: row.get(4)?,
                })
            })
            .map_err(|e| Error::Internal(format!("workspace rollup query: {}", e)))?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    fn month_total(&self, conn: &Connection, since_ms: i64) -> Result<f64, Error> {
        let total: f64 = conn
            .query_row(
                "SELECT COALESCE(SUM(cost_usd), 0) FROM cost_records WHERE created_at >= ?1",
                rusqlite::params![since_ms],
                |row| row.get(0),
            )
            .map_err(|e| Error::Internal(format!("month total query: {}", e)))?;
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory DB with the `cost_records` columns the queries touch plus the
    /// `agent_sessions` / `conversations` / `spaces` tables they join.
    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE cost_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT,
                model TEXT,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                cost_usd REAL NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE agent_sessions (id TEXT PRIMARY KEY, title TEXT, space_id TEXT);
            CREATE TABLE conversations (id TEXT PRIMARY KEY, title TEXT);
            CREATE TABLE spaces (id TEXT PRIMARY KEY, name TEXT, icon TEXT);",
        )
        .unwrap();
        c
    }

    /// Insert one cost record at `created_at` (epoch-ms).
    fn insert_record(
        c: &Connection,
        session_id: &str,
        model: &str,
        in_tok: i64,
        out_tok: i64,
        cost: f64,
        created_at_ms: i64,
    ) {
        c.execute(
            "INSERT INTO cost_records (session_id, model, input_tokens, output_tokens, cost_usd, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![session_id, model, in_tok, out_tok, cost, created_at_ms],
        )
        .unwrap();
    }

    #[test]
    fn by_model_aggregates_and_orders_by_cost_desc() {
        let c = conn();
        let now = chrono::Utc::now().timestamp_millis();
        // Two records for model-a (cheaper total), one big record for model-b.
        insert_record(&c, "s1", "model-a", 100, 50, 0.10, now);
        insert_record(&c, "s1", "model-a", 200, 60, 0.20, now);
        insert_record(&c, "s2", "model-b", 300, 70, 0.90, now);

        let rows = PricingCostService.by_model(&c, Some(30)).unwrap();
        assert_eq!(rows.len(), 2);
        // Costliest first.
        assert_eq!(rows[0].model, "model-b");
        assert!((rows[0].cost_usd - 0.90).abs() < 1e-9);
        assert_eq!(rows[0].turn_count, 1);
        // model-a SUMs the two records.
        assert_eq!(rows[1].model, "model-a");
        assert!((rows[1].cost_usd - 0.30).abs() < 1e-9);
        assert_eq!(rows[1].input_tokens, 300);
        assert_eq!(rows[1].output_tokens, 110);
        assert_eq!(rows[1].turn_count, 2);
    }

    #[test]
    fn daily_filters_by_cutoff_window() {
        let c = conn();
        let now = chrono::Utc::now().timestamp_millis();
        let day_ms = 86_400_000_i64;
        // In-window record + an old record 100 days back (outside default 30).
        insert_record(&c, "s1", "m", 10, 5, 0.01, now);
        insert_record(&c, "s1", "m", 10, 5, 0.01, now - 100 * day_ms);

        // Default window (30 days) sees only the recent record's day.
        let rows = PricingCostService.daily(&c, None).unwrap();
        let total_turns: i64 = rows.iter().map(|r| r.turn_count).sum();
        assert_eq!(total_turns, 1, "old record excluded by 30-day cutoff");
    }

    #[test]
    fn month_total_sums_since_cutoff() {
        let c = conn();
        let now = chrono::Utc::now().timestamp_millis();
        let since = now - 86_400_000; // 1 day ago
        insert_record(&c, "s1", "m", 1, 1, 1.50, now);
        insert_record(&c, "s1", "m", 1, 1, 2.50, now);
        insert_record(&c, "s1", "m", 1, 1, 9.99, now - 5 * 86_400_000); // before `since`

        let total = PricingCostService.month_total(&c, since).unwrap();
        assert!((total - 4.00).abs() < 1e-9, "only in-window records summed");
    }
}
