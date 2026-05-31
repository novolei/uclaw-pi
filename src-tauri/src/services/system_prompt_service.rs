//! System-prompt-domain logic as a Tauri-independent service — the business
//! logic behind the `commands::system_prompt` thin commands.
//!
//! Operates on a borrowed `&Connection` (never `AppState`/`State`) so it
//! unit-tests without Tauri. Extracted from the legacy `tauri_commands.rs` god
//! file per the code-organization ADR (2026-05-31).
//!
//! These commands own genuine inline SQL against three tables:
//!   - `system_prompts` — the user-defined + built-in prompt rows
//!   - `system_prompt_versions` — an append-only snapshot per create/update
//!   - `settings` — the `default_prompt_id` and `append_datetime_username` keys
//!
//! All of that read/write/version-snapshot logic moves here. The cross-cutting
//! [`crate::tauri_commands::invalidate_prompt_cache`] is **not** moved: it
//! `.clear()`s a module-private prompt cache that is shared with the agent
//! prompt-build path (`resolve_user_system_prompt` / `get_system_prompt` /
//! `substitute_template_vars`, which stay in `tauri_commands.rs`). Splitting the
//! invalidator from the cache it owns would fracture that cohesion, so the
//! service calls back into the already-`pub` `invalidate_prompt_cache()` after
//! each mutation — preserving the exact original interleaving.

use rusqlite::Connection;

use crate::error::Error;
use crate::ipc::{
    SystemPromptConfigDto, SystemPromptCreateInput, SystemPromptDto, SystemPromptUpdateInput,
    SystemPromptVersionDto,
};

/// Read a single `system_prompts` row by id into a [`SystemPromptDto`].
fn read_prompt_row(conn: &Connection, id: &str) -> Result<SystemPromptDto, Error> {
    conn.query_row(
        "SELECT id, name, content, is_builtin, sort_order, created_at, updated_at FROM system_prompts WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(SystemPromptDto {
                id: row.get(0)?,
                name: row.get(1)?,
                content: row.get(2)?,
                is_builtin: Some(row.get::<_, i64>(3)? != 0),
                sort_order: Some(row.get(4)?),
                created_at: Some(row.get(5)?),
                updated_at: Some(row.get(6)?),
            })
        },
    )
    .map_err(Error::Database)
}

/// CRUD + versioning over the system-prompt tables.
pub trait SystemPromptService {
    /// Load all prompts (sorted), plus the global `default_prompt_id` and the
    /// `append_datetime_username` toggle.
    fn config(&self, conn: &Connection) -> Result<SystemPromptConfigDto, Error>;
    /// Create a user-defined prompt (+ initial version snapshot). Invalidates
    /// the prompt cache.
    fn create(
        &self,
        conn: &Connection,
        input: SystemPromptCreateInput,
    ) -> Result<SystemPromptDto, Error>;
    /// Delete a user-defined prompt (built-ins are protected); falls the
    /// default back to `builtin-default` if it pointed here. Invalidates the
    /// prompt cache.
    fn delete(&self, conn: &Connection, id: &str) -> Result<(), Error>;
    /// Update a prompt's name and/or content (built-ins are read-only),
    /// snapshotting the prior state into the version table first. Invalidates
    /// the prompt cache.
    fn update(
        &self,
        conn: &Connection,
        id: &str,
        input: SystemPromptUpdateInput,
    ) -> Result<SystemPromptDto, Error>;
    /// Set the global `default_prompt_id`. Invalidates the prompt cache.
    fn set_default(&self, conn: &Connection, id: &str) -> Result<(), Error>;
    /// Read a prompt's version history (newest first).
    fn versions(
        &self,
        conn: &Connection,
        prompt_id: &str,
    ) -> Result<Vec<SystemPromptVersionDto>, Error>;
    /// Persist the "append date/time and username" preference.
    fn set_append_setting(&self, conn: &Connection, enabled: bool) -> Result<(), Error>;
}

/// The SQLite-backed implementation (the only one in production).
pub struct DbSystemPrompt;

impl SystemPromptService for DbSystemPrompt {
    fn config(&self, conn: &Connection) -> Result<SystemPromptConfigDto, Error> {
        let mut stmt = conn
            .prepare("SELECT id, name, content, is_builtin, sort_order, created_at, updated_at FROM system_prompts ORDER BY sort_order ASC, created_at ASC")
            .map_err(Error::Database)?;
        let prompts: Vec<SystemPromptDto> = stmt
            .query_map([], |row| {
                Ok(SystemPromptDto {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    content: row.get(2)?,
                    is_builtin: Some(row.get::<_, i64>(3)? != 0),
                    sort_order: Some(row.get(4)?),
                    created_at: Some(row.get(5)?),
                    updated_at: Some(row.get(6)?),
                })
            })
            .map_err(Error::Database)?
            .filter_map(|r| r.ok())
            .collect();

        let default_prompt_id: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'default_prompt_id'",
                [],
                |r| r.get(0),
            )
            .ok();

        let append_setting: Option<bool> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'append_datetime_username'",
                [],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.parse::<bool>().ok());

        Ok(SystemPromptConfigDto {
            prompts,
            default_prompt_id: default_prompt_id.or(Some("builtin-default".to_string())),
            append_date_time_and_user_name: append_setting,
        })
    }

    fn create(
        &self,
        conn: &Connection,
        input: SystemPromptCreateInput,
    ) -> Result<SystemPromptDto, Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();

        // Find next sort_order
        let max_order: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) FROM system_prompts",
                [],
                |r| r.get(0),
            )
            .unwrap_or(-1);

        conn.execute(
            "INSERT INTO system_prompts (id, name, content, is_builtin, sort_order, created_at, updated_at) VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6)",
            rusqlite::params![id, input.name, input.content, max_order + 1, now, now],
        ).map_err(Error::Database)?;

        // Record initial version snapshot
        let version_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO system_prompt_versions (id, prompt_id, name, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![version_id, id, input.name, input.content, now],
        ).map_err(Error::Database)?;

        tracing::info!(prompt_id = %id, name = %input.name, "System prompt created");
        crate::tauri_commands::invalidate_prompt_cache();
        Ok(SystemPromptDto {
            id,
            name: input.name,
            content: input.content,
            is_builtin: Some(false),
            sort_order: Some(max_order + 1),
            created_at: Some(now),
            updated_at: Some(now),
        })
    }

    fn delete(&self, conn: &Connection, id: &str) -> Result<(), Error> {
        // Block deletion of built-in prompts
        let is_builtin: bool = conn
            .query_row(
                "SELECT is_builtin != 0 FROM system_prompts WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap_or(false);

        if is_builtin {
            return Err(Error::InvalidInput("Cannot delete built-in prompts".into()));
        }

        conn.execute(
            "DELETE FROM system_prompts WHERE id = ?1",
            rusqlite::params![id],
        ).map_err(Error::Database)?;

        // If the deleted prompt was the default, fall back to builtin-default
        let default_id: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'default_prompt_id'",
                [],
                |r| r.get(0),
            )
            .ok();
        if default_id.as_deref() == Some(id) {
            conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES ('default_prompt_id', 'builtin-default')",
                [],
            ).map_err(Error::Database)?;
        }

        tracing::info!(prompt_id = %id, "System prompt deleted");
        crate::tauri_commands::invalidate_prompt_cache();
        Ok(())
    }

    fn update(
        &self,
        conn: &Connection,
        id: &str,
        input: SystemPromptUpdateInput,
    ) -> Result<SystemPromptDto, Error> {
        // Block updates of built-in prompts
        let is_builtin: bool = conn
            .query_row(
                "SELECT is_builtin != 0 FROM system_prompts WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap_or(false);

        if is_builtin {
            return Err(Error::InvalidInput("Cannot modify built-in prompts".into()));
        }

        let now = chrono::Utc::now().timestamp_millis();

        // Snapshot current state before updating (version history)
        {
            let current: Option<(String, String)> = conn
                .query_row(
                    "SELECT name, content FROM system_prompts WHERE id = ?1",
                    rusqlite::params![id],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                )
                .ok();
            if let Some((cur_name, cur_content)) = current {
                let version_id = uuid::Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO system_prompt_versions (id, prompt_id, name, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![version_id, id, cur_name, cur_content, now],
                ).map_err(Error::Database)?;
            }
        }

        if let Some(ref name) = input.name {
            conn.execute(
                "UPDATE system_prompts SET name = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![name, now, id],
            ).map_err(Error::Database)?;
        }
        if let Some(ref content) = input.content {
            conn.execute(
                "UPDATE system_prompts SET content = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![content, now, id],
            ).map_err(Error::Database)?;
        }

        // Re-read the updated row
        let updated = read_prompt_row(conn, id)?;

        tracing::info!(prompt_id = %id, "System prompt updated");
        crate::tauri_commands::invalidate_prompt_cache();
        Ok(updated)
    }

    fn set_default(&self, conn: &Connection, id: &str) -> Result<(), Error> {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('default_prompt_id', ?1)",
            rusqlite::params![id],
        ).map_err(Error::Database)?;
        tracing::info!(prompt_id = %id, "Default system prompt set");
        crate::tauri_commands::invalidate_prompt_cache();
        Ok(())
    }

    fn versions(
        &self,
        conn: &Connection,
        prompt_id: &str,
    ) -> Result<Vec<SystemPromptVersionDto>, Error> {
        let mut stmt = conn
            .prepare("SELECT id, prompt_id, name, content, created_at FROM system_prompt_versions WHERE prompt_id = ?1 ORDER BY created_at DESC")
            .map_err(Error::Database)?;
        let versions: Vec<SystemPromptVersionDto> = stmt
            .query_map(rusqlite::params![prompt_id], |row| {
                Ok(SystemPromptVersionDto {
                    id: row.get(0)?,
                    prompt_id: row.get(1)?,
                    name: row.get(2)?,
                    content: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(Error::Database)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(versions)
    }

    fn set_append_setting(&self, conn: &Connection, enabled: bool) -> Result<(), Error> {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('append_datetime_username', ?1)",
            rusqlite::params![if enabled { "true" } else { "false" }],
        ).map_err(Error::Database)?;
        tracing::info!(enabled, "Append date/time setting updated");
        Ok(())
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
    fn config_returns_builtin_default_and_seed_prompt() {
        let conn = test_conn();
        let cfg = DbSystemPrompt.config(&conn).unwrap();
        // The migration seeds the 'builtin-default' prompt row.
        assert!(cfg.prompts.iter().any(|p| p.id == "builtin-default"));
        // No default set yet → falls back to builtin-default.
        assert_eq!(cfg.default_prompt_id.as_deref(), Some("builtin-default"));
    }

    #[test]
    fn create_persists_prompt_and_initial_version() {
        let conn = test_conn();
        let dto = DbSystemPrompt
            .create(
                &conn,
                SystemPromptCreateInput {
                    name: "My Prompt".into(),
                    content: "Be terse.".into(),
                },
            )
            .unwrap();
        assert_eq!(dto.name, "My Prompt");
        assert_eq!(dto.is_builtin, Some(false));
        // Shows up in config.
        let cfg = DbSystemPrompt.config(&conn).unwrap();
        assert!(cfg.prompts.iter().any(|p| p.id == dto.id && p.name == "My Prompt"));
        // An initial version snapshot was recorded.
        let versions = DbSystemPrompt.versions(&conn, &dto.id).unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].content, "Be terse.");
    }

    #[test]
    fn update_snapshots_prior_state_and_rewrites_row() {
        let conn = test_conn();
        let dto = DbSystemPrompt
            .create(
                &conn,
                SystemPromptCreateInput {
                    name: "V1".into(),
                    content: "first".into(),
                },
            )
            .unwrap();
        let updated = DbSystemPrompt
            .update(
                &conn,
                &dto.id,
                SystemPromptUpdateInput {
                    name: Some("V2".into()),
                    content: Some("second".into()),
                },
            )
            .unwrap();
        assert_eq!(updated.name, "V2");
        assert_eq!(updated.content, "second");
        // History now holds the initial snapshot + the pre-update snapshot
        // (newest first): the pre-update one carries the old "first" content.
        let versions = DbSystemPrompt.versions(&conn, &dto.id).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].content, "first", "newest snapshot = prior state");
    }

    #[test]
    fn update_rejects_builtin() {
        let conn = test_conn();
        let err = DbSystemPrompt
            .update(
                &conn,
                "builtin-default",
                SystemPromptUpdateInput {
                    name: Some("hax".into()),
                    content: None,
                },
            )
            .unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)));
    }

    #[test]
    fn delete_rejects_builtin_but_removes_user_prompt() {
        let conn = test_conn();
        // Built-in is protected.
        let err = DbSystemPrompt.delete(&conn, "builtin-default").unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)));
        // A user prompt deletes cleanly.
        let dto = DbSystemPrompt
            .create(
                &conn,
                SystemPromptCreateInput {
                    name: "Tmp".into(),
                    content: "x".into(),
                },
            )
            .unwrap();
        DbSystemPrompt.delete(&conn, &dto.id).unwrap();
        let cfg = DbSystemPrompt.config(&conn).unwrap();
        assert!(!cfg.prompts.iter().any(|p| p.id == dto.id));
    }

    #[test]
    fn delete_resets_default_when_it_pointed_at_removed_prompt() {
        let conn = test_conn();
        let dto = DbSystemPrompt
            .create(
                &conn,
                SystemPromptCreateInput {
                    name: "Default Me".into(),
                    content: "y".into(),
                },
            )
            .unwrap();
        DbSystemPrompt.set_default(&conn, &dto.id).unwrap();
        assert_eq!(
            DbSystemPrompt.config(&conn).unwrap().default_prompt_id.as_deref(),
            Some(dto.id.as_str())
        );
        DbSystemPrompt.delete(&conn, &dto.id).unwrap();
        // Default falls back to builtin-default.
        assert_eq!(
            DbSystemPrompt.config(&conn).unwrap().default_prompt_id.as_deref(),
            Some("builtin-default")
        );
    }

    #[test]
    fn append_setting_round_trips_through_config() {
        let conn = test_conn();
        // Unset → None.
        assert_eq!(DbSystemPrompt.config(&conn).unwrap().append_date_time_and_user_name, None);
        DbSystemPrompt.set_append_setting(&conn, true).unwrap();
        assert_eq!(
            DbSystemPrompt.config(&conn).unwrap().append_date_time_and_user_name,
            Some(true)
        );
        DbSystemPrompt.set_append_setting(&conn, false).unwrap();
        assert_eq!(
            DbSystemPrompt.config(&conn).unwrap().append_date_time_and_user_name,
            Some(false)
        );
    }
}
