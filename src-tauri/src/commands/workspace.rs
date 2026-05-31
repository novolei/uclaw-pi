//! Workspace-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31) this domain is a **mix**: the
//! commands that carry genuine `&Connection` SQL on `spaces` / `agent_sessions`
//! delegate to [`crate::services::workspace_service::DbWorkspace`] (active-id,
//! create / update / delete, skill-tags, attached-dirs, reorder, upload-path
//! lookup, `@`-mention root resolution); the rest are pure filesystem CRUD over
//! `tokio::fs` / `std::fs` (uclaw.md read/write, directory listing, file
//! rename/move/read/delete, reveal/open-external) or thin delegations to the
//! `state.safety_manager` (the always/session allowed-path policy IPCs). No SQL
//! or business logic lives in the command bodies.
//!
//! Cross-domain helpers that other (still-in-`tauri_commands.rs`) commands also
//! call stay `pub(crate)` in the god file and are imported here:
//! `compute_workspace_dir`, `active_workspace_root`,
//! `sync_playwright_mcp_workspace_root`. The `do_rename_attached_file` /
//! `do_move_attached_file` / `sanitize_upload_filename` / `next_available_path`
//! filesystem helpers and the `WorkspaceFileMatch` result type + `MENTION_SKIP_DIRS`
//! list were Workspace-only and move here as module-private items.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::services::workspace_service::{normalize_skill_tags, DbWorkspace};
use crate::tauri_commands::{
    active_workspace_root, compute_workspace_dir, sync_playwright_mcp_workspace_root,
};

// ─── Active workspace ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_active_workspace_id(
    state: State<'_, AppState>,
) -> Result<Option<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    Ok(DbWorkspace.active_id(&conn))
}

#[tauri::command]
pub async fn set_active_workspace_id(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), Error> {
    // DB work in a block so the MutexGuard drops before the `.await`s below.
    let old_id = {
        let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
        DbWorkspace.set_active_id(&conn, &id)?
    };

    // Publish the workspace-switch event so subscribers (ProactiveService, …)
    // react. This is an AppState side effect, not DB logic, so it stays here.
    state
        .infra_service
        .publish(crate::infra::InfraEvent {
            id: 0, // assigned by InfraService
            event_type: crate::infra::InfraEventType::WorkspaceSwitched,
            platform: "local".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            message: crate::infra::ConversationMessage {
                role: "system".to_string(),
                content: String::new(),
            },
            metadata: serde_json::json!({
                "previous_workspace_id": old_id,
                "new_workspace_id": id,
            }),
            trace_id: None,
        })
        .await;

    sync_playwright_mcp_workspace_root(&state).await?;

    Ok(())
}

// ─── Workspace CRUD ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn create_workspace(
    state: State<'_, AppState>,
    name: String,
    path: Option<String>,
    icon: Option<String>,
) -> Result<serde_json::Value, Error> {
    let id = uuid::Uuid::new_v4().to_string();
    let icon = icon.unwrap_or_else(|| "📁".to_string());
    let now = chrono::Utc::now().to_rfc3339();

    // Compute target dir (auto-derived from name when no path) and mkdir it.
    // create_dir_all is idempotent. Path/mkdir need workspace_root, a non-DB
    // concern, so they stay in the command; the INSERT goes through the service.
    let dir = compute_workspace_dir(&state.workspace_root, &name, path, &id)?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| Error::Internal(format!("mkdir failed for {:?}: {}", &dir, e)))?;
    let resolved_path = dir.to_string_lossy().into_owned();

    let sort_order = {
        let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
        DbWorkspace.create(&conn, &id, &name, &icon, &resolved_path, &now)?
    };

    Ok(serde_json::json!({
        "id": id,
        "name": name,
        "icon": icon,
        "path": resolved_path,
        "sortOrder": sort_order,
        "attachedDirs": Vec::<String>::new(),
        "createdAt": now,
        "updatedAt": now,
    }))
}

#[tauri::command]
pub async fn update_workspace(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    icon: Option<String>,
) -> Result<serde_json::Value, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    DbWorkspace.update(&conn, &id, name, icon)?;
    let row = DbWorkspace.read_row(&conn, &id)?;
    Ok(serde_json::json!({
        "id": row.id,
        "name": row.name,
        "icon": row.icon,
        "path": row.path,
        "sortOrder": row.sort_order,
        "createdAt": row.created_at,
        "updatedAt": row.updated_at,
    }))
}

#[tauri::command]
pub async fn get_workspace_skill_tags(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<Vec<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    Ok(DbWorkspace.get_skill_tags(&conn, &space_id))
}

#[tauri::command]
pub async fn set_workspace_skill_tags(
    state: State<'_, AppState>,
    space_id: String,
    tags: Vec<String>,
) -> Result<Vec<String>, Error> {
    let normalized = normalize_skill_tags(tags);
    let json = serde_json::to_string(&normalized)
        .map_err(|e| Error::Internal(format!("serialize tags: {}", e)))?;
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    let rows = DbWorkspace.set_skill_tags(&conn, &space_id, &json)?;
    if rows == 0 {
        return Err(Error::NotFound(format!("workspace '{}' not found", space_id)));
    }
    tracing::info!(space_id = %space_id, tags = ?normalized, "Updated workspace skill_tags");
    Ok(normalized)
}

#[tauri::command]
pub async fn reorder_workspaces(
    state: State<'_, AppState>,
    ordered_ids: Vec<String>,
) -> Result<(), Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    DbWorkspace.reorder(&conn, &ordered_ids)
}

#[tauri::command]
pub async fn delete_workspace(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), Error> {
    if id == "default" {
        return Err(Error::Internal(
            "cannot delete the 'default' workspace".into(),
        ));
    }
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    DbWorkspace.delete(&conn, &id)
}

// ─── Attached directories (workspace + session) ──────────────────────────────

#[tauri::command]
pub async fn get_workspace_directories(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<Vec<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    DbWorkspace.get_directories(&conn, &workspace_id)
}

#[tauri::command]
pub async fn attach_workspace_directory(
    state: State<'_, AppState>,
    workspace_id: String,
    dir_path: String,
) -> Result<Vec<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    crate::tauri_commands::require_workspace_exists(&conn, &workspace_id)?;
    DbWorkspace.modify_attached_dirs(&conn, "spaces", &workspace_id, |mut dirs| {
        if !dirs.contains(&dir_path) {
            dirs.push(dir_path.clone());
        }
        dirs
    })
}

#[tauri::command]
pub async fn detach_workspace_directory(
    state: State<'_, AppState>,
    workspace_id: String,
    dir_path: String,
) -> Result<Vec<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    crate::tauri_commands::require_workspace_exists(&conn, &workspace_id)?;
    DbWorkspace.modify_attached_dirs(&conn, "spaces", &workspace_id, |dirs| {
        dirs.into_iter().filter(|d| d != &dir_path).collect()
    })
}

#[tauri::command]
pub async fn list_session_directories(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    DbWorkspace.list_session_directories(&conn, &session_id)
}

#[tauri::command]
pub async fn attach_session_directory(
    state: State<'_, AppState>,
    session_id: String,
    dir_path: String,
) -> Result<Vec<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    DbWorkspace.modify_attached_dirs(&conn, "agent_sessions", &session_id, |mut dirs| {
        if !dirs.contains(&dir_path) {
            dirs.push(dir_path.clone());
        }
        dirs
    })
}

#[tauri::command]
pub async fn detach_session_directory(
    state: State<'_, AppState>,
    session_id: String,
    dir_path: String,
) -> Result<Vec<String>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    DbWorkspace.modify_attached_dirs(&conn, "agent_sessions", &session_id, |dirs| {
        dirs.into_iter().filter(|d| d != &dir_path).collect()
    })
}

// ─── Attached-file actions (pure filesystem, no DB) ──────────────────────────

/// Rename a file within its parent directory. Returns the new absolute path.
fn do_rename_attached_file(path: &str, new_name: &str) -> Result<String, Error> {
    let p = std::path::Path::new(path);
    let parent = p
        .parent()
        .ok_or_else(|| Error::Internal(format!("no parent for {}", path)))?;
    let new_path = parent.join(new_name);
    if new_path.exists() {
        return Err(Error::Internal(format!(
            "destination already exists: {}",
            new_path.display()
        )));
    }
    std::fs::rename(p, &new_path)
        .map_err(|e| Error::Internal(format!("rename {} → {}: {}", path, new_path.display(), e)))?;
    Ok(new_path.to_string_lossy().into_owned())
}

/// Move a file into `dest_dir`, keeping the filename. Returns the new path.
/// Falls back to copy+delete on cross-volume (EXDEV) errors.
fn do_move_attached_file(path: &str, dest_dir: &str) -> Result<String, Error> {
    let p = std::path::Path::new(path);
    let fname = p
        .file_name()
        .ok_or_else(|| Error::Internal(format!("no filename in {}", path)))?;
    let new_path = std::path::Path::new(dest_dir).join(fname);
    if new_path.exists() {
        return Err(Error::Internal(format!(
            "destination already exists: {}",
            new_path.display()
        )));
    }
    match std::fs::rename(p, &new_path) {
        Ok(()) => Ok(new_path.to_string_lossy().into_owned()),
        Err(e) if e.raw_os_error() == Some(18) /* EXDEV */ => {
            std::fs::copy(p, &new_path)
                .map_err(|e2| Error::Internal(format!("cross-volume copy: {}", e2)))?;
            std::fs::remove_file(p)
                .map_err(|e2| Error::Internal(format!("cross-volume remove: {}", e2)))?;
            Ok(new_path.to_string_lossy().into_owned())
        }
        Err(e) => Err(Error::Internal(format!("move: {}", e))),
    }
}

#[tauri::command]
pub async fn rename_attached_file(path: String, new_name: String) -> Result<String, Error> {
    do_rename_attached_file(&path, &new_name)
}

#[tauri::command]
pub async fn move_attached_file(path: String, dest_dir: String) -> Result<String, Error> {
    do_move_attached_file(&path, &dest_dir)
}

#[tauri::command]
pub async fn read_attached_file(path: String) -> Result<Vec<u8>, Error> {
    std::fs::read(&path).map_err(|e| Error::Internal(format!("read {}: {}", path, e)))
}

// ─── `@`-mention file search ─────────────────────────────────────────────────

/// One result row of the `@`-mention file picker.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileMatch {
    /// File name only (e.g. `App.tsx`).
    pub name: String,
    /// Absolute path — what gets inserted into the composer as a chip.
    pub absolute_path: String,
    /// Path relative to the workspace root (or attached dir root). Used for the
    /// dropdown's two-line layout: `name` on top, `relative_path` below.
    pub relative_path: String,
    /// File extension (lowercased, no dot), or empty string for files without
    /// one. Drives the icon hint in the dropdown.
    pub extension: String,
}

/// Common heavy / generated / VCS directories the @-mention picker must **never**
/// descend into. A monorepo's `node_modules` can hold 100k+ files; walking them
/// would lock the popup. Keep this list small and well-justified — pruning
/// legitimate dirs would silently drop user files.
const MENTION_SKIP_DIRS: &[&str] = &[
    ".git", ".hg", ".svn",          // VCS
    "node_modules", "target",       // npm + cargo build outputs
    "dist", "build", "out",         // generic build outputs
    "__pycache__", ".venv", "venv", // Python
    ".idea", ".vscode",             // IDE state
    ".uclaw",                       // uClaw's own state in case it lands in a workspace
    ".DS_Store",                    // macOS junk
];

/// Search the session's workspace + attached_dirs for files matching `query`.
///
/// Powers the `@`-mention popover in the composer. Roots are resolved through
/// the workspace service (`agent_sessions` → `conversations` → active workspace);
/// the filesystem walk (skip-dir pruning, name match, sort, truncate) is a
/// non-DB concern and stays here. Returns up to `limit` (default 30) matches,
/// alphabetically sorted. Match rule: case-insensitive substring on the file
/// **name only** (users `@`-ref files by name, not path).
#[tauri::command]
pub async fn search_workspace_files_for_mention(
    state: State<'_, AppState>,
    session_id: String,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<WorkspaceFileMatch>, Error> {
    let limit = limit.unwrap_or(30).min(200);
    let q_lower = query.trim().to_lowercase();

    let roots: Vec<std::path::PathBuf> = {
        let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
        DbWorkspace.mention_search_roots(&conn, &session_id)
    };

    if roots.is_empty() {
        return Ok(vec![]);
    }

    // Walk all roots, prune skip dirs early, filter by query, accumulate.
    let mut matches: Vec<WorkspaceFileMatch> = Vec::new();
    for root in &roots {
        if !root.exists() {
            continue;
        }

        let walker = walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                // Prune at the directory level so we never descend into
                // node_modules / .git / etc.
                let name = match e.file_name().to_str() {
                    Some(n) => n,
                    None => return true,
                };
                if name.starts_with('.') && name != "." {
                    // Hidden files skipped unless the user explicitly attached
                    // this dir (root itself is a dotdir → allowed).
                    if e.depth() > 0 {
                        return false;
                    }
                }
                if e.file_type().is_dir() && MENTION_SKIP_DIRS.contains(&name) {
                    return false;
                }
                true
            });

        for entry in walker.flatten() {
            if matches.len() >= limit * 4 {
                // Hard cap on pre-filter results to bound CPU on huge trees; the
                // final sort+truncate happens below. *4 buffer so the sort has
                // room to pick the best limit entries.
                break;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let name_os = entry.file_name();
            let name = match name_os.to_str() {
                Some(n) => n,
                None => continue,
            };
            if !q_lower.is_empty() && !name.to_lowercase().contains(&q_lower) {
                continue;
            }
            let abs = entry.path().to_path_buf();
            let rel = abs
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| abs.to_string_lossy().into_owned());
            let extension = abs
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase())
                .unwrap_or_default();
            matches.push(WorkspaceFileMatch {
                name: name.to_string(),
                absolute_path: abs.to_string_lossy().into_owned(),
                relative_path: rel,
                extension,
            });
        }
    }

    // Sort: alphabetical case-insensitive by file name. Recency-aware ranking is
    // a future enhancement when we have per-file access stats.
    matches.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    matches.truncate(limit);
    Ok(matches)
}

// ─── Directory listing + uclaw.md (pure filesystem) ──────────────────────────

/// Lightweight directory listing for the Files tab. Reads `path` and returns a
/// flat list of immediate children as FileEntry-shaped objects. Hidden files
/// (dotfiles) and macOS `.DS_Store` are filtered so the panel matches Finder.
#[tauri::command]
pub async fn list_directory_entries(path: String) -> Result<Vec<serde_json::Value>, Error> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Ok(vec![]);
    }
    if !p.is_dir() {
        return Err(Error::InvalidInput(format!("not a directory: {}", path)));
    }
    let mut entries = tokio::fs::read_dir(&p).await.map_err(Error::Io)?;
    let mut out = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(Error::Io)? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let entry_path = entry.path();
        let meta = match entry.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_dir = meta.is_dir();
        let size = if is_dir { None } else { Some(meta.len()) };
        let extension = if is_dir {
            None
        } else {
            entry_path.extension().and_then(|s| s.to_str()).map(|s| s.to_string())
        };
        out.push(serde_json::json!({
            "name": name,
            "path": entry_path.to_string_lossy(),
            "isDirectory": is_dir,
            "isFile": !is_dir,
            "size": size,
            "extension": extension,
        }));
    }
    Ok(out)
}

#[tauri::command]
pub async fn read_workspace_uclaw_md(state: State<'_, AppState>) -> Result<String, Error> {
    let Some(root) = active_workspace_root(&state) else {
        return Ok(String::new());
    };
    let path = root.join("uclaw.md");
    match std::fs::read_to_string(&path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(Error::Internal(format!("read uclaw.md: {}", e))),
    }
}

#[tauri::command]
pub async fn write_workspace_uclaw_md(
    state: State<'_, AppState>,
    content: String,
) -> Result<(), Error> {
    let root = active_workspace_root(&state)
        .ok_or_else(|| Error::InvalidInput("No active workspace".into()))?;
    if !root.exists() {
        std::fs::create_dir_all(&root).map_err(Error::Io)?;
    }
    let path = root.join("uclaw.md");
    std::fs::write(&path, content).map_err(Error::Io)?;
    Ok(())
}

// ─── File upload into a workspace ────────────────────────────────────────────

/// Sanitize a user-provided filename so it can't escape the target dir or hide
/// as a dotfile. Returns the cleaned name. Truncates total length (incl.
/// extension) to 200 chars; preserves the extension on truncation.
fn sanitize_upload_filename(raw: &str) -> Result<String, Error> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidInput("filename is empty".into()));
    }
    if trimmed.contains("..") {
        return Err(Error::InvalidInput("filename contains '..'".into()));
    }
    let base = std::path::Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::InvalidInput("filename has no basename".into()))?;
    if base.starts_with('.') {
        return Err(Error::InvalidInput("dotfiles are not allowed".into()));
    }
    if base.len() <= 200 {
        return Ok(base.to_string());
    }
    // Truncate keeping the extension.
    let p = std::path::Path::new(base);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let ext = p.extension().and_then(|s| s.to_str());
    let ext_part = ext.map(|e| format!(".{}", e)).unwrap_or_default();
    let max_stem = 200usize.saturating_sub(ext_part.len());
    let truncated_stem: String = stem.chars().take(max_stem).collect();
    Ok(format!("{}{}", truncated_stem, ext_part))
}

/// Given a target dir + sanitized filename, return a path that doesn't collide
/// with anything on disk. Appends " (2)", " (3)", … before the extension.
/// Errors after 99 attempts.
fn next_available_path(
    dir: &std::path::Path,
    filename: &str,
) -> Result<std::path::PathBuf, Error> {
    let initial = dir.join(filename);
    if !initial.exists() {
        return Ok(initial);
    }
    let p = std::path::Path::new(filename);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let ext = p.extension().and_then(|s| s.to_str());
    for n in 2..=99u32 {
        let new_name = match ext {
            Some(e) => format!("{} ({}).{}", stem, n, e),
            None => format!("{} ({})", stem, n),
        };
        let candidate = dir.join(new_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(Error::Internal(format!(
        "could not find a free filename for '{}' after 99 attempts",
        filename
    )))
}

#[tauri::command]
pub async fn upload_workspace_file(
    state: State<'_, AppState>,
    workspace_id: String,
    filename: String,
    content: Vec<u8>,
) -> Result<String, Error> {
    let ws_path = {
        let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
        DbWorkspace.workspace_path(&conn, &workspace_id)?
    };
    let ws_path = ws_path
        .ok_or_else(|| Error::InvalidInput(format!("workspace '{}' has no path", workspace_id)))?;
    let ws_path = std::path::PathBuf::from(ws_path);

    tokio::fs::create_dir_all(&ws_path).await.map_err(Error::Io)?;

    let clean = sanitize_upload_filename(&filename)?;
    let target = next_available_path(&ws_path, &clean)?;
    tokio::fs::write(&target, &content).await.map_err(Error::Io)?;
    Ok(target.to_string_lossy().into_owned())
}

/// Native-drop variant of [`upload_workspace_file`]: read bytes from
/// `source_path` on disk, then sanitize / dedupe / write into the workspace
/// folder. Avoids roundtripping multi-MB files through IPC when the OS already
/// handed us a real path via onDragDropEvent.
#[tauri::command]
pub async fn copy_file_into_workspace(
    state: State<'_, AppState>,
    workspace_id: String,
    source_path: String,
) -> Result<String, Error> {
    let src = std::path::PathBuf::from(&source_path);
    if !src.exists() {
        return Err(Error::NotFound(format!("source file '{}'", source_path)));
    }
    let bytes = tokio::fs::read(&src).await.map_err(Error::Io)?;
    let raw_name = src
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::InvalidInput(format!("invalid filename in '{}'", source_path)))?;

    let ws_path = {
        let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
        DbWorkspace.workspace_path(&conn, &workspace_id)?
    };
    let ws_path = ws_path
        .ok_or_else(|| Error::InvalidInput(format!("workspace '{}' has no path", workspace_id)))?;
    let ws_path = std::path::PathBuf::from(ws_path);
    tokio::fs::create_dir_all(&ws_path).await.map_err(Error::Io)?;

    let clean = sanitize_upload_filename(raw_name)?;
    let target = next_available_path(&ws_path, &clean)?;
    tokio::fs::write(&target, &bytes).await.map_err(Error::Io)?;
    Ok(target.to_string_lossy().into_owned())
}

// ─── Path policy IPCs (delegate to SafetyManager) ────────────────────────────

#[tauri::command]
pub async fn list_always_allowed_paths(state: State<'_, AppState>) -> Result<Vec<String>, Error> {
    let mgr = state.safety_manager.read().await;
    Ok(mgr
        .list_always_allowed_paths()
        .iter()
        .map(|p| p.display().to_string())
        .collect())
}

#[tauri::command]
pub async fn add_always_allowed_path(state: State<'_, AppState>, path: String) -> Result<(), Error> {
    let p = std::path::PathBuf::from(&path);
    if !p.is_absolute() {
        return Err(Error::InvalidInput("path must be absolute".into()));
    }
    let mut mgr = state.safety_manager.write().await;
    mgr.add_always_allowed_path(p)
}

#[tauri::command]
pub async fn remove_always_allowed_path(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), Error> {
    let p = std::path::PathBuf::from(&path);
    let mut mgr = state.safety_manager.write().await;
    mgr.remove_always_allowed_path(&p)
}

#[tauri::command]
pub async fn list_session_allowed_paths(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<String>, Error> {
    let mgr = state.safety_manager.read().await;
    Ok(mgr
        .list_session_allowed_paths(&session_id)
        .iter()
        .map(|p| p.display().to_string())
        .collect())
}

#[tauri::command]
pub async fn promote_session_path_to_global(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<(), Error> {
    let p = std::path::PathBuf::from(&path);
    let mut mgr = state.safety_manager.write().await;
    mgr.promote_session_path_to_global(&session_id, &p)
}

// ─── File / path utilities (pure filesystem + OS) ────────────────────────────

/// Delete a single file by absolute path. Used by the Files tab's per-entry
/// trash button. Rejects relative paths and directories so a stray click can't
/// recursively wipe a folder. The caller confirms with the user first.
#[tauri::command]
pub async fn delete_workspace_file(path: String) -> Result<(), Error> {
    let p = std::path::PathBuf::from(&path);
    if !p.is_absolute() {
        return Err(Error::InvalidInput("path must be absolute".into()));
    }
    let meta = tokio::fs::metadata(&p).await.map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Error::NotFound(format!("file '{}'", path)),
        _ => Error::Io(e),
    })?;
    if meta.is_dir() {
        return Err(Error::InvalidInput(format!(
            "'{}' is a directory; this command only deletes files",
            path
        )));
    }
    tokio::fs::remove_file(&p).await.map_err(Error::Io)?;
    Ok(())
}

/// Lightweight type-of-path probe. Used by the frontend to decide whether a
/// native drag-drop payload is a folder (→ attach_workspace_directory) or a
/// file (→ upload_workspace_file). Returns false on missing path or any IO error.
#[tauri::command]
pub async fn path_is_directory(path: String) -> Result<bool, Error> {
    let p = std::path::PathBuf::from(&path);
    let meta = match tokio::fs::metadata(&p).await {
        Ok(m) => m,
        Err(_) => return Ok(false),
    };
    Ok(meta.is_dir())
}

/// Open the active workspace's `uclaw.md` in the OS-native default application
/// (file manager / text editor). Used by the Settings → 提示词 tab "在外部编辑器
/// 打开" button. Creates the file if absent so the editor opens an empty file
/// rather than failing.
#[tauri::command]
pub async fn open_workspace_uclaw_md_externally(state: State<'_, AppState>) -> Result<(), Error> {
    let root = active_workspace_root(&state)
        .ok_or_else(|| Error::InvalidInput("No active workspace".into()))?;
    if !root.exists() {
        std::fs::create_dir_all(&root).map_err(Error::Io)?;
    }
    let path = root.join("uclaw.md");
    if !path.exists() {
        // Touch with empty content so the OS opener has something to open.
        std::fs::write(&path, "").map_err(Error::Io)?;
    }

    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(target_os = "windows")]
    let cmd = "explorer";

    std::process::Command::new(cmd)
        .arg(&path)
        .spawn()
        .map_err(|e| Error::Internal(format!("open external editor: {}", e)))?;

    Ok(())
}

/// Reveal `path` in the host file manager.
///
/// macOS `open -R <file>` selects the file inside Finder; Windows
/// `explorer /select,"<file>"` does the equivalent. Linux has no universal
/// "select" affordance, so we open the parent directory. All branches are
/// best-effort: a spawn failure surfaces the error so the UI can toast.
#[tauri::command]
pub async fn reveal_path_in_file_manager(path: String) -> Result<(), Error> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(Error::InvalidInput(format!("path does not exist: {path}")));
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| Error::Internal(format!("reveal in Finder: {e}")))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{path}"))
            .spawn()
            .map_err(|e| Error::Internal(format!("reveal in Explorer: {e}")))?;
    }
    #[cfg(target_os = "linux")]
    {
        let dir = if p.is_dir() {
            p.clone()
        } else {
            p.parent().map(std::path::Path::to_path_buf).unwrap_or(p.clone())
        };
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| Error::Internal(format!("xdg-open: {e}")))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn read_default_prompts() -> Result<crate::ipc::DefaultPromptsResponse, Error> {
    use crate::agent::mode_prompts;
    use crate::safety::SafetyMode;
    Ok(crate::ipc::DefaultPromptsResponse {
        baseline: mode_prompts::KARPATHY_BASELINE.to_string(),
        mode_ask: mode_prompts::mode_addition(&SafetyMode::Ask).to_string(),
        mode_accept_edits: mode_prompts::mode_addition(&SafetyMode::AcceptEdits).to_string(),
        mode_plan: mode_prompts::mode_addition(&SafetyMode::Plan).to_string(),
        mode_bypass: mode_prompts::mode_addition(&SafetyMode::Yolo).to_string(),
    })
}

#[cfg(test)]
mod file_action_tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn create_tmp_file(dir: &std::path::Path, name: &str, content: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(content).unwrap();
        p
    }

    #[test]
    fn rename_attached_file_renames_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let original = create_tmp_file(tmp.path(), "old.txt", b"hello");
        let new_path = do_rename_attached_file(original.to_string_lossy().as_ref(), "new.txt").unwrap();
        assert!(!original.exists(), "old path should no longer exist");
        let new_pb = std::path::PathBuf::from(&new_path);
        assert!(new_pb.exists(), "new path should exist");
        assert_eq!(fs::read(&new_pb).unwrap(), b"hello");
    }

    #[test]
    fn rename_attached_file_refuses_to_clobber_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let original = create_tmp_file(tmp.path(), "old.txt", b"original");
        let _existing = create_tmp_file(tmp.path(), "existing.txt", b"do not lose me");
        let result = do_rename_attached_file(original.to_string_lossy().as_ref(), "existing.txt");
        assert!(result.is_err(), "rename onto existing file must error");
        assert!(original.exists(), "original file untouched after refused rename");
        assert_eq!(
            fs::read(tmp.path().join("existing.txt")).unwrap(),
            b"do not lose me",
            "existing file preserved"
        );
    }

    #[test]
    fn move_attached_file_moves_to_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tmp.path().join("src");
        let dst_dir = tmp.path().join("dst");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();
        let original = create_tmp_file(&src_dir, "f.txt", b"data");
        let new_path =
            do_move_attached_file(original.to_string_lossy().as_ref(), dst_dir.to_string_lossy().as_ref())
                .unwrap();
        assert!(!original.exists());
        let new_pb = std::path::PathBuf::from(&new_path);
        assert!(new_pb.starts_with(&dst_dir));
        assert_eq!(fs::read(&new_pb).unwrap(), b"data");
    }

    #[test]
    fn move_attached_file_refuses_to_clobber_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tmp.path().join("src");
        let dst_dir = tmp.path().join("dst");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();
        let original = create_tmp_file(&src_dir, "f.txt", b"data");
        let _existing = create_tmp_file(&dst_dir, "f.txt", b"existing data");
        let result =
            do_move_attached_file(original.to_string_lossy().as_ref(), dst_dir.to_string_lossy().as_ref());
        assert!(result.is_err(), "move onto existing file must error");
        assert!(original.exists(), "original file untouched after refused move");
        assert_eq!(
            fs::read(dst_dir.join("f.txt")).unwrap(),
            b"existing data",
            "existing file preserved"
        );
    }

    #[test]
    fn upload_workspace_file_sanitizes_filename() {
        assert!(sanitize_upload_filename("hello.txt").is_ok());
        assert_eq!(sanitize_upload_filename("hello.txt").unwrap(), "hello.txt".to_string());
        assert_eq!(sanitize_upload_filename("a/b/c.txt").unwrap(), "c.txt".to_string());
        assert!(matches!(sanitize_upload_filename("../escape.txt"), Err(Error::InvalidInput(_))));
        assert!(matches!(sanitize_upload_filename(".hidden"), Err(Error::InvalidInput(_))));
        assert!(matches!(sanitize_upload_filename(""), Err(Error::InvalidInput(_))));
        // Truncation: 250 chars + .png → 200 chars max, extension preserved.
        let long = "a".repeat(250) + ".png";
        let out = sanitize_upload_filename(&long).unwrap();
        assert!(out.len() <= 200);
        assert!(out.ends_with(".png"));
    }

    #[test]
    fn upload_workspace_file_dedupes_on_collision() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("logo.png"), b"a").unwrap();
        let p2 = next_available_path(dir.path(), "logo.png").unwrap();
        assert_eq!(p2.file_name().unwrap(), "logo (2).png");

        std::fs::write(dir.path().join("logo (2).png"), b"b").unwrap();
        let p3 = next_available_path(dir.path(), "logo.png").unwrap();
        assert_eq!(p3.file_name().unwrap(), "logo (3).png");
    }

    #[test]
    fn upload_workspace_file_no_extension_still_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("README"), b"a").unwrap();
        let p = next_available_path(dir.path(), "README").unwrap();
        assert_eq!(p.file_name().unwrap(), "README (2)");
    }
}

#[cfg(test)]
mod mention_file_search_tests {
    use super::MENTION_SKIP_DIRS;

    /// The skip list is load-bearing: missing a heavy dir means the `@`-mention
    /// popup hangs in a real codebase. This test pins the expected set so a
    /// future refactor that accidentally removes `node_modules` (etc.) fails loudly.
    #[test]
    fn skip_set_includes_load_bearing_heavy_dirs() {
        for required in ["node_modules", "target", ".git", "__pycache__", ".venv"] {
            assert!(
                MENTION_SKIP_DIRS.contains(&required),
                "skip set must include `{}` — removing it would make the @-mention picker hang on real codebases",
                required,
            );
        }
    }

    /// Skip list shouldn't accidentally include legitimate source dirs.
    #[test]
    fn skip_set_excludes_legitimate_source_dirs() {
        for legit in ["src", "components", "tests", "docs", "examples", "lib"] {
            assert!(
                !MENTION_SKIP_DIRS.contains(&legit),
                "skip set must NOT include `{}` — that would hide user files",
                legit,
            );
        }
    }
}
