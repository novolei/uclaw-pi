//! Artifact / file-tree Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), this domain has **no service**:
//! every command is filesystem CRUD over a `state`-derived path — either the
//! app `workspace_root` (`list_artifacts` / `read_artifact` / `write_artifact` /
//! `delete_artifact`, the legacy flat view) or a per-space workspace dir
//! (`state.data_dir/spaces/<id>/workspace`, the tree view). The reads/writes go
//! through `tokio::fs` and the Tauri-independent helpers in
//! [`crate::workspace`] (`list_artifact_tree`, `load_artifact_children`,
//! `mime_from_path`). There is **no `state.db` SQL** anywhere in the domain, so
//! the JUDGMENT RULE resolves to a straight thin move.
//!
//! `build_artifact_tree` (the recursive walker behind the flat `list_artifacts`)
//! was Artifact-only and moves here as a module-private helper. A separate copy
//! lives in `api/handlers/artifacts.rs` for the HTTP surface — that one is
//! independent and untouched.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{
    ArtifactContentResponse, ArtifactNode, ArtifactTreeNodeResponse, CreateArtifactInput,
    DetectFileTypeResponse, ListArtifactTreeInput, LoadArtifactChildrenInput, MoveArtifactInput,
    ReadArtifactInput, RenameArtifactInput, WriteArtifactInput,
};

#[tauri::command]
pub async fn list_artifacts(state: State<'_, AppState>) -> Result<Vec<ArtifactNode>, Error> {
    let workspace = state.workspace_root.clone();
    build_artifact_tree(&workspace, &workspace).await
}

#[tauri::command]
pub async fn read_artifact(state: State<'_, AppState>, input: ReadArtifactInput) -> Result<ArtifactContentResponse, Error> {
    let workspace = state.workspace_root.clone();
    let full_path = workspace.join(&input.path);
    let content = tokio::fs::read_to_string(&full_path).await
        .map_err(|e| Error::Io(e))?;
    let size = content.len() as u64;
    Ok(ArtifactContentResponse { path: input.path, content, size })
}

#[tauri::command]
pub async fn write_artifact(state: State<'_, AppState>, input: WriteArtifactInput) -> Result<ArtifactContentResponse, Error> {
    let workspace = state.workspace_root.clone();
    let full_path = workspace.join(&input.path);
    if let Some(parent) = full_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| Error::Io(e))?;
    }
    tokio::fs::write(&full_path, &input.content).await.map_err(|e| Error::Io(e))?;
    let size = input.content.len() as u64;
    Ok(ArtifactContentResponse { path: input.path, content: input.content, size })
}

#[tauri::command]
pub async fn delete_artifact(state: State<'_, AppState>, path: String) -> Result<bool, Error> {
    let workspace = state.workspace_root.clone();
    let full_path = workspace.join(&path);
    tokio::fs::remove_file(&full_path).await.map_err(|e| Error::Io(e))?;
    Ok(true)
}

// ─── Enhanced Artifact Tree Commands ─────────────────────────────────────

#[tauri::command]
pub async fn list_artifacts_tree(
    state: State<'_, AppState>,
    input: ListArtifactTreeInput,
) -> Result<Vec<ArtifactTreeNodeResponse>, Error> {
    let space_dir = state.data_dir.join("spaces").join(&input.space_id).join("workspace");
    if !space_dir.exists() {
        tokio::fs::create_dir_all(&space_dir).await.map_err(Error::Io)?;
    }
    crate::workspace::list_artifact_tree(&space_dir, &input.path).await
}

#[tauri::command]
pub async fn load_artifact_children(
    state: State<'_, AppState>,
    input: LoadArtifactChildrenInput,
) -> Result<Vec<ArtifactTreeNodeResponse>, Error> {
    let space_dir = state.data_dir.join("spaces").join(&input.space_id).join("workspace");
    crate::workspace::load_artifact_children(&space_dir, &input.path).await
}

// ─── Extended Artifact Commands ─────────────────────────────────────────

#[tauri::command]
pub async fn create_artifact(
    state: State<'_, AppState>,
    input: CreateArtifactInput,
) -> Result<ArtifactTreeNodeResponse, Error> {
    let space_dir = state.data_dir.join("spaces").join(&input.space_id).join("workspace");
    let clean = input.path.trim_start_matches('/');
    let full_path = space_dir.join(clean);

    if input.is_dir.unwrap_or(false) {
        tokio::fs::create_dir_all(&full_path).await.map_err(Error::Io)?;
    } else {
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(Error::Io)?;
        }
        tokio::fs::write(&full_path, input.content.unwrap_or_default())
            .await
            .map_err(Error::Io)?;
    }

    let name = full_path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    let metadata = tokio::fs::metadata(&full_path).await.map_err(Error::Io)?;
    let parent_path = std::path::Path::new(clean).parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    Ok(ArtifactTreeNodeResponse {
        path: clean.to_string(),
        name,
        is_dir: metadata.is_dir(),
        parent_path,
        size_bytes: if metadata.is_dir() { None } else { Some(metadata.len()) },
        mime_type: if metadata.is_dir() { None } else { crate::workspace::mime_from_path(&full_path) },
        modified_at: metadata.modified().ok().map(|t| {
            chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()
        }),
        children: if metadata.is_dir() { Some(vec![]) } else { None },
    })
}

#[tauri::command]
pub async fn rename_artifact(
    state: State<'_, AppState>,
    input: RenameArtifactInput,
) -> Result<bool, Error> {
    let space_dir = state.data_dir.join("spaces").join(&input.space_id).join("workspace");
    let old_path = space_dir.join(input.old_path.trim_start_matches('/'));
    let new_path = space_dir.join(input.new_path.trim_start_matches('/'));

    if !old_path.exists() {
        return Err(Error::NotFound(format!("File not found: {}", input.old_path)));
    }

    tokio::fs::rename(&old_path, &new_path).await.map_err(Error::Io)?;
    Ok(true)
}

#[tauri::command]
pub async fn move_artifact(
    state: State<'_, AppState>,
    input: MoveArtifactInput,
) -> Result<bool, Error> {
    let space_dir = state.data_dir.join("spaces").join(&input.space_id).join("workspace");
    let src = space_dir.join(input.src_path.trim_start_matches('/'));
    let dest = space_dir.join(input.dest_path.trim_start_matches('/'));

    if !src.exists() {
        return Err(Error::NotFound(format!("File not found: {}", input.src_path)));
    }

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(Error::Io)?;
    }

    tokio::fs::rename(&src, &dest).await.map_err(Error::Io)?;
    Ok(true)
}

#[tauri::command]
pub async fn delete_artifact_recursive(
    state: State<'_, AppState>,
    space_id: String,
    path: String,
) -> Result<bool, Error> {
    let space_dir = state.data_dir.join("spaces").join(&space_id).join("workspace");
    let clean = path.trim_start_matches('/');
    let full_path = space_dir.join(clean);

    if !full_path.exists() {
        return Err(Error::NotFound(format!("File not found: {}", path)));
    }

    if full_path.is_dir() {
        tokio::fs::remove_dir_all(&full_path).await.map_err(Error::Io)?;
    } else {
        tokio::fs::remove_file(&full_path).await.map_err(Error::Io)?;
    }

    Ok(true)
}

#[tauri::command]
pub async fn detect_file_type(
    path: String,
) -> Result<DetectFileTypeResponse, Error> {
    let ext = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let (mime_type, category) = match ext.as_str() {
        "ts" | "tsx" | "js" | "jsx" | "rs" | "py" | "go" | "java" | "c" | "cpp" | "h" | "css" | "scss" | "less" | "json" | "svelte" | "sql" | "sh" | "bash" | "zsh" | "yaml" | "yml" | "toml" | "xml" | "swift" | "kt" | "rb" | "php" | "r" | "dart" | "lua" => {
            (format!("text/{}", if ext == "rs" { "x-rust" } else if ext == "py" { "x-python" } else if ext == "go" { "x-go" } else if ext == "svelte" { "x-svelte" } else if ext == "sh" || ext == "bash" || ext == "zsh" { "x-shellscript" } else if ext == "sql" { "x-sql" } else if ext == "yaml" || ext == "yml" { "yaml" } else if ext == "toml" { "toml" } else { &ext }), "code")
        },
        "html" | "htm" => ("text/html".to_string(), "html"),
        "md" | "markdown" => ("text/markdown".to_string(), "markdown"),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" | "ico" => {
            (format!("image/{}", if ext == "jpg" { "jpeg" } else if ext == "svg" { "svg+xml" } else { &ext }), "image")
        },
        "txt" | "log" | "csv" => ("text/plain".to_string(), "text"),
        _ => ("application/octet-stream".to_string(), "binary"),
    };

    Ok(DetectFileTypeResponse { mime_type, category: category.to_string() })
}

/// Recursively build the flat-view artifact tree from a workspace root,
/// reporting paths relative to `base`. Skips dotfiles, `node_modules`, and
/// `target`. Directories sort before files, then case-insensitively by name.
///
/// Artifact-only helper behind [`list_artifacts`]. A structurally similar copy
/// lives in `api/handlers/artifacts.rs` for the HTTP surface — that one is
/// independent.
async fn build_artifact_tree(root: &std::path::PathBuf, base: &std::path::PathBuf) -> Result<Vec<ArtifactNode>, Error> {
    let mut nodes = Vec::new();
    let mut entries = tokio::fs::read_dir(root).await.map_err(|e| Error::Io(e))?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| Error::Io(e))? {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
        let relative = path.strip_prefix(base).unwrap_or(&path);

        if name.starts_with('.') || name == "node_modules" || name == "target" { continue; }

        if path.is_dir() {
            let children = Box::pin(build_artifact_tree(&path, base)).await?;
            nodes.push(ArtifactNode {
                name: name.into(),
                path: relative.to_string_lossy().into(),
                is_dir: true,
                size: None,
                children: if children.is_empty() { None } else { Some(children) },
            });
        } else {
            let size = entry.metadata().await.map(|m| m.len()).ok();
            nodes.push(ArtifactNode {
                name: name.into(),
                path: relative.to_string_lossy().into(),
                is_dir: false,
                size,
                children: None,
            });
        }
    }
    nodes.sort_by(|a, b| {
        if a.is_dir != b.is_dir { b.is_dir.cmp(&a.is_dir) }
        else { a.name.to_lowercase().cmp(&b.name.to_lowercase()) }
    });
    Ok(nodes)
}
