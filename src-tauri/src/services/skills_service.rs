//! Skills-domain logic as a Tauri-independent service.
//!
//! Per the code-organization ADR (2026-05-31), most Skills commands delegate to
//! the in-memory [`crate::skills::SkillsRegistry`] manager (held in
//! `state.skills_registry`) and need no service — they moved verbatim to
//! [`crate::commands::skills`]. The **one** piece of genuine inline SQL in that
//! domain is the per-workspace skill-tag read that
//! `list_active_manifest_skills` performs against the `spaces` table (V19+
//! `skill_tags` column) to scope the active manifest. That read is lifted here
//! so it operates on a borrowed `&Connection` and unit-tests without Tauri.
//!
//! The surrounding manifest computation
//! ([`crate::skills_manifest::compute_active_manifest_entries`]) + provenance
//! enrichment stays in the thin command — it is bound to the live
//! `skills_registry` lock and the `memory_graph_store`, neither of which is a
//! plain `&Connection`, so it is not service-extractable.

use rusqlite::Connection;

use crate::error::Error;

/// Read-side access to the Skills domain's SQLite state (today just the
/// per-workspace skill-tag scoping column).
pub trait SkillsService {
    /// Read the workspace's configured skill tags (V19+ `spaces.skill_tags`,
    /// a JSON array of strings). Returns an empty vec when the space row is
    /// missing, the column is NULL, or the JSON fails to parse — all of which
    /// mean "no scoping filter", matching the agent loop's behavior in
    /// `send_agent_message`.
    fn workspace_skill_tags(&self, conn: &Connection, space_id: &str) -> Result<Vec<String>, Error>;
}

/// The SQLite-backed implementation (the only one in production).
pub struct DbSkills;

impl SkillsService for DbSkills {
    fn workspace_skill_tags(&self, conn: &Connection, space_id: &str) -> Result<Vec<String>, Error> {
        let raw: Option<String> = conn
            .query_row(
                "SELECT skill_tags FROM spaces WHERE id = ?1",
                rusqlite::params![space_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .unwrap_or(None);
        Ok(raw
            .as_deref()
            .and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn missing_space_yields_no_tags() {
        let conn = test_conn();
        // No row inserted → empty (no scoping filter).
        let tags = DbSkills.workspace_skill_tags(&conn, "nonexistent").unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn empty_array_default_yields_no_tags() {
        let conn = test_conn();
        // skill_tags is `TEXT NOT NULL DEFAULT '[]'` (V19), so a bare insert
        // gets the empty-array default → no scoping filter.
        conn.execute(
            "INSERT INTO spaces (id, name) VALUES ('s1', 'Space One')",
            [],
        )
        .unwrap();
        let tags = DbSkills.workspace_skill_tags(&conn, "s1").unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn parses_json_array_of_tags() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO spaces (id, name, skill_tags) VALUES ('s2', 'Space Two', ?1)",
            rusqlite::params![r#"["research","process"]"#],
        )
        .unwrap();
        let tags = DbSkills.workspace_skill_tags(&conn, "s2").unwrap();
        assert_eq!(tags, vec!["research".to_string(), "process".to_string()]);
    }

    #[test]
    fn malformed_json_yields_no_tags() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO spaces (id, name, skill_tags) VALUES ('s3', 'Space Three', 'not json')",
            [],
        )
        .unwrap();
        // Unparseable column → empty (graceful fallback, no error).
        let tags = DbSkills.workspace_skill_tags(&conn, "s3").unwrap();
        assert!(tags.is_empty());
    }
}
