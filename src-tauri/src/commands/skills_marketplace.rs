//! Thin Tauri commands over skills_marketplace (parse → service → map).
use tauri::State;
use crate::app::AppState;
use crate::error::Error;
use crate::skills_marketplace::{client::SkillsShClient, install, InstallScope, SkillSummary, SkillDetail, SkillAudit, MarketplaceError};

fn map_err(e: MarketplaceError) -> Error { Error::Internal(e.to_string()) }

fn read_api_key(state: &AppState) -> Option<String> {
    let conn = state.db.lock().ok()?;
    conn.query_row("SELECT value FROM settings WHERE key='skills_sh_api_key'", [], |r| r.get::<_, String>(0)).ok()
}

#[tauri::command]
pub async fn search_skill_marketplace(state: State<'_, AppState>, query: String, limit: Option<usize>) -> Result<Vec<SkillSummary>, Error> {
    let client = SkillsShClient::new(read_api_key(&state));
    client.search(&query, limit.unwrap_or(20)).await.map_err(map_err)
}

#[tauri::command]
pub async fn list_skill_marketplace(state: State<'_, AppState>, view: Option<String>, page: Option<usize>) -> Result<Vec<SkillSummary>, Error> {
    let client = SkillsShClient::new(read_api_key(&state));
    client.list(view.as_deref().unwrap_or("trending"), page.unwrap_or(0), 60).await.map_err(map_err)
}

#[tauri::command]
pub async fn get_skill_marketplace_detail(state: State<'_, AppState>, id: String) -> Result<SkillDetail, Error> {
    SkillsShClient::new(read_api_key(&state)).detail(&id).await.map_err(map_err)
}

#[tauri::command]
pub async fn get_skill_marketplace_audit(state: State<'_, AppState>, id: String) -> Result<SkillAudit, Error> {
    SkillsShClient::new(read_api_key(&state)).audit(&id).await.map_err(map_err)
}

#[tauri::command]
pub async fn install_skill_from_marketplace(
    state: State<'_, AppState>, id: String, scope: InstallScope, workspace: Option<String>,
) -> Result<String, Error> {
    let client = SkillsShClient::new(read_api_key(&state));
    let detail = client.detail(&id).await.map_err(map_err)?;
    let slug = install::flatten_slug(&id);
    let skills_root = state.data_dir.join("skills");
    let dir = install::write_skill_files(&skills_root, &slug, &detail).map_err(map_err)?;

    if scope == InstallScope::Workspace {
        if let Some(ws) = workspace.as_deref() {
            install::link_into_workspace(std::path::Path::new(ws), &slug, &dir).map_err(map_err)?;
            // TODO(P4): write the workspace tag into dir/SKILL.md activation.tags (frontmatter edit).
        }
    }
    {
        let mut reg = state.skills_registry.write().await;
        reg.add_scan_dir(dir.clone(), crate::skills::SkillProvenance::Marketplace);
        reg.discover();
    }
    if let Ok(conn) = state.db.lock() {
        let now = chrono::Utc::now().timestamp();
        if let Err(e) = install::record_install(&conn, &slug, &detail.hash, now) {
            tracing::warn!(slug = %slug, "skills_marketplace: V25 record_install failed: {e}");
        }
    }
    Ok(slug)
}
