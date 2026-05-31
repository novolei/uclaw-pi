//! Automation-domain SQL as a Tauri-independent service — the business logic
//! behind the `commands::automation::get_or_create_spec_home_thread` thin
//! command.
//!
//! Operates on a borrowed `&Connection` (never `AppState`/`State`) so it
//! unit-tests without Tauri. Extracted from the legacy `tauri_commands.rs` god
//! file per the code-organization ADR (2026-05-31). The rest of the Automation
//! domain (`list_automations`, `trigger_automation_manual`, `stop_automation_runs`,
//! `get_automation_activity`) delegates to the async
//! [`crate::automation::runtime::AppRuntimeService`] and stays thin in
//! `commands::automation`; only `get_or_create_spec_home_thread` reaches into the
//! DB directly (find-or-create a `agent_sessions` "home thread" row), so only its
//! SQL lives here.

use rusqlite::{Connection, OptionalExtension};

use crate::automation::runtime::run_session::{ensure_automations_space, resolve_home_space};

/// Find-or-create the per-spec "home thread" agent session.
pub trait AutomationService {
    /// Resolve (creating if absent) the singleton home-thread `agent_sessions`
    /// row for an automation spec, returning the camelCase session JSON the UI
    /// expects.
    ///
    /// The home thread is identified by
    /// `metadata_json.$.origin == 'automation:home_thread'` scoped to
    /// `metadata_json.$.spec_id == spec_id`. Ensures the shared "Automations"
    /// space exists and resolves the spec's home space before either branch.
    fn get_or_create_home_thread(
        &self,
        conn: &Connection,
        spec_id: &str,
    ) -> Result<serde_json::Value, String>;
}

/// The SQLite-backed implementation (the only one in production).
pub struct DbAutomation;

impl AutomationService for DbAutomation {
    fn get_or_create_home_thread(
        &self,
        conn: &Connection,
        spec_id: &str,
    ) -> Result<serde_json::Value, String> {
        ensure_automations_space(conn)
            .map_err(|e| format!("ensure automations space: {e}"))?;

        let space_id = resolve_home_space(conn, spec_id)
            .map_err(|e| format!("resolve home space: {e}"))?;

        // Try to find existing home-thread session.
        let existing: Option<(String, String, i64, i64, i64, i64, i64)> = conn
            .query_row(
                "SELECT id, title, message_count, pinned, archived, created_at, updated_at
                 FROM agent_sessions
                 WHERE json_extract(metadata_json, '$.spec_id') = ?1
                   AND json_extract(metadata_json, '$.origin') = 'automation:home_thread'
                 LIMIT 1",
                rusqlite::params![spec_id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| format!("query home thread: {e}"))?;

        if let Some((id, title, msg_count, pinned, archived, created_at, updated_at)) = existing {
            return Ok(serde_json::json!({
                "id": id,
                "workspaceId": space_id,
                "title": title,
                "messageCount": msg_count,
                "pinned": pinned != 0,
                "archived": archived != 0,
                "createdAt": created_at,
                "updatedAt": updated_at,
            }));
        }

        // Create new home-thread session.
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let meta = serde_json::json!({
            "spec_id": spec_id,
            "origin": "automation:home_thread"
        });

        conn.execute(
            "INSERT INTO agent_sessions
             (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
             VALUES (?1,?2,'Home thread',?3,0,0,0,?4,?4)",
            rusqlite::params![&id, &space_id, meta.to_string(), now],
        )
        .map_err(|e| format!("insert home thread: {e}"))?;

        Ok(serde_json::json!({
            "id": id,
            "workspaceId": space_id,
            "title": "Home thread",
            "messageCount": 0,
            "pinned": false,
            "archived": false,
            "createdAt": now,
            "updatedAt": now,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stand-alone schema mirroring the live V20 shape that the home-thread SQL
    /// (and its `ensure_automations_space` / `resolve_home_space` helpers) touch,
    /// so the service unit-tests against an in-memory DB without the full
    /// migration stack: `spaces`, `automation_specs` (with the V20 `space_id`
    /// column), and `agent_sessions`.
    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE spaces (
                id         TEXT PRIMARY KEY,
                name       TEXT NOT NULL,
                icon       TEXT NOT NULL DEFAULT '📁',
                path       TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE automation_specs (
                id         TEXT PRIMARY KEY,
                space_id   TEXT,
                name       TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE agent_sessions (
                id            TEXT PRIMARY KEY,
                space_id      TEXT NOT NULL DEFAULT 'default',
                title         TEXT NOT NULL DEFAULT 'New session',
                metadata_json TEXT NOT NULL DEFAULT '{}',
                message_count INTEGER NOT NULL DEFAULT 0,
                pinned        INTEGER NOT NULL DEFAULT 0,
                archived      INTEGER NOT NULL DEFAULT 0,
                created_at    INTEGER NOT NULL,
                updated_at    INTEGER NOT NULL
            );",
        )
        .unwrap();
        conn
    }

    /// Insert an `automation_specs` row. `space_id = None` mirrors a spec with
    /// no bound space (the common case → home thread lands in the shared
    /// "Automations" space). In production `get_or_create_spec_home_thread` is
    /// always called with a real, existing spec id, so every test seeds the spec
    /// row that `resolve_home_space` reads.
    fn insert_spec(conn: &Connection, spec_id: &str, space_id: Option<&str>) {
        conn.execute(
            "INSERT INTO automation_specs (id, space_id, name) VALUES (?1, ?2, 'S')",
            rusqlite::params![spec_id, space_id],
        )
        .unwrap();
    }

    fn home_thread_count(conn: &Connection, spec_id: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM agent_sessions
             WHERE json_extract(metadata_json, '$.spec_id') = ?1
               AND json_extract(metadata_json, '$.origin') = 'automation:home_thread'",
            rusqlite::params![spec_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn creates_home_thread_when_absent() {
        let conn = setup();
        insert_spec(&conn, "spec-1", None);
        let v = DbAutomation
            .get_or_create_home_thread(&conn, "spec-1")
            .unwrap();

        assert_eq!(v["title"], "Home thread");
        assert_eq!(v["messageCount"], 0);
        assert_eq!(v["pinned"], false);
        assert_eq!(v["archived"], false);
        // No space_id set on the spec → falls back to the shared "Automations".
        assert_eq!(v["workspaceId"], "automations");
        assert!(v["id"].as_str().is_some());
        assert_eq!(home_thread_count(&conn, "spec-1"), 1);

        // The shared Automations space was auto-created (idempotent helper).
        let space_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM spaces WHERE id = 'automations'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(space_exists, 1);
    }

    #[test]
    fn returns_existing_home_thread_without_creating_a_second() {
        let conn = setup();
        insert_spec(&conn, "spec-1", None);
        let first = DbAutomation
            .get_or_create_home_thread(&conn, "spec-1")
            .unwrap();
        let first_id = first["id"].as_str().unwrap().to_string();

        let second = DbAutomation
            .get_or_create_home_thread(&conn, "spec-1")
            .unwrap();

        // Same row returned, no duplicate inserted.
        assert_eq!(second["id"].as_str().unwrap(), first_id);
        assert_eq!(home_thread_count(&conn, "spec-1"), 1);
    }

    #[test]
    fn honors_spec_space_id_when_set() {
        let conn = setup();
        conn.execute(
            "INSERT INTO spaces (id, name) VALUES ('proj', 'Project')",
            [],
        )
        .unwrap();
        insert_spec(&conn, "spec-2", Some("proj"));

        let v = DbAutomation
            .get_or_create_home_thread(&conn, "spec-2")
            .unwrap();

        assert_eq!(v["workspaceId"], "proj");
    }

    #[test]
    fn distinct_specs_get_distinct_home_threads() {
        let conn = setup();
        insert_spec(&conn, "spec-a", None);
        insert_spec(&conn, "spec-b", None);
        let a = DbAutomation
            .get_or_create_home_thread(&conn, "spec-a")
            .unwrap();
        let b = DbAutomation
            .get_or_create_home_thread(&conn, "spec-b")
            .unwrap();

        assert_ne!(a["id"].as_str().unwrap(), b["id"].as_str().unwrap());
        assert_eq!(home_thread_count(&conn, "spec-a"), 1);
        assert_eq!(home_thread_count(&conn, "spec-b"), 1);
    }
}
