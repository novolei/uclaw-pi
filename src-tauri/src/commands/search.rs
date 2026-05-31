//! Search-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31):
//!   - `search_conversations` has inline SQL (the flat UNION-of-branches over
//!     `messages*` / `agent_turns*` / `agent_messages*`) → lifted into
//!     [`crate::services::search_service`]; this just locks `state.db`, calls the
//!     service, and maps errors.
//!   - `search_workspace` and the file pass of `search_all` are `tokio::fs` walks
//!     over `state.data_dir` (no DB) → they stay thin here and own the
//!     [`search_files`] helper directly.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{SearchInput, SearchResult};
use crate::services::search_service::{DbSearch, SearchService};

/// Substring-search workspace file paths (fs walk, no DB). Capped at 20.
#[tauri::command]
pub async fn search_workspace(
    state: State<'_, AppState>,
    input: SearchInput,
) -> Result<Vec<SearchResult>, Error> {
    let workspace = state.data_dir.join("workspace");
    let query = input.query.to_lowercase();
    let mut results = Vec::new();

    search_files(&workspace, &workspace, &query, &mut results).await?;
    results.truncate(20);
    Ok(results)
}

/// Cross-domain conversation/message search — the flat UNION-of-branches lives in
/// the service; this just locks the DB and delegates.
#[tauri::command]
pub async fn search_conversations(
    state: State<'_, AppState>,
    input: SearchInput,
) -> Result<Vec<SearchResult>, Error> {
    let conn = state
        .db
        .lock()
        .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    DbSearch.search_conversations(&conn, &input.query, input.scope.as_deref())
}

/// Combined palette search: conversation/message hits (via the service) plus a
/// workspace-file pass, capped at 30.
#[tauri::command]
pub async fn search_all(
    state: State<'_, AppState>,
    input: SearchInput,
) -> Result<Vec<SearchResult>, Error> {
    let mut results = Vec::new();

    // Conversations + messages — DB branches via the service (unscoped).
    let conv_results = {
        let conn = state
            .db
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
        DbSearch.search_conversations(&conn, &input.query, None)?
    };
    results.extend(conv_results);

    // Workspace files.
    let workspace = state.data_dir.join("workspace");
    search_files(&workspace, &workspace, &input.query.to_lowercase(), &mut results).await?;

    results.truncate(30);
    Ok(results)
}

/// Recursively substring-match file paths under `root` against `query` (already
/// lowercased by the caller), pushing `file` hits into `results`. Skips dotfiles,
/// `node_modules`, and `target`. Stops once `results` reaches 30.
async fn search_files(
    root: &std::path::Path,
    base: &std::path::Path,
    query: &str,
    results: &mut Vec<SearchResult>,
) -> Result<(), Error> {
    let mut entries = tokio::fs::read_dir(root).await.map_err(Error::Io)?;
    while let Some(entry) = entries.next_entry().await.map_err(Error::Io)? {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }

        if path.is_dir() {
            Box::pin(search_files(&path, base, query, results)).await?;
        } else {
            let relative = path.strip_prefix(base).unwrap_or(&path);
            if relative.to_string_lossy().to_lowercase().contains(query) {
                let size = entry.metadata().await.map(|m| m.len()).unwrap_or(0);
                results.push(SearchResult {
                    id: uuid::Uuid::new_v4().to_string(),
                    title: relative.to_string_lossy().into(),
                    snippet: format!("{} bytes", size),
                    source: "file".into(),
                    source_id: relative.to_string_lossy().into(),
                    message_id: None,
                    workspace_id: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                });
            }
            if results.len() >= 30 {
                return Ok(());
            }
        }
    }
    Ok(())
}
