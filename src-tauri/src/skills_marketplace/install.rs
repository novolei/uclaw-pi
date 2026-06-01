//! Install/uninstall/update for skills.sh skills.
//! Real files land in <skills_root>/_marketplace/<slug>/ (Marketplace tier).

use std::path::{Path, PathBuf};
use super::{MarketplaceError, SkillDetail};

/// "expo/skills/react-native" -> "expo__skills__react-native".
#[must_use]
pub fn flatten_slug(id: &str) -> String {
    id.replace('/', "__")
}

/// A safe single name/slug component: non-empty, not "." or "..", and only
/// `[A-Za-z0-9._-]`. Blocks "/", "\\", ":" (Windows drive), absolute paths, and
/// traversal — the install dir can never be escaped.
fn is_safe_component(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// Reject path-traversal / absolute components in a file path.
/// Splits on '/' and requires every segment to satisfy `is_safe_component`.
fn safe_rel(path: &str) -> Result<&str, MarketplaceError> {
    if path.is_empty() || path.split('/').any(|seg| !is_safe_component(seg)) {
        return Err(MarketplaceError::Invalid(format!("unsafe path: {path}")));
    }
    Ok(path)
}

/// Write `detail.files` into <skills_root>/_marketplace/<slug>/ and return that dir.
/// Overwrites an existing install at that slug.
pub fn write_skill_files(skills_root: &Path, slug: &str, detail: &SkillDetail) -> Result<PathBuf, MarketplaceError> {
    if !is_safe_component(slug) {
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
    if !is_safe_component(slug) {
        return Err(MarketplaceError::Invalid(format!("unsafe slug: {slug}")));
    }
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

// wired by P2 uninstall command
#[allow(dead_code)]
/// Remove the workspace symlink for a slug (workspace-uninstall).
pub fn unlink_from_workspace(workspace: &Path, slug: &str) {
    let _ = std::fs::remove_file(workspace.join(".uclaw").join("skills").join(slug));
}

/// Add `tag` to the `activation.tags` list in `<skill_dir>/SKILL.md`'s YAML
/// frontmatter, idempotently, preserving the markdown body. This is how a
/// workspace-scoped install activates: the SAME tag must appear on the skill
/// (here) AND in the space's `skill_tags` for `skill_matches_workspace`
/// (set-intersection) to match. Only the frontmatter is reserialized; the body
/// is rejoined verbatim.
pub fn add_activation_tag(skill_dir: &Path, tag: &str) -> Result<(), MarketplaceError> {
    let path = skill_dir.join("SKILL.md");
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| MarketplaceError::Install(format!("read SKILL.md: {e}")))?;

    // Split `---\n<frontmatter>\n---\n<body>`. No frontmatter ⇒ can't inject.
    let rest = raw
        .strip_prefix("---\n")
        .ok_or_else(|| MarketplaceError::Install("SKILL.md has no frontmatter".into()))?;
    let end = rest
        .find("\n---")
        .ok_or_else(|| MarketplaceError::Install("unterminated frontmatter".into()))?;
    let front = &rest[..end];
    let after = &rest[end + 4..]; // skip "\n---"
    let body = after.strip_prefix('\n').unwrap_or(after);

    let mut doc: serde_yml::Value = serde_yml::from_str(front)
        .map_err(|e| MarketplaceError::Install(format!("parse frontmatter: {e}")))?;
    let map = doc
        .as_mapping_mut()
        .ok_or_else(|| MarketplaceError::Install("frontmatter is not a mapping".into()))?;

    let act_key = serde_yml::Value::String("activation".to_string());
    if !map.contains_key(&act_key) {
        map.insert(
            act_key.clone(),
            serde_yml::Value::Mapping(serde_yml::Mapping::new()),
        );
    }
    let act_map = map
        .get_mut(&act_key)
        .and_then(|v| v.as_mapping_mut())
        .ok_or_else(|| MarketplaceError::Install("activation is not a mapping".into()))?;

    let tags_key = serde_yml::Value::String("tags".to_string());
    if !act_map.contains_key(&tags_key) {
        act_map.insert(tags_key.clone(), serde_yml::Value::Sequence(Vec::new()));
    }
    let seq = act_map
        .get_mut(&tags_key)
        .and_then(|v| v.as_sequence_mut())
        .ok_or_else(|| MarketplaceError::Install("activation.tags is not a list".into()))?;
    if !seq.iter().any(|v| v.as_str() == Some(tag)) {
        seq.push(serde_yml::Value::String(tag.to_string()));
    }

    let new_front = serde_yml::to_string(&doc)
        .map_err(|e| MarketplaceError::Install(format!("serialize frontmatter: {e}")))?;
    let new_raw = format!("---\n{new_front}---\n{body}");
    std::fs::write(&path, new_raw)
        .map_err(|e| MarketplaceError::Install(format!("write SKILL.md: {e}")))?;
    Ok(())
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

// wired by P2 uninstall command
#[allow(dead_code)]
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
    fn rejects_dot_slug_and_windows_abs_and_backslash() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("skills");
        // single-dot slug rejected
        assert!(write_skill_files(&root, ".", &detail("x/y/z")).is_err());
        // Windows-absolute file path rejected
        let mut d = detail("x/y/z"); d.files[0].path = "C:/evil.md".into();
        assert!(write_skill_files(&root, "x__y__z", &d).is_err());
        // backslash path rejected
        let mut d2 = detail("x/y/z"); d2.files[0].path = "examples\\evil.md".into();
        assert!(write_skill_files(&root, "x__y__z2", &d2).is_err());
        // legit subdir path ALLOWED
        let mut d3 = detail("x/y/z"); d3.files = vec![crate::skills_marketplace::SkillFile{ path: "examples/app.ts".into(), contents: "x".into() }];
        assert!(write_skill_files(&root, "x__y__z3", &d3).is_ok());
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

    #[test]
    fn add_activation_tag_adds_to_frontmatter_and_preserves_body() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        std::fs::write(
            dir.join("SKILL.md"),
            "---\nname: demo\ndescription: a demo skill\n---\n# Demo\n\nbody line\n",
        )
        .unwrap();

        add_activation_tag(dir, "ws-alpha").unwrap();
        let out = std::fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert!(out.contains("ws-alpha"), "tag written: {out}");
        assert!(
            out.contains("# Demo") && out.contains("body line"),
            "body preserved: {out}"
        );

        // Idempotent: a second call must not duplicate the tag.
        add_activation_tag(dir, "ws-alpha").unwrap();
        let out2 = std::fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert_eq!(out2.matches("ws-alpha").count(), 1, "no duplicate tag: {out2}");

        // Frontmatter is still valid YAML and the tag lives under activation.tags.
        let front = out2.strip_prefix("---\n").unwrap();
        let front = &front[..front.find("\n---").unwrap()];
        let doc: serde_yml::Value = serde_yml::from_str(front).unwrap();
        let tags = doc
            .get("activation")
            .and_then(|a| a.get("tags"))
            .and_then(|t| t.as_sequence())
            .expect("activation.tags is a sequence");
        assert!(tags.iter().any(|v| v.as_str() == Some("ws-alpha")));
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
