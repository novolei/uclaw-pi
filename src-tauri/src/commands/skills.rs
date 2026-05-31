//! Skills-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), almost every Skills command
//! delegates to the in-memory [`crate::skills::SkillsRegistry`] manager held in
//! `state.skills_registry` (an async `RwLock`) — list / toggle / discover /
//! reload / detail / match are pure registry reads or mutations with no SQL, so
//! the JUDGMENT RULE resolves to a thin move. `get_workspace_capabilities`
//! additionally reads `state.mcp_manager` to build the aggregate view; it has no
//! SQL either. `fork_skill_to_user` copies a Bundled skill into the user tier on
//! disk (no SQL) — the `copy_dir_recursive` helper was Skills-only and moves
//! here with it (plus its `fork_skill_tests` unit module).
//!
//! The **one** SQL touch in the domain — the per-workspace `spaces.skill_tags`
//! read inside `list_active_manifest_skills` — is lifted into
//! [`crate::services::skills_service::DbSkills`]; the surrounding manifest
//! computation + provenance enrichment stays here because it is bound to the
//! live `skills_registry` lock and `memory_graph_store`, neither of which is a
//! plain `&Connection`.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{
    McpServerInfo, SkillDetailResponse, SkillInfo, SkillMatchInput, SkillMatchResult,
    SkillParamInfo, SkillToggleInput, WorkspaceCapabilities,
};
use crate::services::skills_service::{DbSkills, SkillsService};

#[tauri::command]
pub async fn list_skills(state: State<'_, AppState>) -> Result<Vec<SkillInfo>, Error> {
    let registry = state.skills_registry.read().await;
    Ok(registry.list().into_iter().map(|s| SkillInfo {
        name: s.name.clone(),
        version: s.version.clone(),
        description: s.description.clone(),
        author: s.author.clone(),
        enabled: registry.is_enabled(&s.name),
        category: s.category.clone(),
        provenance: registry.get_loaded(&s.name).map(|l| l.provenance).unwrap_or(crate::skills::SkillProvenance::Project),
    }).collect())
}

/// Return the skills + MCP servers a workspace can call into.
///
/// The frontend `getWorkspaceCapabilities(slug)` bridge has been calling
/// this for a while but the backend handler never existed — the call hit
/// `.catch(() => ({mcpServers:[], skills:[]}))` on the frontend and the
/// LeftSidebar count badge + `mention-suggestions.tsx` dropdown silently
/// rendered empty. This wires up the real data source.
///
/// Skills and MCP servers are app-global today, so `slug` is accepted (to
/// preserve the frontend contract) but ignored. Per-workspace scoping is
/// future work — when it lands, swap in the workspace-filtered registries
/// here without changing the IPC surface.
#[tauri::command]
pub async fn get_workspace_capabilities(
    state: State<'_, AppState>,
    slug: Option<String>,
) -> Result<WorkspaceCapabilities, Error> {
    // slug is accepted for forward-compat but unused until skills/MCP
    // become workspace-scoped. Log it so an empty-result regression
    // narrows down quickly.
    tracing::debug!(slug = ?slug, "get_workspace_capabilities");

    // Reuse the same code paths as `list_mcp_servers` and `list_skills` so
    // the aggregate view stays consistent with the dedicated endpoints. A
    // future refactor can DRY these into shared internal functions; for
    // now duplication is cheaper than restructuring.
    let mcp_servers: Vec<McpServerInfo> = {
        let mgr = state.mcp_manager.read().await;
        let statuses: std::collections::HashMap<String, (crate::mcp::McpServerStatus, Option<String>)> = mgr
            .all_server_statuses()
            .into_iter()
            .map(|(id, st, err)| (id, (st, err)))
            .collect();
        mgr.all_servers().into_iter().map(|c| {
            let (status_enum, err) = statuses.get(&c.id)
                .cloned()
                .unwrap_or((crate::mcp::McpServerStatus::Disconnected, None));
            let status = match status_enum {
                crate::mcp::McpServerStatus::Disconnected => "disconnected",
                crate::mcp::McpServerStatus::Connecting => "connecting",
                crate::mcp::McpServerStatus::Connected => "connected",
                crate::mcp::McpServerStatus::Error => "error",
            };
            McpServerInfo {
                id: c.id.clone(),
                name: c.name.clone(),
                description: c.description.clone(),
                transport_type: c.transport_type.clone(),
                command: c.command.clone(),
                args: c.args.clone(),
                env: Some(c.env.clone()),
                url: c.url.clone(),
                enabled: c.enabled,
                auto_approve: c.auto_approve,
                error_message: err,
                status: status.into(),
            }
        }).collect()
    };

    let skills: Vec<SkillInfo> = {
        let registry = state.skills_registry.read().await;
        registry.list().into_iter().map(|s| SkillInfo {
            name: s.name.clone(),
            version: s.version.clone(),
            description: s.description.clone(),
            author: s.author.clone(),
            enabled: registry.is_enabled(&s.name),
            category: s.category.clone(),
            provenance: registry.get_loaded(&s.name).map(|l| l.provenance).unwrap_or(crate::skills::SkillProvenance::Project),
        }).collect()
    };

    Ok(WorkspaceCapabilities { mcp_servers, skills })
}

#[tauri::command]
pub async fn toggle_skill(state: State<'_, AppState>, input: SkillToggleInput) -> Result<bool, Error> {
    let mut registry = state.skills_registry.write().await;
    if input.enabled {
        Ok(registry.enable(&input.name))
    } else {
        Ok(registry.disable(&input.name))
    }
}

#[tauri::command]
pub async fn discover_skills(state: State<'_, AppState>) -> Result<Vec<SkillInfo>, Error> {
    let mut registry = state.skills_registry.write().await;
    let _names = registry.discover();
    Ok(registry.list().into_iter().map(|s| SkillInfo {
        name: s.name.clone(),
        version: s.version.clone(),
        description: s.description.clone(),
        author: s.author.clone(),
        enabled: registry.is_enabled(&s.name),
        category: s.category.clone(),
        provenance: registry.get_loaded(&s.name).map(|l| l.provenance).unwrap_or(crate::skills::SkillProvenance::Project),
    }).collect())
}

#[tauri::command]
pub async fn reload_skills(state: State<'_, AppState>) -> Result<Vec<SkillInfo>, Error> {
    let mut registry = state.skills_registry.write().await;
    let _names = registry.reload();
    Ok(registry.list().into_iter().map(|s| SkillInfo {
        name: s.name.clone(),
        version: s.version.clone(),
        description: s.description.clone(),
        author: s.author.clone(),
        enabled: registry.is_enabled(&s.name),
        category: s.category.clone(),
        provenance: registry.get_loaded(&s.name).map(|l| l.provenance).unwrap_or(crate::skills::SkillProvenance::Project),
    }).collect())
}

/// Copy a Bundled skill into the user's `~/.uclaw/skills/<name>/` so the
/// user can edit it freely. The bundled original is left in place but
/// "shadowed" by the user copy on the next discovery pass — `reload()`
/// runs automatically before this command returns.
///
/// Returns the destination path on success. Errors:
///   - skill not found
///   - skill is not Bundled (User skills are already editable; Project
///     skills are dev-only and shouldn't be forked)
///   - destination already exists (idempotency: refuse rather than
///     overwrite a user's existing fork)
#[tauri::command]
pub async fn fork_skill_to_user(
    state: State<'_, AppState>,
    name: String,
) -> Result<String, Error> {
    let mut registry = state.skills_registry.write().await;

    // Snapshot what we need before dropping the borrow — copying happens
    // outside the lock to keep the critical section short.
    let source_dir = {
        let loaded = registry
            .get_loaded(&name)
            .ok_or_else(|| Error::NotFound(format!("Skill '{}' not found", name)))?;
        if loaded.provenance != crate::skills::SkillProvenance::Bundled {
            return Err(Error::InvalidInput(format!(
                "Skill '{}' has provenance {:?} — only Bundled skills can be forked. \
                 User and Project skills are already editable in place.",
                name, loaded.provenance,
            )));
        }
        loaded.manifest.path.clone()
    };

    let dest_dir = uclaw_utils_home::uclaw_home_pathbuf()
        .map_err(|_| Error::Internal("Home directory unavailable".into()))?
        .join("skills")
        .join(&name);

    if dest_dir.exists() {
        return Err(Error::InvalidInput(format!(
            "A user fork of '{}' already exists at {}. Delete it first if you want a fresh fork.",
            name,
            dest_dir.display(),
        )));
    }

    copy_dir_recursive(&source_dir, &dest_dir)?;

    tracing::info!(
        skill = %name,
        from = %source_dir.display(),
        to = %dest_dir.display(),
        "Forked Bundled skill to User tier"
    );

    // Re-discover so the user copy registers and shadows the bundled one
    // immediately. `reload()` preserves the disabled set.
    registry.reload();

    Ok(dest_dir.display().to_string())
}

/// One row of the active-manifest debug panel.
///
/// Returned by `list_active_manifest_skills`. Mirrors the actual selection
/// `build_skills_manifest` performs (top-K by E3 ranking + strategy bias),
/// surfacing the structured rows instead of the formatted prompt string
/// so the Settings UI can render badges + per-skill rank/cited counts.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveManifestSkill {
    /// 1-based position in the injected manifest.
    pub rank: usize,
    pub name: String,
    pub summary: String,
    /// "bundled" | "user" | "project" | "learned" — for static/borrowed
    /// skills this resolves through the registry to the real disk tier.
    pub provenance: String,
    /// cited_count for learned skills; 0 for static (kept for symmetry).
    pub cited_count: u64,
}

/// Compute the skills manifest that **would be** injected for a session
/// right now, without actually running the agent loop. Powers the
/// Settings → 内置技能 → 活动技能 debug panel: users can see exactly
/// which skills the LLM sees + in what order.
///
/// Args mirror the agent-loop call site in `send_agent_message`:
///   - `space_id` — currently the agent loop hard-codes `"default"`;
///     the IPC accepts it for forward-compat with per-workspace scoping.
///   - `strategy` — optional `"repair" | "optimize" | "innovate"`;
///     unrecognized values fall back to Balanced (the same as the loop).
///   - `max_entries` — defaults to 30 to match the loop.
#[tauri::command]
pub async fn list_active_manifest_skills(
    state: State<'_, AppState>,
    space_id: Option<String>,
    strategy: Option<String>,
    max_entries: Option<usize>,
) -> Result<Vec<ActiveManifestSkill>, Error> {
    use crate::skills_manifest::{compute_active_manifest_entries, StrategyBias};

    let bias = match strategy.as_deref() {
        Some("repair") => StrategyBias::Repair,
        Some("optimize") => StrategyBias::Optimize,
        Some("innovate") => StrategyBias::Innovate,
        _ => StrategyBias::Balanced,
    };
    let sid = space_id.unwrap_or_else(|| "default".into());
    let limit = max_entries.unwrap_or(30);

    // Resolve workspace tags (V19+) so the debug panel reflects the
    // filtered set, not the raw global manifest. Empty / missing column
    // = no filter (matches send_agent_message's behavior).
    let workspace_tags: Vec<String> = {
        let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
        DbSkills.workspace_skill_tags(&conn, &sid)?
    };

    let registry = state.skills_registry.read().await;
    let store = &state.memory_graph_store;
    let entries = compute_active_manifest_entries(
        &registry,
        store,
        &sid,
        limit,
        bias,
        if workspace_tags.is_empty() { None } else { Some(workspace_tags.as_slice()) },
    );

    // Enrich "builtin" provenance to the real disk tier using the registry's
    // LoadedSkill data. "learned" passes through unchanged.
    let enriched: Vec<ActiveManifestSkill> = entries
        .into_iter()
        .enumerate()
        .map(|(idx, e)| {
            let provenance = if e.provenance == "learned" {
                "learned".to_string()
            } else {
                registry
                    .get_loaded(&e.name)
                    .map(|loaded| match loaded.provenance {
                        crate::skills::SkillProvenance::Bundled => "bundled",
                        crate::skills::SkillProvenance::User => "user",
                        crate::skills::SkillProvenance::Project => "project",
                        crate::skills::SkillProvenance::Marketplace => "marketplace",
                    })
                    .unwrap_or("project")
                    .to_string()
            };
            ActiveManifestSkill {
                rank: idx + 1,
                name: e.name,
                summary: e.summary,
                provenance,
                cited_count: e.cited_count,
            }
        })
        .collect();

    Ok(enriched)
}

/// Recursively copy a directory tree. Symlinks are intentionally ignored —
/// Bundled skills are vendored from the repo so they shouldn't contain any,
/// and silently following one could let a malicious skill exfiltrate
/// arbitrary files into the user's `~/.uclaw/` on fork.
pub(crate) fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<(), Error> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ty.is_file() {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_skill_detail(state: State<'_, AppState>, name: String) -> Result<SkillDetailResponse, Error> {
    let registry = state.skills_registry.read().await;
    let loaded = registry.get_loaded(&name)
        .ok_or_else(|| Error::NotFound(format!("Skill '{}' not found", name)))?;
    Ok(SkillDetailResponse {
        name: loaded.manifest.name.clone(),
        version: loaded.manifest.version.clone(),
        description: loaded.manifest.description.clone(),
        author: loaded.manifest.author.clone(),
        enabled: registry.is_enabled(&loaded.manifest.name),
        category: loaded.manifest.category.clone(),
        keywords: loaded.manifest.activation.keywords.clone(),
        tags: loaded.manifest.activation.tags.clone(),
        patterns: loaded.manifest.activation.patterns.clone(),
        parameters: loaded.manifest.parameters.iter().map(|p| SkillParamInfo {
            name: p.name.clone(),
            param_type: p.r#type.clone(),
            required: p.required,
            description: p.description.clone(),
            default: p.default.clone(),
        }).collect(),
        prompt_length: loaded.prompt_content.len(),
        path: loaded.manifest.path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn match_skills(state: State<'_, AppState>, input: SkillMatchInput) -> Result<Vec<SkillMatchResult>, Error> {
    let registry = state.skills_registry.read().await;
    let matched = registry.match_skills(&input.message);
    Ok(matched.into_iter().map(|s| {
        let score = crate::skills::score_skill(s, &input.message);
        let preview = if s.prompt_content.len() > 200 {
            format!("{}...", &s.prompt_content[..200])
        } else {
            s.prompt_content.clone()
        };
        SkillMatchResult {
            name: s.manifest.name.clone(),
            score,
            prompt_preview: preview,
        }
    }).collect())
}

#[cfg(test)]
mod fork_skill_tests {
    use super::copy_dir_recursive;

    /// Recursive copy must preserve directory shape and file content.
    /// Used by `fork_skill_to_user` to copy a Bundled skill (which can
    /// contain SKILL.md plus subdirectories like `scripts/` or
    /// `references/`) into the user's `~/.uclaw/skills/` tier.
    #[test]
    fn recursive_copy_preserves_tree() {
        let tmp = tempfile::TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");

        std::fs::create_dir_all(src.join("scripts")).unwrap();
        std::fs::create_dir_all(src.join("references")).unwrap();
        std::fs::write(src.join("SKILL.md"), "---\nname: x\n---\nbody").unwrap();
        std::fs::write(src.join("scripts/helper.sh"), "#!/bin/sh\necho hi").unwrap();
        std::fs::write(src.join("references/api.md"), "# API").unwrap();

        copy_dir_recursive(&src, &dst).expect("copy should succeed");

        assert!(dst.join("SKILL.md").exists());
        assert!(dst.join("scripts/helper.sh").exists());
        assert!(dst.join("references/api.md").exists());
        assert_eq!(
            std::fs::read_to_string(dst.join("SKILL.md")).unwrap(),
            "---\nname: x\n---\nbody",
            "file content must be copied verbatim"
        );
    }

    /// Symlinks are intentionally skipped — Bundled skills come from a
    /// vendored repo so they shouldn't contain any. Silently following
    /// a symlink could let a malicious skill exfiltrate arbitrary files
    /// into the user's ~/.uclaw/ via fork. Cross-platform: only run on
    /// Unix where symlink creation doesn't require admin.
    #[cfg(unix)]
    #[test]
    fn recursive_copy_skips_symlinks() {
        let tmp = tempfile::TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("real.md"), "real content").unwrap();
        std::os::unix::fs::symlink("/etc/passwd", src.join("evil")).unwrap();

        copy_dir_recursive(&src, &dst).expect("copy should succeed");

        assert!(dst.join("real.md").exists(), "real files should be copied");
        assert!(!dst.join("evil").exists(),
            "symlinks must be skipped — silently following could exfil arbitrary files");
    }

    /// Existing destination must be left intact and merged into (the IPC
    /// guards against existing-fork via a separate check; the helper is
    /// permissive — `create_dir_all` is idempotent on the root dir).
    #[test]
    fn recursive_copy_into_existing_dir_merges() {
        let tmp = tempfile::TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(src.join("new.md"), "new").unwrap();
        std::fs::write(dst.join("existing.md"), "existing").unwrap();

        copy_dir_recursive(&src, &dst).expect("copy should succeed");

        assert!(dst.join("new.md").exists());
        assert!(dst.join("existing.md").exists(),
            "pre-existing files in dst must survive");
    }
}
