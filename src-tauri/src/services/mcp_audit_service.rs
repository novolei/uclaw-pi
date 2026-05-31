//! MCP-audit SQL as a Tauri-independent service — the business logic behind the
//! `commands::mcp::list_mcp_audit` thin command.
//!
//! Operates on a borrowed `&Connection` (never `AppState`/`State`) so it
//! unit-tests without Tauri. Extracted from the legacy `tauri_commands.rs` god
//! file per the code-organization ADR (2026-05-31). The rest of the MCP domain
//! delegates to the in-memory `McpManager` and stays thin in `commands::mcp`;
//! only `list_mcp_audit` reads the `mcp_audit` table directly, so only its SQL
//! lives here.

use rusqlite::Connection;

use crate::mcp::McpAuditEntry;

/// Read access to the `mcp_audit` log. Values are stored pre-redacted (the write
/// path redacts env values before insert), so this reader returns them as-is.
pub trait McpAuditService {
    /// Most-recent-first audit rows. `server_id = None` returns all servers;
    /// `cap` is the already-clamped row limit (caller enforces the 1..=1000
    /// ceiling). Rows that fail to map are skipped rather than failing the query.
    fn list(
        &self,
        conn: &Connection,
        server_id: Option<&str>,
        cap: i64,
    ) -> Result<Vec<McpAuditEntry>, String>;
}

/// The SQLite-backed implementation (the only one in production).
pub struct DbMcpAudit;

impl McpAuditService for DbMcpAudit {
    fn list(
        &self,
        conn: &Connection,
        server_id: Option<&str>,
        cap: i64,
    ) -> Result<Vec<McpAuditEntry>, String> {
        let (sql, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match server_id {
            Some(id) => (
                "SELECT id, server_id, event_kind, message_redacted, created_at \
                 FROM mcp_audit WHERE server_id = ?1 \
                 ORDER BY created_at DESC LIMIT ?2",
                vec![Box::new(id.to_string()), Box::new(cap)],
            ),
            None => (
                "SELECT id, server_id, event_kind, message_redacted, created_at \
                 FROM mcp_audit ORDER BY created_at DESC LIMIT ?1",
                vec![Box::new(cap)],
            ),
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
        let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
        let rows = stmt
            .query_map(params_ref.as_slice(), |r| {
                Ok(McpAuditEntry {
                    id: r.get(0)?,
                    server_id: r.get(1)?,
                    event_kind: r.get(2)?,
                    message_redacted: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })
            .map_err(|e| format!("query: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stand-alone `mcp_audit` schema mirroring migration V40, so the service
    /// unit-tests against an in-memory DB without the full migration stack.
    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE mcp_audit (
                id                 TEXT PRIMARY KEY,
                server_id          TEXT NOT NULL,
                event_kind         TEXT NOT NULL,
                message_redacted   TEXT NOT NULL,
                created_at         INTEGER NOT NULL
            );",
        )
        .unwrap();
        conn
    }

    fn insert(conn: &Connection, id: &str, server: &str, kind: &str, msg: &str, ts: i64) {
        conn.execute(
            "INSERT INTO mcp_audit (id, server_id, event_kind, message_redacted, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, server, kind, msg, ts],
        )
        .unwrap();
    }

    #[test]
    fn empty_table_returns_no_rows() {
        let conn = setup();
        let rows = DbMcpAudit.list(&conn, None, 100).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn returns_all_servers_most_recent_first() {
        let conn = setup();
        insert(&conn, "a", "srv1", "connect", "ok", 100);
        insert(&conn, "b", "srv2", "error", "boom", 300);
        insert(&conn, "c", "srv1", "disconnect", "bye", 200);

        let rows = DbMcpAudit.list(&conn, None, 100).unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        // ordered by created_at DESC: 300, 200, 100
        assert_eq!(ids, vec!["b", "c", "a"]);
    }

    #[test]
    fn filters_by_server_id() {
        let conn = setup();
        insert(&conn, "a", "srv1", "connect", "ok", 100);
        insert(&conn, "b", "srv2", "error", "boom", 300);
        insert(&conn, "c", "srv1", "disconnect", "bye", 200);

        let rows = DbMcpAudit.list(&conn, Some("srv1"), 100).unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["c", "a"]); // both srv1, DESC by time
        assert!(rows.iter().all(|r| r.server_id == "srv1"));
    }

    #[test]
    fn cap_limits_row_count() {
        let conn = setup();
        for i in 0..5 {
            insert(&conn, &format!("id{i}"), "srv1", "connect", "ok", i);
        }
        let rows = DbMcpAudit.list(&conn, None, 2).unwrap();
        assert_eq!(rows.len(), 2);
        // newest two: ts 4 then 3
        assert_eq!(rows[0].created_at, 4);
        assert_eq!(rows[1].created_at, 3);
    }
}
