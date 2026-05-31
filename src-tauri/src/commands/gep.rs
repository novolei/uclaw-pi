//! GEP-gene-evolution-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), this domain has **no service**:
//! every command delegates to the [`crate::agent::gep::repository::GeneRepository`]
//! reached through the [`crate::proactive::service::ProactiveService`]
//! (`state.proactive_service`) — see the local [`get_gene_repo`] handle helper.
//! That repository *is* the logic holder; it owns its own SQLite access behind
//! `list_all_genes` / `list_active_genes` / `load_gene` / `list_capsules` /
//! `list_events_for_gene` / `retire_gene` / `update_gene_status`, so there is
//! **no inline `state.db` SQL to lift** and the JUDGMENT RULE resolves to a
//! thin move.
//!
//! Relocated verbatim from the legacy `tauri_commands.rs` god file (the
//! `// ─── GEP Gene Evolution Commands` section): the 5 `#[tauri::command]`s
//! plus every GEP-only response shape ([`GeneSummary`], [`GeneDetail`],
//! [`EvolutionTreeNode`], [`EvolutionTree`]) and the `get_gene_repo` handle
//! helper — none of which had any caller outside this domain. The unrelated
//! commands that shared the section tail in the god file
//! (`respond_plan_mode_suggest`, `get_app_health`, `get_memu_status`,
//! `memu_embed_text`) belong to other domains and were left behind.

use tauri::State;

use crate::agent::gep::types::GeneStatus;
use crate::app::AppState;
use crate::error::Error;

/// Lightweight gene summary for list display.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeneSummary {
    gene_id: String,
    asset_id: String,
    category: String,
    summary: String,
    version: String,
    status: String,
    created_at: i64,
    updated_at: i64,
    capsule_count: usize,
}

impl From<crate::agent::gep::types::Gene> for GeneSummary {
    fn from(g: crate::agent::gep::types::Gene) -> Self {
        Self {
            gene_id: g.gene_id,
            asset_id: g.asset_id,
            category: g.category.to_string(),
            summary: g.summary,
            version: g.version,
            status: format!("{:?}", g.status),
            created_at: g.created_at,
            updated_at: g.updated_at,
            capsule_count: 0,
        }
    }
}

/// Full gene detail with capsules and events.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeneDetail {
    gene: crate::agent::gep::types::Gene,
    capsules: Vec<crate::agent::gep::types::Capsule>,
    events: Vec<crate::agent::gep::types::EvolutionEvent>,
}

/// Evolution tree node.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EvolutionTreeNode {
    asset_id: String,
    version: String,
    parent_asset_id: Option<String>,
    created_at: i64,
    summary: String,
}

/// Evolution tree for a gene_id (all versions across asset_ids).
#[derive(Debug, Clone, serde::Serialize)]
pub struct EvolutionTree {
    gene_id: String,
    versions: Vec<EvolutionTreeNode>,
}

/// Helper: get the GeneRepository Arc from AppState.
async fn get_gene_repo(
    state: &AppState,
) -> Result<std::sync::Arc<std::sync::Mutex<crate::agent::gep::repository::GeneRepository>>, Error> {
    let proactive_svc = state.proactive_service.read().await;
    let pro_svc = proactive_svc
        .as_ref()
        .ok_or_else(|| Error::Internal("ProactiveService not initialized".into()))?;
    Ok(pro_svc.gene_repository())
}

/// List all genes, optionally filtered by status.
#[tauri::command]
pub async fn list_genes(
    state: State<'_, AppState>,
    status_filter: Option<String>,
) -> Result<Vec<GeneSummary>, Error> {
    let repo_arc = get_gene_repo(&state).await?;
    let repo = repo_arc.lock().map_err(|e| Error::Internal(format!("GeneRepository lock poisoned: {}", e)))?;
    let genes = match status_filter.as_deref() {
        Some("active") => repo
            .list_active_genes()
            .map_err(|e| Error::Internal(e.to_string()))?,
        _ => repo
            .list_all_genes()
            .map_err(|e| Error::Internal(e.to_string()))?,
    };
    let summaries: Vec<GeneSummary> = genes
        .into_iter()
        .map(|g| {
            let capsule_count = repo
                .list_capsules(&g.gene_id)
                .map(|c| c.len())
                .unwrap_or(0);
            let mut s = GeneSummary::from(g);
            s.capsule_count = capsule_count;
            s
        })
        .collect();
    Ok(summaries)
}

/// Get full detail for a gene (gene + capsules + events).
#[tauri::command]
pub async fn get_gene_detail(
    state: State<'_, AppState>,
    asset_id: String,
) -> Result<GeneDetail, Error> {
    let repo_arc = get_gene_repo(&state).await?;
    let repo = repo_arc.lock().map_err(|e| Error::Internal(format!("GeneRepository lock poisoned: {}", e)))?;
    let gene = repo
        .load_gene(&asset_id)
        .map_err(|e| Error::NotFound(format!("Gene not found: {}", e)))?;
    let capsules = repo
        .list_capsules(&gene.gene_id)
        .map_err(|e| Error::Internal(e.to_string()))?;
    let events = repo
        .list_events_for_gene(&gene.gene_id)
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(GeneDetail {
        gene,
        capsules,
        events,
    })
}

/// Get the evolution tree (version history) for a gene_id.
#[tauri::command]
pub async fn get_gene_evolution_tree(
    state: State<'_, AppState>,
    gene_id: String,
) -> Result<EvolutionTree, Error> {
    let repo_arc = get_gene_repo(&state).await?;
    let repo = repo_arc.lock().map_err(|e| Error::Internal(format!("GeneRepository lock poisoned: {}", e)))?;
    let all_genes = repo
        .list_all_genes()
        .map_err(|e| Error::Internal(e.to_string()))?;
    let versions: Vec<EvolutionTreeNode> = all_genes
        .into_iter()
        .filter(|g| g.gene_id == gene_id)
        .map(|g| EvolutionTreeNode {
            asset_id: g.asset_id.clone(),
            version: g.version.clone(),
            parent_asset_id: None,
            created_at: g.created_at,
            summary: g.summary.clone(),
        })
        .collect();
    let mut sorted = versions;
    sorted.sort_by_key(|v| v.created_at);
    for i in 1..sorted.len() {
        sorted[i].parent_asset_id = Some(sorted[i - 1].asset_id.clone());
    }
    Ok(EvolutionTree {
        gene_id,
        versions: sorted,
    })
}

/// Retire a gene (set status to Retired).
#[tauri::command]
pub async fn retire_gene(
    state: State<'_, AppState>,
    asset_id: String,
    reason: String,
) -> Result<(), Error> {
    let repo_arc = get_gene_repo(&state).await?;
    let mut repo = repo_arc.lock().map_err(|e| Error::Internal(format!("GeneRepository lock poisoned: {}", e)))?;
    repo.retire_gene(&asset_id, &reason)
        .map_err(|e| Error::Internal(e.to_string()))
}

/// Reactivate a retired gene (set status back to Active).
#[tauri::command]
pub async fn reactivate_gene(
    state: State<'_, AppState>,
    asset_id: String,
) -> Result<(), Error> {
    let repo_arc = get_gene_repo(&state).await?;
    let mut repo = repo_arc.lock().map_err(|e| Error::Internal(format!("GeneRepository lock poisoned: {}", e)))?;
    repo
        .update_gene_status(&asset_id, GeneStatus::Active)
        .map_err(|e| Error::Internal(e.to_string()))
}
