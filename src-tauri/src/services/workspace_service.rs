//! Workspace/space resolution + persistence as a Tauri-independent service.
//!
//! Two concerns live here, both operating on a borrowed `&Connection` (never
//! `AppState`/`State`) so they unit-test without Tauri:
//!
//! 1. **cwd resolution** ([`WorkspaceService`]) — the filesystem cwd pi should
//!    run in for a conversation/session, derived from the owning space's
//!    `spaces.path`. Extracted out of `engine_persist` per the code-organization
//!    ADR (2026-05-31): persistence and workspace-resolution are distinct.
//! 2. **workspace DB CRUD** ([`DbWorkspace`]) — the `spaces` / `agent_sessions`
//!    SQL behind the `commands::workspace` thin commands (active-id, create /
//!    update / delete, skill-tags, attached-dirs, reorder, mention-root
//!    resolution). Lifted out of the legacy `tauri_commands.rs` god file (slice
//!    10) so the command bodies are pure parse → call service → map/emit.
//!
//! Cross-domain helpers that other god-file commands still call
//! (`require_workspace_exists`, `compute_workspace_dir`, `slugify`,
//! `active_workspace_root`, `session_workspace_root`, `resolve_workspace_id_or_default`)
//! stay `pub(crate)` in `tauri_commands.rs` and are imported here.

use std::path::PathBuf;

use rusqlite::Connection;

use crate::error::Error;
use crate::tauri_commands::require_workspace_exists;

/// Resolve the pi working directory for an agent session / chat conversation.
pub trait WorkspaceService {
    /// The owning space's path for an Agent session (`agent_sessions.space_id`).
    fn agent_session_cwd(&self, conn: &Connection, session_id: &str) -> Option<PathBuf>;
    /// The owning space's path for a chat conversation (`conversations.workspace_id`).
    fn conversation_cwd(&self, conn: &Connection, conversation_id: &str) -> Option<PathBuf>;
}

/// SQLite-backed implementation (the only one in production).
pub struct DbWorkspace;

impl WorkspaceService for DbWorkspace {
    fn agent_session_cwd(&self, conn: &Connection, session_id: &str) -> Option<PathBuf> {
        space_cwd(
            conn,
            "SELECT sp.path FROM agent_sessions s JOIN spaces sp ON s.space_id = sp.id WHERE s.id = ?1",
            session_id,
        )
    }

    fn conversation_cwd(&self, conn: &Connection, conversation_id: &str) -> Option<PathBuf> {
        space_cwd(
            conn,
            "SELECT sp.path FROM conversations c JOIN spaces sp ON c.workspace_id = sp.id WHERE c.id = ?1",
            conversation_id,
        )
    }
}

/// The space path iff it's a non-empty existing directory, else `None` — pi then
/// keeps its process cwd rather than erroring on a missing/empty workspace path.
fn space_cwd(conn: &Connection, sql: &str, id: &str) -> Option<PathBuf> {
    let path: String = conn.query_row(sql, [id], |r| r.get(0)).ok()?;
    if path.is_empty() {
        return None;
    }
    let pb = PathBuf::from(path);
    pb.is_dir().then_some(pb)
}

/// A fully-read `spaces` row, as the `update_workspace` command echoes it back.
pub struct WorkspaceRow {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub path: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

// ─── Workspace DB CRUD (the `commands::workspace` logic layer) ───────────────

impl DbWorkspace {
    /// The active workspace id from `settings`, or `None` if unset.
    pub fn active_id(&self, conn: &Connection) -> Option<String> {
        conn.query_row(
            "SELECT value FROM settings WHERE key = 'active_workspace_id'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
    }

    /// Switch the active workspace. Validates the target exists, persists the
    /// new id, and returns the *previous* active id (defaulting to `"default"`)
    /// so the caller can publish a switch event. The event-publish + Playwright
    /// MCP sync are caller (command) concerns, not DB logic.
    pub fn set_active_id(&self, conn: &Connection, id: &str) -> Result<String, Error> {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM spaces WHERE id = ?1",
                rusqlite::params![id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if !exists {
            return Err(Error::Internal(format!("Workspace '{}' not found", id)));
        }

        let old_id: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'active_workspace_id'",
                rusqlite::params![],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "default".to_string());

        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('active_workspace_id', ?1)",
            rusqlite::params![id],
        )
        .map_err(Error::Database)?;

        Ok(old_id)
    }

    /// Insert a new workspace row. The caller pre-computes + mkdirs the path (it
    /// needs `workspace_root`, a non-DB concern) and passes the resolved string
    /// in. Returns the assigned `sort_order` (MAX(existing) + 1, so it sorts last).
    pub fn create(
        &self,
        conn: &Connection,
        id: &str,
        name: &str,
        icon: &str,
        resolved_path: &str,
        now: &str,
    ) -> Result<i64, Error> {
        let sort_order: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM spaces",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        conn.execute(
            "INSERT INTO spaces (id, name, icon, path, sort_order, attached_dirs, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, '[]', ?6, ?6)",
            rusqlite::params![id, name, icon, resolved_path, sort_order, now],
        )
        .map_err(Error::Database)?;

        Ok(sort_order)
    }

    /// Apply name and/or icon updates to a workspace. Refuses to rename
    /// `'default'` (sentinel protection) but allows icon changes on it.
    pub fn update(
        &self,
        conn: &Connection,
        id: &str,
        name: Option<String>,
        icon: Option<String>,
    ) -> Result<(), Error> {
        if id == "default" && name.is_some() {
            return Err(Error::Internal(
                "cannot rename the 'default' workspace".into(),
            ));
        }
        require_workspace_exists(conn, id)?;
        let now = chrono::Utc::now().to_rfc3339();
        if let Some(n) = name.as_ref() {
            conn.execute(
                "UPDATE spaces SET name = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![n, &now, id],
            )
            .map_err(Error::Database)?;
        }
        if let Some(i) = icon.as_ref() {
            conn.execute(
                "UPDATE spaces SET icon = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![i, &now, id],
            )
            .map_err(Error::Database)?;
        }
        Ok(())
    }

    /// Read back a full `spaces` row (the `update_workspace` command echoes it).
    pub fn read_row(&self, conn: &Connection, id: &str) -> Result<WorkspaceRow, Error> {
        conn.query_row(
            "SELECT id, name, icon, path, sort_order, created_at, updated_at FROM spaces WHERE id = ?1",
            rusqlite::params![id],
            |r| {
                Ok(WorkspaceRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    icon: r.get(2)?,
                    path: r.get(3)?,
                    sort_order: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                })
            },
        )
        .map_err(Error::Database)
    }

    /// Read a workspace's `skill_tags` JSON column → `Vec<String>`.
    ///
    /// Returns `[]` (not an error) for: pre-V19 rows (NULL skill_tags), malformed
    /// JSON (logged + empty), or an unknown workspace id. The empty case shares
    /// the "no filter" manifest semantic, so degrading gracefully keeps the agent
    /// loop robust.
    pub fn get_skill_tags(&self, conn: &Connection, space_id: &str) -> Vec<String> {
        let raw: Option<String> = conn
            .query_row(
                "SELECT skill_tags FROM spaces WHERE id = ?1",
                rusqlite::params![space_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .unwrap_or(None);
        match raw.as_deref() {
            Some(json) => serde_json::from_str(json).unwrap_or_else(|e| {
                tracing::warn!(
                    space_id = %space_id, err = %e,
                    "skill_tags JSON malformed; returning empty"
                );
                Vec::new()
            }),
            None => Vec::new(),
        }
    }

    /// Persist a workspace's normalized `skill_tags` JSON. Returns rows affected
    /// (0 ⇒ unknown workspace id, which the command maps to `NotFound`).
    pub fn set_skill_tags(
        &self,
        conn: &Connection,
        space_id: &str,
        json: &str,
    ) -> Result<usize, Error> {
        conn.execute(
            "UPDATE spaces SET skill_tags = ?2, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![space_id, json],
        )
        .map_err(Error::Database)
    }

    /// Apply `sort_order = idx` for each id in the supplied ordered list, in a
    /// transaction (partial reorders don't leave the DB inconsistent). Validates
    /// every id exists first.
    pub fn reorder(&self, conn: &Connection, ordered_ids: &[String]) -> Result<(), Error> {
        for id in ordered_ids {
            require_workspace_exists(conn, id)?;
        }
        let tx = conn.unchecked_transaction().map_err(Error::Database)?;
        for (idx, id) in ordered_ids.iter().enumerate() {
            tx.execute(
                "UPDATE spaces SET sort_order = ?1 WHERE id = ?2",
                rusqlite::params![idx as i64, id],
            )
            .map_err(Error::Database)?;
        }
        tx.commit().map_err(Error::Database)?;
        Ok(())
    }

    /// The workspace-level `spaces.attached_dirs` list. Errors if the workspace
    /// doesn't exist.
    pub fn get_directories(
        &self,
        conn: &Connection,
        workspace_id: &str,
    ) -> Result<Vec<String>, Error> {
        require_workspace_exists(conn, workspace_id)?;
        let json: String = conn
            .query_row(
                "SELECT attached_dirs FROM spaces WHERE id = ?1",
                rusqlite::params![workspace_id],
                |r| r.get(0),
            )
            .map_err(Error::Database)?;
        serde_json::from_str(&json).map_err(|e| Error::Internal(format!("JSON parse: {}", e)))
    }

    /// The session-level `agent_sessions.attached_dirs` list. Errors if the
    /// session doesn't exist.
    pub fn list_session_directories(
        &self,
        conn: &Connection,
        session_id: &str,
    ) -> Result<Vec<String>, Error> {
        let json: String = conn
            .query_row(
                "SELECT attached_dirs FROM agent_sessions WHERE id = ?1",
                rusqlite::params![session_id],
                |r| r.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    Error::NotFound(format!("agent_session '{}'", session_id))
                }
                other => Error::Database(other),
            })?;
        serde_json::from_str(&json).map_err(|e| Error::Internal(format!("JSON parse: {}", e)))
    }

    /// Generic read-modify-write of an `attached_dirs` JSON column. Works for
    /// `spaces` (workspace level) and `agent_sessions` (session level). The
    /// closure receives the current list and returns the new one; we serialize
    /// back and write, branching on the table for the right `updated_at` encoding
    /// (`spaces` is RFC3339 TEXT; `agent_sessions` is INTEGER ms). Returns the
    /// new list.
    pub fn modify_attached_dirs<F>(
        &self,
        conn: &Connection,
        table: &str,
        id: &str,
        f: F,
    ) -> Result<Vec<String>, Error>
    where
        F: FnOnce(Vec<String>) -> Vec<String>,
    {
        let json: String = conn
            .query_row(
                &format!("SELECT attached_dirs FROM {} WHERE id = ?1", table),
                rusqlite::params![id],
                |r| r.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    Error::NotFound(format!("{} '{}'", table, id))
                }
                other => Error::Database(other),
            })?;
        let dirs: Vec<String> = serde_json::from_str(&json).unwrap_or_default();
        let new_dirs = f(dirs);
        let new_json = serde_json::to_string(&new_dirs)
            .map_err(|e| Error::Internal(format!("JSON encode: {}", e)))?;

        match table {
            "agent_sessions" => {
                conn.execute(
                    "UPDATE agent_sessions SET attached_dirs = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![&new_json, chrono::Utc::now().timestamp_millis(), id],
                )
                .map_err(Error::Database)?;
            }
            _ => {
                conn.execute(
                    &format!(
                        "UPDATE {} SET attached_dirs = ?1, updated_at = ?2 WHERE id = ?3",
                        table
                    ),
                    rusqlite::params![&new_json, chrono::Utc::now().to_rfc3339(), id],
                )
                .map_err(Error::Database)?;
            }
        }
        Ok(new_dirs)
    }

    /// Re-home all agent_sessions in the given workspace to `'default'`.
    /// Application-layer equivalent of `ON DELETE SET DEFAULT` (no FK exists on
    /// `agent_sessions.space_id` — Phase 1 spec §3 non-goals). Called by
    /// [`Self::delete`] BEFORE removing the spaces row.
    pub fn rehome_agent_sessions_to_default(
        &self,
        conn: &Connection,
        workspace_id: &str,
    ) -> Result<(), Error> {
        conn.execute(
            "UPDATE agent_sessions SET space_id = 'default', updated_at = ?2 WHERE space_id = ?1",
            rusqlite::params![workspace_id, chrono::Utc::now().timestamp_millis()],
        )
        .map_err(Error::Database)?;
        Ok(())
    }

    /// Delete a workspace and its dependent data. Clears the active-id setting if
    /// this workspace was active, re-homes its agent_sessions to `'default'`,
    /// cascades the memory-graph + KV memory tables (FK CASCADE only covers some
    /// of these), then drops the `spaces` row. The `'default'` sentinel guard is
    /// the caller's (command) responsibility.
    pub fn delete(&self, conn: &Connection, id: &str) -> Result<(), Error> {
        let active: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'active_workspace_id'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok();
        if active.as_deref() == Some(id) {
            let _ = conn.execute("DELETE FROM settings WHERE key = 'active_workspace_id'", []);
        }

        self.rehome_agent_sessions_to_default(conn, id)?;

        // Explicit memory-graph + KV cascade — FK CASCADE only fires on some of
        // these, so clean them up to avoid orphaned rows after the workspace goes.
        let _ = conn.execute(
            "DELETE FROM memory_keywords WHERE space_id = ?1",
            rusqlite::params![id],
        );
        let _ = conn.execute(
            "DELETE FROM memory_routes WHERE space_id = ?1",
            rusqlite::params![id],
        );
        let _ = conn.execute(
            "DELETE FROM memory_edges WHERE space_id = ?1",
            rusqlite::params![id],
        );
        let _ = conn.execute(
            "DELETE FROM memory_versions WHERE node_id IN (SELECT id FROM memory_nodes WHERE space_id = ?1)",
            rusqlite::params![id],
        );
        let _ = conn.execute(
            "DELETE FROM memory_fts WHERE node_id IN (SELECT id FROM memory_nodes WHERE space_id = ?1)",
            rusqlite::params![id],
        );
        let _ = conn.execute(
            "DELETE FROM memory_nodes WHERE space_id = ?1",
            rusqlite::params![id],
        );
        let _ = conn.execute(
            "DELETE FROM memories WHERE space_id = ?1",
            rusqlite::params![id],
        );
        tracing::info!(workspace_id = %id, "Cleaned up memory graph data for deleted workspace");

        conn.execute("DELETE FROM spaces WHERE id = ?1", rusqlite::params![id])
            .map_err(Error::Database)?;
        Ok(())
    }

    /// The on-disk path for a workspace (`spaces.path`), as a non-empty trimmed
    /// string. `Err(NotFound)` for an unknown id; `Ok(None)` for a row with an
    /// empty/NULL path (the upload commands map that to `InvalidInput`).
    pub fn workspace_path(
        &self,
        conn: &Connection,
        workspace_id: &str,
    ) -> Result<Option<String>, Error> {
        let raw: Option<String> = conn
            .query_row(
                "SELECT path FROM spaces WHERE id = ?1",
                rusqlite::params![workspace_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    Error::NotFound(format!("workspace '{}'", workspace_id))
                }
                other => Error::Database(other),
            })?;
        Ok(raw.filter(|s| !s.trim().is_empty()))
    }

    /// Resolve every root the `@`-mention file picker should walk for a given
    /// `session_id`, which may identify an agent session (Agent composer), a chat
    /// conversation (Chat composer), or neither (brand-new draft → globally-active
    /// workspace). Returns, de-duplicated in order: the space path, the
    /// workspace-level `spaces.attached_dirs`, and the session-level
    /// `agent_sessions.attached_dirs`. The actual filesystem walk is a non-DB
    /// concern and stays in the command.
    pub fn mention_search_roots(&self, conn: &Connection, session_id: &str) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();

        // Resolve the owning space: agent_sessions → conversations → active id.
        let mut space_id: Option<String> = conn
            .query_row(
                "SELECT space_id FROM agent_sessions WHERE id = ?1",
                rusqlite::params![session_id],
                |r| r.get::<_, String>(0),
            )
            .ok();
        if space_id.is_none() {
            space_id = conn
                .query_row(
                    "SELECT space_id FROM conversations WHERE id = ?1",
                    rusqlite::params![session_id],
                    |r| r.get::<_, String>(0),
                )
                .ok();
        }
        if space_id.is_none() {
            space_id = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = 'active_workspace_id'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .ok();
        }

        if let Some(sid) = space_id {
            // 1. spaces.path
            if let Ok(path) = conn.query_row::<Option<String>, _, _>(
                "SELECT path FROM spaces WHERE id = ?1",
                rusqlite::params![&sid],
                |r| r.get(0),
            ) {
                if let Some(p) = path.filter(|s| !s.trim().is_empty()) {
                    out.push(PathBuf::from(p));
                }
            }
            // 2. spaces.attached_dirs
            if let Ok(Some(raw)) = conn.query_row::<Option<String>, _, _>(
                "SELECT attached_dirs FROM spaces WHERE id = ?1",
                rusqlite::params![&sid],
                |r| r.get(0),
            ) {
                if let Ok(dirs) = serde_json::from_str::<Vec<String>>(&raw) {
                    for d in dirs {
                        if !d.trim().is_empty() {
                            out.push(PathBuf::from(d));
                        }
                    }
                }
            }
        }

        // 3. agent_sessions.attached_dirs (Agent mode only).
        if let Ok(Some(raw)) = conn.query_row::<Option<String>, _, _>(
            "SELECT attached_dirs FROM agent_sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        ) {
            if let Ok(dirs) = serde_json::from_str::<Vec<String>>(&raw) {
                for d in dirs {
                    if !d.trim().is_empty() {
                        out.push(PathBuf::from(d));
                    }
                }
            }
        }

        // Dedup while preserving order — same path under both workspace and
        // session attached_dirs shouldn't be double-walked.
        let mut seen = std::collections::HashSet::new();
        out.retain(|p| seen.insert(p.clone()));
        out
    }
}

/// Normalize a list of skill tags: trim, lowercase, drop empties, dedup while
/// preserving the user's original order. The returned vec is what lands in the
/// DB; the `set_workspace_skill_tags` command echoes it back so the frontend can
/// update local state without re-fetching.
pub fn normalize_skill_tags(input: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(input.len());
    for raw in input {
        let cleaned = raw.trim().to_lowercase();
        if cleaned.is_empty() {
            continue;
        }
        if seen.insert(cleaned.clone()) {
            out.push(cleaned);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_existing_dir_and_nones_missing_or_bad() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE spaces (id TEXT PRIMARY KEY, path TEXT NOT NULL);
             CREATE TABLE agent_sessions (id TEXT PRIMARY KEY, space_id TEXT);",
        )
        .unwrap();
        // No space row → None.
        c.execute("INSERT INTO agent_sessions VALUES ('orphan', 'nope')", []).unwrap();
        assert!(DbWorkspace.agent_session_cwd(&c, "orphan").is_none());
        // Nonexistent path → None.
        c.execute("INSERT INTO spaces VALUES ('bad', '/no/such/dir/xyzzy')", []).unwrap();
        c.execute("INSERT INTO agent_sessions VALUES ('s_bad', 'bad')", []).unwrap();
        assert!(DbWorkspace.agent_session_cwd(&c, "s_bad").is_none());
        // Existing directory resolves.
        let tmp = std::env::temp_dir();
        c.execute("INSERT INTO spaces VALUES ('ok', ?1)", [tmp.to_str().unwrap()]).unwrap();
        c.execute("INSERT INTO agent_sessions VALUES ('s_ok', 'ok')", []).unwrap();
        assert_eq!(DbWorkspace.agent_session_cwd(&c, "s_ok").as_deref(), Some(tmp.as_path()));
    }

    // ─── Workspace DB CRUD tests (ported from tauri_commands::workspace_integrity_tests) ──

    /// In-memory DB with the `spaces` + `agent_sessions` schema (and a seeded
    /// `'default'` workspace) the service touches. Mirrors the god-file's
    /// `fresh_db` so the lifted logic keeps its original coverage.
    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V1_INITIAL).unwrap();
        conn.execute_batch(crate::db::migrations::V8_AGENT_SESSIONS).unwrap();
        for stmt in crate::db::migrations::V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            conn.execute(stmt, []).unwrap();
        }
        for stmt in crate::db::migrations::V17_WORKSPACE_PATH_SORT_ATTACHED
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            conn.execute(stmt, []).unwrap();
        }
        // V19 adds the `spaces.skill_tags` column the skill-tag methods read/write.
        for stmt in crate::db::migrations::V19_SPACES_SKILL_TAGS
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            conn.execute(stmt, []).unwrap();
        }
        conn
    }

    fn insert_workspace(conn: &Connection, id: &str, name: &str) {
        conn.execute(
            "INSERT INTO spaces (id, name, icon, created_at, updated_at)
             VALUES (?1, ?2, '📁', datetime('now'), datetime('now'))",
            rusqlite::params![id, name],
        )
        .unwrap();
    }

    fn insert_session(conn: &Connection, id: &str, space_id: &str) {
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at)
             VALUES (?1, ?2, 'test', 0, 0)",
            rusqlite::params![id, space_id],
        )
        .unwrap();
    }

    fn space_id_of(conn: &Connection, session_id: &str) -> String {
        conn.query_row(
            "SELECT space_id FROM agent_sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn read_workspace_name(conn: &Connection, id: &str) -> String {
        conn.query_row(
            "SELECT name FROM spaces WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn read_workspace_icon(conn: &Connection, id: &str) -> String {
        conn.query_row(
            "SELECT icon FROM spaces WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn read_sort_order(conn: &Connection, id: &str) -> i64 {
        conn.query_row(
            "SELECT sort_order FROM spaces WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn read_workspace_dirs(conn: &Connection, id: &str) -> Vec<String> {
        let json: String = conn
            .query_row(
                "SELECT attached_dirs FROM spaces WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        serde_json::from_str(&json).unwrap()
    }

    fn read_session_dirs(conn: &Connection, id: &str) -> Vec<String> {
        let json: String = conn
            .query_row(
                "SELECT attached_dirs FROM agent_sessions WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn rehome_agent_sessions_moves_them_to_default() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-x", "x");
        insert_session(&conn, "s-1", "ws-x");
        insert_session(&conn, "s-2", "ws-x");
        DbWorkspace.rehome_agent_sessions_to_default(&conn, "ws-x").unwrap();
        assert_eq!(space_id_of(&conn, "s-1"), "default");
        assert_eq!(space_id_of(&conn, "s-2"), "default");
    }

    #[test]
    fn rehome_does_nothing_when_no_sessions_in_workspace() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-empty", "empty");
        assert!(DbWorkspace.rehome_agent_sessions_to_default(&conn, "ws-empty").is_ok());
    }

    #[test]
    fn update_workspace_changes_name() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-real", "Original");
        DbWorkspace.update(&conn, "ws-real", Some("Renamed".into()), None).unwrap();
        assert_eq!(read_workspace_name(&conn, "ws-real"), "Renamed");
    }

    #[test]
    fn update_workspace_refuses_to_rename_default() {
        let conn = fresh_db();
        let r = DbWorkspace.update(&conn, "default", Some("NotDefault".into()), None);
        assert!(r.is_err(), "renaming 'default' must return Err");
        assert_eq!(read_workspace_name(&conn, "default"), "默认工作区");
    }

    #[test]
    fn update_workspace_allows_icon_change_on_default() {
        let conn = fresh_db();
        DbWorkspace.update(&conn, "default", None, Some("🌟".into())).unwrap();
        assert_eq!(read_workspace_icon(&conn, "default"), "🌟");
    }

    #[test]
    fn create_assigns_sort_order_and_reads_back() {
        let conn = fresh_db();
        let now = chrono::Utc::now().to_rfc3339();
        // 'default' seeded at sort_order 0 → first created sorts after it.
        let so = DbWorkspace.create(&conn, "ws-a", "A", "📁", "/tmp/ws-a", &now).unwrap();
        assert!(so >= 1, "new workspace sorts after the seeded default");
        let row = DbWorkspace.read_row(&conn, "ws-a").unwrap();
        assert_eq!(row.name, "A");
        assert_eq!(row.path.as_deref(), Some("/tmp/ws-a"));
        assert_eq!(row.sort_order, so);
    }

    #[test]
    fn reorder_workspaces_sets_sort_order_by_array_index() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-a", "A");
        insert_workspace(&conn, "ws-b", "B");
        insert_workspace(&conn, "ws-c", "C");
        DbWorkspace.reorder(&conn, &["ws-c".into(), "ws-a".into(), "ws-b".into()]).unwrap();
        assert_eq!(read_sort_order(&conn, "ws-c"), 0);
        assert_eq!(read_sort_order(&conn, "ws-a"), 1);
        assert_eq!(read_sort_order(&conn, "ws-b"), 2);
    }

    #[test]
    fn reorder_workspaces_errors_on_unknown_id_no_partial_writes() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-a", "A");
        let before = read_sort_order(&conn, "ws-a");
        let result = DbWorkspace.reorder(&conn, &["ws-a".into(), "ghost".into()]);
        assert!(result.is_err(), "unknown id must error");
        assert_eq!(read_sort_order(&conn, "ws-a"), before);
    }

    #[test]
    fn attach_workspace_directory_appends_to_json() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-x", "X");
        let after = DbWorkspace
            .modify_attached_dirs(&conn, "spaces", "ws-x", |mut dirs| {
                dirs.push("/tmp/foo".into());
                dirs
            })
            .unwrap();
        assert_eq!(after, vec!["/tmp/foo".to_string()]);
        assert_eq!(read_workspace_dirs(&conn, "ws-x"), vec!["/tmp/foo".to_string()]);
    }

    #[test]
    fn detach_workspace_directory_removes_existing() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-x", "X");
        DbWorkspace
            .modify_attached_dirs(&conn, "spaces", "ws-x", |_| {
                vec!["/tmp/foo".into(), "/tmp/bar".into()]
            })
            .unwrap();
        let after = DbWorkspace
            .modify_attached_dirs(&conn, "spaces", "ws-x", |dirs| {
                dirs.into_iter().filter(|d| d != "/tmp/foo").collect()
            })
            .unwrap();
        assert_eq!(after, vec!["/tmp/bar".to_string()]);
    }

    #[test]
    fn get_directories_errors_on_unknown_workspace() {
        let conn = fresh_db();
        assert!(matches!(
            DbWorkspace.get_directories(&conn, "ghost"),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn session_attached_dirs_round_trip() {
        let conn = fresh_db();
        insert_session(&conn, "s-1", "default");
        let after = DbWorkspace
            .modify_attached_dirs(&conn, "agent_sessions", "s-1", |mut dirs| {
                dirs.push("/tmp/sess-dir".into());
                dirs
            })
            .unwrap();
        assert_eq!(after, vec!["/tmp/sess-dir".to_string()]);
        assert_eq!(read_session_dirs(&conn, "s-1"), vec!["/tmp/sess-dir".to_string()]);
        assert_eq!(
            DbWorkspace.list_session_directories(&conn, "s-1").unwrap(),
            vec!["/tmp/sess-dir".to_string()]
        );
    }

    #[test]
    fn list_session_directories_errors_on_unknown_session() {
        let conn = fresh_db();
        assert!(matches!(
            DbWorkspace.list_session_directories(&conn, "nope"),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn skill_tags_round_trip_and_empty_default() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-x", "X");
        // Unset → empty.
        assert!(DbWorkspace.get_skill_tags(&conn, "ws-x").is_empty());
        let normalized = normalize_skill_tags(vec!["  Research ".into(), "research".into()]);
        let json = serde_json::to_string(&normalized).unwrap();
        let rows = DbWorkspace.set_skill_tags(&conn, "ws-x", &json).unwrap();
        assert_eq!(rows, 1);
        assert_eq!(DbWorkspace.get_skill_tags(&conn, "ws-x"), vec!["research".to_string()]);
        // Unknown id → 0 rows.
        assert_eq!(DbWorkspace.set_skill_tags(&conn, "ghost", "[]").unwrap(), 0);
    }

    #[test]
    fn active_id_set_get_and_reject_unknown() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-a", "A");
        // Default seeded by V16 means active is unset until we set it.
        assert!(DbWorkspace.set_active_id(&conn, "ghost").is_err());
        let old = DbWorkspace.set_active_id(&conn, "ws-a").unwrap();
        assert_eq!(old, "default", "no prior active id falls back to 'default'");
        assert_eq!(DbWorkspace.active_id(&conn).as_deref(), Some("ws-a"));
        // Switching again returns the previous active id.
        insert_workspace(&conn, "ws-b", "B");
        assert_eq!(DbWorkspace.set_active_id(&conn, "ws-b").unwrap(), "ws-a");
    }

    #[test]
    fn delete_cascades_clears_active_and_rehomes() {
        let conn = fresh_db();
        insert_workspace(&conn, "ws-x", "X");
        insert_session(&conn, "s-1", "ws-x");
        DbWorkspace.set_active_id(&conn, "ws-x").unwrap();

        DbWorkspace.delete(&conn, "ws-x").unwrap();

        // spaces row gone.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM spaces WHERE id = 'ws-x'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        // Active id cleared.
        assert!(DbWorkspace.active_id(&conn).is_none());
        // Session re-homed.
        assert_eq!(space_id_of(&conn, "s-1"), "default");
    }

    #[test]
    fn workspace_path_reports_missing_empty_and_set() {
        let conn = fresh_db();
        // Unknown id → NotFound.
        assert!(matches!(
            DbWorkspace.workspace_path(&conn, "ghost"),
            Err(Error::NotFound(_))
        ));
        // Row with NULL path → Ok(None).
        insert_workspace(&conn, "ws-x", "X");
        assert_eq!(DbWorkspace.workspace_path(&conn, "ws-x").unwrap(), None);
        // Row with a real path → Ok(Some).
        conn.execute(
            "UPDATE spaces SET path = '/tmp/ws-x' WHERE id = 'ws-x'",
            [],
        )
        .unwrap();
        assert_eq!(
            DbWorkspace.workspace_path(&conn, "ws-x").unwrap().as_deref(),
            Some("/tmp/ws-x")
        );
    }

    #[test]
    fn mention_roots_resolve_from_session_then_conversation_then_active() {
        let conn = fresh_db();
        // Agent-session path: space.path + workspace attached + session attached.
        insert_workspace(&conn, "ws-x", "X");
        conn.execute("UPDATE spaces SET path = '/tmp/ws-x', attached_dirs = '[\"/tmp/ws-extra\"]' WHERE id = 'ws-x'", []).unwrap();
        insert_session(&conn, "s-1", "ws-x");
        conn.execute(
            "UPDATE agent_sessions SET attached_dirs = '[\"/tmp/sess-extra\"]' WHERE id = 's-1'",
            [],
        )
        .unwrap();
        let roots = DbWorkspace.mention_search_roots(&conn, "s-1");
        assert_eq!(
            roots,
            vec![
                PathBuf::from("/tmp/ws-x"),
                PathBuf::from("/tmp/ws-extra"),
                PathBuf::from("/tmp/sess-extra"),
            ]
        );

        // Unknown id with an active workspace set → that workspace's path.
        DbWorkspace.set_active_id(&conn, "ws-x").unwrap();
        let roots2 = DbWorkspace.mention_search_roots(&conn, "does-not-exist");
        assert_eq!(roots2.first(), Some(&PathBuf::from("/tmp/ws-x")));
    }

    #[test]
    fn normalize_skill_tags_trims_lowercases_dedups_preserving_order() {
        let input = vec![
            "  Research ".into(),
            "RESEARCH".into(),
            "".into(),
            "Writing".into(),
        ];
        assert_eq!(
            normalize_skill_tags(input),
            vec!["research".to_string(), "writing".to_string()]
        );
        assert_eq!(normalize_skill_tags(vec![]), Vec::<String>::new());
    }
}
