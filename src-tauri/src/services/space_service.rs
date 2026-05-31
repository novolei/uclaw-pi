//! Workspace ("space") CRUD as a Tauri-independent service — the business logic
//! behind the `commands::space` thin commands.
//!
//! Operates on a borrowed `&Connection` plus an explicit `workspace_root: &Path`
//! (never `AppState`/`State`) so it unit-tests without Tauri. Extracted from the
//! legacy `tauri_commands.rs` god file per the code-organization ADR (2026-05-31);
//! `list`'s NULL-`path` backfill (compute dir → mkdir → persist) previously lived
//! inline in the command body.

use std::path::Path;

use rusqlite::Connection;

use crate::error::Error;
use crate::ipc::{CreateSpaceInput, SpaceResponse};
use crate::tauri_commands::compute_workspace_dir;

/// Create / list / delete workspaces (the `spaces` table).
pub trait SpaceService {
    /// Insert a new workspace sorting last (sort_order = MAX(existing) + 1).
    fn create(&self, conn: &Connection, input: CreateSpaceInput) -> Result<SpaceResponse, Error>;
    /// List workspaces, backfilling any NULL `path` on the fly: the `default`
    /// workspace maps to `workspace_root`; others to a per-workspace subdir
    /// derived from the name. Backfill best-effort mkdirs and persists the path.
    fn list(&self, conn: &Connection, workspace_root: &Path) -> Result<Vec<SpaceResponse>, Error>;
    /// Delete a workspace by id. Returns whether a row was removed.
    fn delete(&self, conn: &Connection, id: &str) -> Result<bool, Error>;
}

/// The SQLite-backed implementation (the only one in production).
pub struct DbSpace;

impl SpaceService for DbSpace {
    fn create(&self, conn: &Connection, input: CreateSpaceInput) -> Result<SpaceResponse, Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let icon = input.icon.unwrap_or_else(|| "📁".into());

        // Compute sort_order = MAX(existing) + 1 so new workspace sorts last.
        let sort_order: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM spaces",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        conn.execute(
            "INSERT INTO spaces (id, name, icon, sort_order, attached_dirs, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, '[]', ?5, ?5)",
            rusqlite::params![id, input.name, icon, sort_order, now],
        )
        .map_err(Error::Database)?;

        Ok(SpaceResponse {
            id,
            name: input.name,
            icon,
            path: None,
            attached_dirs: vec![],
            sort_order,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    fn list(&self, conn: &Connection, workspace_root: &Path) -> Result<Vec<SpaceResponse>, Error> {
        // Workspaces created before Task 4's auto-mkdir have NULL path. Backfill
        // them on-the-fly: default workspace → workground root; others → a
        // per-workspace subdir derived from the name. Create the directory and
        // persist the path so the frontend FileBrowser has a stable target.

        // First pass: read raw rows.
        let mut stmt = conn
            .prepare(
                "SELECT id, name, icon, path, attached_dirs, sort_order, created_at, updated_at
                 FROM spaces ORDER BY sort_order ASC",
            )
            .map_err(Error::Database)?;
        type RawRow = (String, String, String, Option<String>, String, i64, String, String);
        let rows: Vec<RawRow> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2).unwrap_or_else(|_| "📁".into()),
                    row.get::<_, Option<String>>(3).ok().flatten(),
                    row.get::<_, String>(4).unwrap_or_else(|_| "[]".into()),
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(Error::Database)?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);

        let mut spaces = Vec::with_capacity(rows.len());
        for (id, name, icon, raw_path, attached_json, sort_order, created_at, updated_at) in rows {
            let trimmed_path = raw_path.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty());
            let path = if let Some(p) = trimmed_path {
                p.to_string()
            } else {
                // Backfill: default → workground root; others → per-workspace subdir.
                let resolved = if id == "default" {
                    workspace_root.to_path_buf()
                } else {
                    compute_workspace_dir(workspace_root, &name, None, &id)
                        .unwrap_or_else(|_| workspace_root.to_path_buf())
                };
                // Best-effort mkdir + persist; failures don't block list.
                let _ = std::fs::create_dir_all(&resolved);
                let resolved_str = resolved.to_string_lossy().into_owned();
                let _ = conn.execute(
                    "UPDATE spaces SET path = ?1 WHERE id = ?2",
                    rusqlite::params![&resolved_str, &id],
                );
                resolved_str
            };
            let attached_dirs: Vec<String> = serde_json::from_str(&attached_json).unwrap_or_default();
            spaces.push(SpaceResponse {
                id,
                name,
                icon,
                path: Some(path),
                attached_dirs,
                sort_order,
                created_at,
                updated_at,
            });
        }

        Ok(spaces)
    }

    fn delete(&self, conn: &Connection, id: &str) -> Result<bool, Error> {
        let rows = conn
            .execute("DELETE FROM spaces WHERE id = ?1", rusqlite::params![id])
            .map_err(Error::Database)?;
        Ok(rows > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory DB with just the `spaces` columns the service touches.
    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE spaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                icon TEXT,
                path TEXT,
                attached_dirs TEXT NOT NULL DEFAULT '[]',
                sort_order INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
        )
        .unwrap();
        c
    }

    /// A unique temp workspace root per test so the backfill mkdir is isolated.
    fn temp_root() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("uclaw-space-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_then_list_round_trips_and_assigns_sort_order() {
        let c = conn();
        let root = temp_root();

        let a = DbSpace
            .create(&c, CreateSpaceInput { name: "Alpha".into(), icon: Some("🅰️".into()) })
            .unwrap();
        assert_eq!(a.sort_order, 0, "first space sorts at 0");
        assert_eq!(a.icon, "🅰️");
        assert_eq!(a.path, None, "create does not resolve a path");

        let b = DbSpace
            .create(&c, CreateSpaceInput { name: "Beta".into(), icon: None })
            .unwrap();
        assert_eq!(b.sort_order, 1, "second space sorts after the first");
        assert_eq!(b.icon, "📁", "default icon applied when None");

        let listed = DbSpace.list(&c, &root).unwrap();
        assert_eq!(listed.len(), 2);
        // ORDER BY sort_order ASC.
        assert_eq!(listed[0].name, "Alpha");
        assert_eq!(listed[1].name, "Beta");
        // Every listed space got a resolved (non-NULL) path via backfill.
        assert!(listed.iter().all(|s| s.path.is_some()));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn list_backfills_null_path_and_persists_it() {
        let c = conn();
        let root = temp_root();

        // Seed a row with NULL path directly (simulating pre-mkdir data).
        c.execute(
            "INSERT INTO spaces (id, name, icon, path, attached_dirs, sort_order, created_at, updated_at)
             VALUES ('ws1', 'My Project', '📁', NULL, '[]', 0, 't', 't')",
            [],
        )
        .unwrap();

        let listed = DbSpace.list(&c, &root).unwrap();
        let resolved = listed[0].path.clone().expect("path backfilled");
        // Non-default id → per-workspace subdir under the root (slugified name).
        assert!(resolved.starts_with(root.to_string_lossy().as_ref()));
        assert!(std::path::Path::new(&resolved).is_dir(), "backfill mkdir'd the dir");

        // Path was persisted: re-reading the column returns the resolved path.
        let persisted: Option<String> = c
            .query_row("SELECT path FROM spaces WHERE id = 'ws1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(persisted.as_deref(), Some(resolved.as_str()));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn list_maps_default_space_to_workspace_root() {
        let c = conn();
        let root = temp_root();
        c.execute(
            "INSERT INTO spaces (id, name, icon, path, attached_dirs, sort_order, created_at, updated_at)
             VALUES ('default', 'Default', '📁', NULL, '[]', 0, 't', 't')",
            [],
        )
        .unwrap();

        let listed = DbSpace.list(&c, &root).unwrap();
        assert_eq!(
            listed[0].path.as_deref(),
            Some(root.to_string_lossy().as_ref()),
            "default space resolves to the workspace root itself"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn delete_reports_whether_a_row_was_removed() {
        let c = conn();
        DbSpace
            .create(&c, CreateSpaceInput { name: "Gone".into(), icon: None })
            .unwrap();
        let id: String = c.query_row("SELECT id FROM spaces", [], |r| r.get(0)).unwrap();

        assert!(DbSpace.delete(&c, &id).unwrap(), "existing row → true");
        assert!(!DbSpace.delete(&c, &id).unwrap(), "second delete → false");
        assert!(!DbSpace.delete(&c, "nope").unwrap(), "unknown id → false");
    }
}
