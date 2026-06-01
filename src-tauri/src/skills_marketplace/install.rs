//! Install/uninstall/update for skills.sh skills.
//! Real files land in <skills_root>/_marketplace/<slug>/ (Marketplace tier).

use std::path::{Path, PathBuf};
use super::{MarketplaceError, SkillDetail};

/// "expo/skills/react-native" -> "expo__skills__react-native".
#[must_use]
pub fn flatten_slug(id: &str) -> String {
    id.replace('/', "__")
}

/// Reject path-traversal / absolute components in a file path.
fn safe_rel(path: &str) -> Result<&str, MarketplaceError> {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') || path.contains("..") {
        return Err(MarketplaceError::Invalid(format!("unsafe path: {path}")));
    }
    Ok(path)
}

/// Write `detail.files` into <skills_root>/_marketplace/<slug>/ and return that dir.
/// Overwrites an existing install at that slug.
pub fn write_skill_files(skills_root: &Path, slug: &str, detail: &SkillDetail) -> Result<PathBuf, MarketplaceError> {
    if slug.is_empty() || slug.contains('/') || slug.contains("..") {
        return Err(MarketplaceError::Invalid(format!("unsafe slug: {slug}")));
    }
    let dir = skills_root.join("_marketplace").join(slug);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| MarketplaceError::Install(e.to_string()))?;
    }
    for f in &detail.files {
        let rel = safe_rel(&f.path)?;
        let dest = dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| MarketplaceError::Install(e.to_string()))?;
        }
        std::fs::write(&dest, &f.contents).map_err(|e| MarketplaceError::Install(e.to_string()))?;
    }
    Ok(dir)
}

/// Create <workspace>/.uclaw/skills/<slug> -> <global_dir> (symlink, visibility only).
pub fn link_into_workspace(workspace: &Path, slug: &str, global_dir: &Path) -> Result<(), MarketplaceError> {
    let link_dir = workspace.join(".uclaw").join("skills");
    std::fs::create_dir_all(&link_dir).map_err(|e| MarketplaceError::Install(e.to_string()))?;
    let link = link_dir.join(slug);
    if link.exists() || std::fs::symlink_metadata(&link).is_ok() {
        let _ = std::fs::remove_file(&link);
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(global_dir, &link).map_err(|e| MarketplaceError::Install(e.to_string()))?;
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(global_dir, &link).map_err(|e| MarketplaceError::Install(e.to_string()))?;
    Ok(())
}

/// Remove the workspace symlink for a slug (workspace-uninstall).
pub fn unlink_from_workspace(workspace: &Path, slug: &str) {
    let _ = std::fs::remove_file(workspace.join(".uclaw").join("skills").join(slug));
}

use rusqlite::OptionalExtension as _;

/// Record an install in V25 (item_type="skill"; `version` stores the skills.sh hash).
pub fn record_install(conn: &rusqlite::Connection, slug: &str, hash: &str, now_secs: i64) -> Result<(), MarketplaceError> {
    conn.execute(
        "INSERT OR REPLACE INTO marketplace_standalone_installs (slug, item_type, version, installed_at, mcp_server_id) VALUES (?,?,?,?,NULL)",
        rusqlite::params![slug, "skill", hash, now_secs],
    ).map_err(|e| MarketplaceError::Install(e.to_string()))?;
    Ok(())
}

pub fn read_install_version(conn: &rusqlite::Connection, slug: &str) -> Result<Option<String>, MarketplaceError> {
    conn.query_row(
        "SELECT version FROM marketplace_standalone_installs WHERE slug=?1 AND item_type='skill'",
        rusqlite::params![slug], |r| r.get::<_, String>(0),
    ).optional().map_err(|e| MarketplaceError::Install(e.to_string()))
}

pub fn remove_install_row(conn: &rusqlite::Connection, slug: &str) -> Result<(), MarketplaceError> {
    conn.execute("DELETE FROM marketplace_standalone_installs WHERE slug=?1 AND item_type='skill'",
        rusqlite::params![slug]).map_err(|e| MarketplaceError::Install(e.to_string()))?;
    Ok(())
}

/// True iff the stored hash differs from `latest_hash` (or not installed).
pub fn needs_update(conn: &rusqlite::Connection, slug: &str, latest_hash: &str) -> bool {
    match read_install_version(conn, slug) {
        Ok(Some(h)) => h != latest_hash,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills_marketplace::{SkillDetail, SkillFile, InstallScope};
    use tempfile::TempDir;

    fn detail(id: &str) -> SkillDetail {
        SkillDetail { id: id.into(), source: "expo/skills".into(), slug: "react-native".into(),
            hash: "h1".into(), files: vec![SkillFile { path: "SKILL.md".into(),
            contents: "---\nname: react-native\n---\nbody".into() }] }
    }

    #[test]
    fn slug_flattens_id() {
        assert_eq!(flatten_slug("expo/skills/react-native"), "expo__skills__react-native");
    }

    #[test]
    fn global_install_writes_files_under_marketplace() {
        let tmp = TempDir::new().unwrap();
        let skills_root = tmp.path().join("skills");
        let dir = write_skill_files(&skills_root, "expo__skills__react-native", &detail("expo/skills/react-native")).unwrap();
        assert!(dir.join("SKILL.md").exists());
        assert!(dir.starts_with(skills_root.join("_marketplace")));
        assert_eq!(std::fs::read_to_string(dir.join("SKILL.md")).unwrap(), "---\nname: react-native\n---\nbody");
    }

    #[test]
    fn rejects_path_traversal() {
        let tmp = TempDir::new().unwrap();
        let mut d = detail("x/y/z");
        d.files[0].path = "../evil.md".into();
        let err = write_skill_files(&tmp.path().join("skills"), "x__y__z", &d);
        assert!(err.is_err());
    }

    #[test]
    fn workspace_symlink_points_at_global_dir() {
        let tmp = TempDir::new().unwrap();
        let global = tmp.path().join("global_skills").join("_marketplace").join("s1");
        std::fs::create_dir_all(&global).unwrap();
        let ws = tmp.path().join("ws");
        link_into_workspace(&ws, "s1", &global).unwrap();
        let link = ws.join(".uclaw").join("skills").join("s1");
        assert!(link.exists());
        assert_eq!(std::fs::read_link(&link).unwrap(), global);
    }

    fn mem_conn() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE marketplace_standalone_installs (slug TEXT PRIMARY KEY, item_type TEXT NOT NULL, version TEXT NOT NULL, installed_at INTEGER NOT NULL, mcp_server_id TEXT);").unwrap();
        c
    }

    #[test]
    fn records_and_reads_install_row() {
        let c = mem_conn();
        record_install(&c, "expo__skills__react-native", "h1", 1000).unwrap();
        let v = read_install_version(&c, "expo__skills__react-native").unwrap();
        assert_eq!(v.as_deref(), Some("h1"));
        assert!(needs_update(&c, "expo__skills__react-native", "h2"));     // hash changed
        assert!(!needs_update(&c, "expo__skills__react-native", "h1"));    // same hash
    }
}
