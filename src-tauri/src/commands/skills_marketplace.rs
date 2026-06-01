//! Thin Tauri commands over skills_marketplace (parse → service → map).
use tauri::State;
use crate::app::AppState;
use crate::error::Error;
use crate::skills_marketplace::{client::SkillsShClient, github, install, skillsmp::SkillsmpClient, InstallScope, MarketplaceProvider, SkillSummary, SkillDetail, SkillAudit, MarketplaceError};

fn map_err(e: MarketplaceError) -> Error {
    // Surface the real cause in the log — the UI necessarily shows a generic
    // message, so without this the actual error (401, decode, timeout…) is lost.
    tracing::warn!("skills_marketplace: {e}");
    Error::Internal(e.to_string())
}

/// Read the API key for a provider from `settings` (`skills_sh_api_key` /
/// `skillsmp_api_key`). skillsmp's key is optional (anonymous tier if absent).
fn read_api_key(state: &AppState, provider: MarketplaceProvider) -> Option<String> {
    let key_name = match provider {
        MarketplaceProvider::SkillsSh => "skills_sh_api_key",
        MarketplaceProvider::Skillsmp => "skillsmp_api_key",
    };
    let conn = state.db.lock().ok()?;
    conn.query_row("SELECT value FROM settings WHERE key=?1", [key_name], |r| r.get::<_, String>(0)).ok()
}

/// Produce a `SkillDetail` for a skill regardless of provider: skills.sh from its
/// detail endpoint; skillsmp from the row's `githubUrl` (`source`) via the GitHub
/// contents API. After this, install/preview are provider-agnostic. (The DB lock
/// inside `read_api_key` is released before any await.)
async fn provider_detail(
    state: &AppState,
    id: &str,
    provider: MarketplaceProvider,
    source: Option<&str>,
) -> Result<SkillDetail, Error> {
    match provider {
        MarketplaceProvider::SkillsSh => {
            SkillsShClient::new(read_api_key(state, provider)).detail(id).await.map_err(map_err)
        }
        MarketplaceProvider::Skillsmp => {
            let gh = source.ok_or_else(|| {
                Error::Internal("skillsmp install/detail needs a github source (install_url)".into())
            })?;
            github::fetch_github_skill(gh).await.map_err(map_err)
        }
    }
}

#[tauri::command]
pub async fn search_skill_marketplace(state: State<'_, AppState>, query: String, limit: Option<usize>, provider: Option<MarketplaceProvider>) -> Result<Vec<SkillSummary>, Error> {
    let provider = provider.unwrap_or_default();
    let limit = limit.unwrap_or(20);
    match provider {
        MarketplaceProvider::SkillsSh => SkillsShClient::new(read_api_key(&state, provider)).search(&query, limit).await.map_err(map_err),
        MarketplaceProvider::Skillsmp => SkillsmpClient::new(read_api_key(&state, provider)).search(&query, limit).await.map_err(map_err),
    }
}

#[tauri::command]
pub async fn list_skill_marketplace(state: State<'_, AppState>, view: Option<String>, page: Option<usize>, provider: Option<MarketplaceProvider>) -> Result<Vec<SkillSummary>, Error> {
    match provider.unwrap_or_default() {
        MarketplaceProvider::SkillsSh => SkillsShClient::new(read_api_key(&state, MarketplaceProvider::SkillsSh)).list(view.as_deref().unwrap_or("trending"), page.unwrap_or(0), 60).await.map_err(map_err),
        // skillsmp.com exposes no list/trending endpoint — only search.
        MarketplaceProvider::Skillsmp => Ok(Vec::new()),
    }
}

#[tauri::command]
pub async fn get_skill_marketplace_detail(state: State<'_, AppState>, id: String, provider: Option<MarketplaceProvider>, source: Option<String>) -> Result<SkillDetail, Error> {
    provider_detail(&state, &id, provider.unwrap_or_default(), source.as_deref()).await
}

#[tauri::command]
pub async fn get_skill_marketplace_audit(state: State<'_, AppState>, id: String, provider: Option<MarketplaceProvider>) -> Result<SkillAudit, Error> {
    match provider.unwrap_or_default() {
        MarketplaceProvider::SkillsSh => SkillsShClient::new(read_api_key(&state, MarketplaceProvider::SkillsSh)).audit(&id).await.map_err(map_err),
        // skillsmp.com has no audit endpoint → empty (UI shows 未审计).
        MarketplaceProvider::Skillsmp => Ok(SkillAudit { audits: Vec::new() }),
    }
}

/// Whether an installed marketplace skill has a newer version on skills.sh.
/// `true` iff the slug is tracked-installed (V25) AND its stored hash differs from
/// the latest detail hash. Not-installed ⇒ `false` (nothing to update).
#[tauri::command]
pub async fn check_skill_marketplace_update(state: State<'_, AppState>, id: String, provider: Option<MarketplaceProvider>, source: Option<String>) -> Result<bool, Error> {
    let detail = provider_detail(&state, &id, provider.unwrap_or_default(), source.as_deref()).await?;
    let slug = install::flatten_slug(&id);
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    let installed = install::read_install_version(&conn, &slug).ok().flatten();
    Ok(installed.is_some_and(|h| h != detail.hash))
}

/// Install a skill from the marketplace (skills.sh, or skillsmp.com via its
/// `source` githubUrl). `scope` = Global → written untagged, so it's
/// active in every workspace. `scope` = Workspace → the skill's SKILL.md and the
/// space's `skill_tags` both get the (normalized) `workspace_id` as a tag, so the
/// V19 `skill_matches_workspace` intersection activates it in that workspace.
/// (Per V19 semantics, giving a space its first tag turns on tag-filtering there.)
#[tauri::command]
pub async fn install_skill_from_marketplace(
    state: State<'_, AppState>, id: String, scope: InstallScope, workspace_id: Option<String>,
    provider: Option<MarketplaceProvider>, source: Option<String>,
) -> Result<String, Error> {
    let detail = provider_detail(&state, &id, provider.unwrap_or_default(), source.as_deref()).await?;
    let slug = install::flatten_slug(&id);
    let skills_root = state.data_dir.join("skills");
    let dir = install::write_skill_files(&skills_root, &slug, &detail).map_err(map_err)?;

    if scope == InstallScope::Workspace {
        if let Some(space_id) = workspace_id.as_deref() {
            // Tag string = normalized space id; the SAME tag goes on the skill and
            // the space so skill_matches_workspace (intersection) activates it here.
            let tag = space_id.trim().to_lowercase();
            if !tag.is_empty() {
                // 1) Tag the skill in its SKILL.md (best-effort: a skill without
                //    frontmatter stays untagged/global rather than failing install).
                if let Err(e) = install::add_activation_tag(&dir, &tag) {
                    tracing::warn!(slug = %slug, "skills_marketplace: workspace tag write skipped: {e}");
                }
                // 2) Add the tag to the space's skill_tags + resolve its path — one
                //    short SYNC DB section, no await while the lock is held.
                let ws_path: Option<String> = match state.db.lock() {
                    Ok(conn) => {
                        use crate::services::workspace_service::DbWorkspace;
                        let mut tags = DbWorkspace.get_skill_tags(&conn, space_id);
                        if !tags.iter().any(|t| t == &tag) {
                            tags.push(tag.clone());
                            match serde_json::to_string(&tags) {
                                Ok(json) => match DbWorkspace.set_skill_tags(&conn, space_id, &json) {
                                    // 0 rows ⇒ unknown space id: the skill got tagged but the
                                    // space did not, so it will NOT activate here. Warn so the
                                    // failed activation is diagnosable (was silently swallowed).
                                    Ok(0) => tracing::warn!(
                                        space_id = %space_id,
                                        "skills_marketplace: workspace skill_tags not stored (unknown space id) — skill will not activate in this workspace"
                                    ),
                                    Ok(_) => {}
                                    Err(e) => tracing::warn!(
                                        space_id = %space_id,
                                        "skills_marketplace: persist workspace skill_tags failed: {e} — skill may not activate"
                                    ),
                                },
                                Err(e) => tracing::warn!(
                                    "skills_marketplace: serialize skill_tags failed: {e}"
                                ),
                            }
                        }
                        conn.query_row(
                            "SELECT path FROM spaces WHERE id = ?1",
                            [space_id],
                            |r| r.get::<_, Option<String>>(0),
                        )
                        .ok()
                        .flatten()
                    }
                    Err(_) => None,
                };
                // 3) Best-effort symlink for file-tree visibility (only if the space
                //    has a filesystem path; tag is what actually activates).
                if let Some(p) = ws_path {
                    if let Err(e) =
                        install::link_into_workspace(std::path::Path::new(&p), &slug, &dir)
                    {
                        tracing::warn!(slug = %slug, "skills_marketplace: workspace symlink skipped: {e}");
                    }
                }
            }
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
