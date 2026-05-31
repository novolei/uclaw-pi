//! Workspace/space resolution — the filesystem cwd pi should run in for a
//! conversation, derived from the owning space's `spaces.path`.
//!
//! Tauri-independent (operates on a `&Connection`), so it unit-tests without
//! Tauri. Extracted out of `engine_persist` per the code-organization ADR
//! (2026-05-31): persistence and workspace-resolution are distinct concerns.

use std::path::PathBuf;

use rusqlite::Connection;

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
}
