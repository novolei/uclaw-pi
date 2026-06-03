//! Bundle 21-D + 21-E — `skill_marketplace_search` and
//! `skill_install_from_marketplace` builtin tools.
//!
//! Together these wire uClaw into the skills.sh / GitHub agent-skill
//! ecosystem without requiring an external `npx skills` CLI on the
//! user's machine. The pair mirrors what `find-skills`
//! (vercel-labs/skills) and `skill-creator` (anthropics/skills) ask
//! their host agent to do:
//!
//! 1. `skill_marketplace_search` — discover candidate skills by
//!    keyword. Queries skills.sh via `crate::skills_marketplace::client`
//!    (P1 client). API key read from settings at registration.
//!
//! 2. `skill_install_from_marketplace` — fetch a specific
//!    `owner/repo/<path-to-skill>` from GitHub raw, validate the
//!    SKILL.md, write it under `~/.uclaw/skills/_marketplace/
//!    <owner>__<repo>__<slug>/`, and trigger registry rescan.
//!    Always requires user approval (network + foreign code +
//!    cross-session persistence).
//!
//! Skill-creator and find-skills SKILL.md files reference `npx
//! skills ...` commands; the bundled-into-uClaw versions point at
//! these two tools instead. See Bundle 21-C.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::json;
use tauri::Emitter;
use tokio::sync::RwLock;

use crate::agent::tools::tool::{
    ApprovalRequirement, Tool, ToolError, ToolErrorKind, ToolOutput,
};
use crate::skills::SkillsRegistry;
use crate::skills_marketplace::{client::SkillsShClient, skillsmp::SkillsmpClient, MarketplaceError, SkillSummary};

const USER_AGENT: &str = "uClaw/0.1";
const INSTALL_TIMEOUT_MS: u64 = 30_000;
const MAX_FILE_BYTES: usize = 512 * 1024; // 512 KB per file. Skills should be small.
const MAX_FILES_PER_SKILL: usize = 32;

// ───────────────────────────────────────────────────────────────────
// Tool 1 — skill_marketplace_search
// ───────────────────────────────────────────────────────────────────

pub struct SkillMarketplaceSearchTool {
    /// skills.sh API key (that provider requires it). Read from settings at registration.
    skills_sh_key: Option<String>,
    /// skillsmp.com API key — OPTIONAL (the anonymous tier works without it).
    skillsmp_key: Option<String>,
}
impl SkillMarketplaceSearchTool {
    #[must_use]
    pub fn new(skills_sh_key: Option<String>, skillsmp_key: Option<String>) -> Self {
        Self { skills_sh_key, skillsmp_key }
    }
}
impl Default for SkillMarketplaceSearchTool {
    fn default() -> Self { Self::new(None, None) }
}

#[async_trait]
impl Tool for SkillMarketplaceSearchTool {
    fn name(&self) -> &str {
        "skill_marketplace_search"
    }

    fn description(&self) -> &str {
        "Search a skill marketplace for installable skills matching a query. Default provider \"skills_sh\" (skills.sh — NO API key needed for search; a key only raises the rate limit). Pass provider=\"skillsmp\" for the skillsmp.com alternate (also keyless). Use when the user asks \"is there a skill for X\" or local skill_search finds nothing. Returns candidates {id, name, source, installs}. Do NOT install silently — surface the candidates so the USER can choose to Install (global or this workspace)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Free-text query describing what the user wants. E.g. \"lunar calendar conversion\", \"pdf form filling\", \"slack message formatting\"."
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results. Default 8, max 20.",
                    "default": 8,
                    "minimum": 1,
                    "maximum": 20
                },
                "provider": {
                    "type": "string",
                    "enum": ["skills_sh", "skillsmp"],
                    "description": "Marketplace to search. Default \"skills_sh\" (skills.sh — no API key needed for search). \"skillsmp\" is the alternate (skillsmp.com, also keyless).",
                    "default": "skills_sh"
                }
            },
            "required": ["query"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        // Search is read-only network egress. Auto-approve, same
        // tier as the web tool.
        ApprovalRequirement::Never
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let started = Instant::now();
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::kinded(ToolErrorKind::InvalidInput, "missing required `query`")
            })?
            .trim()
            .to_string();
        if query.is_empty() {
            return Err(ToolError::kinded(
                ToolErrorKind::InvalidInput,
                "`query` must not be empty",
            ));
        }
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n.clamp(1, 20) as usize)
            .unwrap_or(8);
        // Default skills.sh (keyless search; a key only raises its rate limit).
        // Unknown/missing → skills_sh.
        let provider = match params.get("provider").and_then(|v| v.as_str()) {
            Some("skillsmp") => "skillsmp",
            _ => "skills_sh",
        };

        let results = query_provider(&query, limit, provider, &self.skills_sh_key, &self.skillsmp_key).await?;
        let result_count = results.len();
        let elapsed = started.elapsed().as_millis() as u64;

        Ok(ToolOutput::new(
            json!({
                "ok": true,
                "query": query,
                "limit": limit,
                "provider": provider,
                "resultCount": result_count,
                "results": results,
                "note": if result_count == 0 {
                    "No marketplace matches. Try different keywords, or check local skills via skill_search."
                } else {
                    "These are installable. Surface them ({id, name, source, installs}) and let the USER click Install (global or this workspace) — do NOT install silently."
                },
            }),
            elapsed,
        ))
    }
}

/// Search the chosen marketplace for `query`. Returns the candidate rows the LLM
/// (and the P3 install card) consume. skillsmp is keyless; skills.sh needs a key.
async fn query_provider(
    query: &str,
    limit: usize,
    provider: &str,
    skills_sh_key: &Option<String>,
    skillsmp_key: &Option<String>,
) -> Result<Vec<serde_json::Value>, ToolError> {
    let mut results = match provider {
        "skills_sh" => SkillsShClient::new(skills_sh_key.clone()).search(query, limit).await,
        _ => SkillsmpClient::new(skillsmp_key.clone()).search(query, limit).await,
    }
    .map_err(|e| match e {
        MarketplaceError::MissingApiKey => ToolError::kinded(
            ToolErrorKind::InvalidInput,
            "skills.sh API key not set — add it in Settings, or use provider=\"skillsmp\" (no key needed). Local skill_search still works.",
        ),
        other => ToolError::kinded(ToolErrorKind::NetworkError, format!("marketplace search failed: {other}")),
    })?;
    // Rank by popularity so the most-installed/most-starred skills surface first
    // (skills.sh search has no sort param; skillsmp's `sortBy=stars` is only a
    // hint). Defensive + provider-agnostic — the LLM and the install card both
    // consume this order.
    sort_by_popularity(&mut results);
    Ok(to_result_json(&results))
}

/// Order candidates by popularity (`installs`; for skillsmp this is GitHub
/// stars) descending. Stable, so the API's relevance order is preserved among
/// equal-popularity ties.
fn sort_by_popularity(results: &mut [SkillSummary]) {
    results.sort_by(|a, b| b.installs.cmp(&a.installs));
}

/// Map `SkillSummary`s to the JSON rows surfaced to the LLM / install card.
/// `installUrl` is the install source (the githubUrl for skillsmp).
fn to_result_json(results: &[SkillSummary]) -> Vec<serde_json::Value> {
    results.iter().map(|s| json!({
        "id": s.id, "name": s.name, "source": s.source, "installs": s.installs,
        "installUrl": s.install_url, "description": s.description,
    })).collect()
}

fn truncate_for_error(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}

// ───────────────────────────────────────────────────────────────────
// Tool 2 — skill_install_from_marketplace
// ───────────────────────────────────────────────────────────────────

pub struct SkillInstallFromMarketplaceTool<R: tauri::Runtime = tauri::Wry> {
    pub registry: Arc<RwLock<SkillsRegistry>>,
    pub data_dir: PathBuf,
    pub app_handle: tauri::AppHandle<R>,
    pub conversation_id: String,
}

impl<R: tauri::Runtime> SkillInstallFromMarketplaceTool<R> {
    pub fn new(
        registry: Arc<RwLock<SkillsRegistry>>,
        data_dir: PathBuf,
        app_handle: tauri::AppHandle<R>,
        conversation_id: String,
    ) -> Self {
        Self {
            registry,
            data_dir,
            app_handle,
            conversation_id,
        }
    }
}

#[async_trait]
impl<R: tauri::Runtime> Tool for SkillInstallFromMarketplaceTool<R> {
    fn name(&self) -> &str {
        "skill_install_from_marketplace"
    }

    fn description(&self) -> &str {
        "Install a skill from a public GitHub repo into ~/.uclaw/skills/_marketplace/. Use when the user accepts a suggestion from skill_marketplace_search (or names a specific skill). The source string is `owner/repo/<path-to-skill-dir>` (the directory CONTAINING the SKILL.md, NOT the SKILL.md path itself). The install requires user approval because it fetches third-party code and persists it across all future sessions."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "source": {
                    "type": "string",
                    "description": "GitHub source: `owner/repo/<path-to-skill-dir>`. Examples: \"anthropics/skills/skill-creator\", \"vercel-labs/skills/find-skills\", \"obra/superpowers/brainstorming\"."
                },
                "ref": {
                    "type": "string",
                    "description": "Git ref (branch/tag/commit) to install from. Default \"main\".",
                    "default": "main"
                },
                "force": {
                    "type": "boolean",
                    "description": "If true, overwrites existing installation. Default false — refuses to clobber.",
                    "default": false
                }
            },
            "required": ["source"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        // Network fetch + third-party code + cross-session
        // persistence. Always ask the user.
        ApprovalRequirement::UnlessAutoApproved
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let started = Instant::now();

        let source = params
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::kinded(ToolErrorKind::InvalidInput, "missing required `source`")
            })?
            .trim()
            .to_string();

        let git_ref = params
            .get("ref")
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string();

        let force = params
            .get("force")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Parse `owner/repo/<path>` — at minimum 3 segments.
        let parts: Vec<&str> = source.split('/').collect();
        if parts.len() < 3 {
            return Err(ToolError::kinded(
                ToolErrorKind::InvalidInput,
                format!(
                    "source {source:?} must be in form `owner/repo/<skill-dir-path>` \
                     (e.g. \"anthropics/skills/skill-creator\")"
                ),
            ));
        }
        let owner = parts[0];
        let repo = parts[1];
        let skill_path = parts[2..].join("/");

        // Slug for local install dir. Format mirrors marketplace
        // recovery in AppState::new: `_marketplace/<owner>__<slug>`.
        // We append the path tail too so two skills from the same
        // repo don't collide.
        let path_tail = skill_path.replace('/', "__");
        let install_slug = format!("{owner}__{repo}__{path_tail}");
        let install_dir = self
            .data_dir
            .join("skills")
            .join("_marketplace")
            .join(&install_slug);

        if install_dir.exists() && !force {
            return Err(ToolError::kinded(
                ToolErrorKind::InvalidInput,
                format!(
                    "skill already installed at {}. Pass force=true to reinstall.",
                    install_dir.display()
                ),
            ));
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(INSTALL_TIMEOUT_MS))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| {
                ToolError::kinded(
                    ToolErrorKind::NetworkError,
                    format!("failed to build http client: {e}"),
                )
            })?;

        // List files in the skill dir via GitHub contents API.
        let list_url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
            owner,
            repo,
            urlencoding::encode(&skill_path),
            urlencoding::encode(&git_ref),
        );
        let resp = client
            .get(&list_url)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| {
                ToolError::kinded(
                    ToolErrorKind::NetworkError,
                    format!("github contents request failed: {e}"),
                )
            })?;
        if !resp.status().is_success() {
            return Err(ToolError::kinded(
                ToolErrorKind::UpstreamError,
                format!(
                    "github contents API returned {}: source={source} ref={git_ref}",
                    resp.status()
                ),
            ));
        }
        let listing: serde_json::Value = resp.json().await.map_err(|e| {
            ToolError::kinded(
                ToolErrorKind::ParseError,
                format!("github contents API returned malformed JSON: {e}"),
            )
        })?;

        // contents API returns a single object for files, an array
        // for directories. We expect the user to point at a dir.
        let entries: Vec<serde_json::Value> = match listing {
            serde_json::Value::Array(arr) => arr,
            other => {
                return Err(ToolError::kinded(
                    ToolErrorKind::InvalidInput,
                    format!(
                        "source {source:?} points at a file, not a directory. Pass \
                         the path to the skill DIRECTORY (the one containing SKILL.md). \
                         Got: {}",
                        truncate_for_error(&other.to_string(), 200)
                    ),
                ));
            }
        };

        if entries.len() > MAX_FILES_PER_SKILL {
            return Err(ToolError::kinded(
                ToolErrorKind::InvalidInput,
                format!(
                    "refusing to install skill with {} files (cap is {}). \
                     This is likely a misnamed source pointing at a large dir.",
                    entries.len(),
                    MAX_FILES_PER_SKILL,
                ),
            ));
        }

        // Verify the listing contains a SKILL.md.
        let has_skill_md = entries.iter().any(|e| {
            e.get("name").and_then(|v| v.as_str()) == Some("SKILL.md")
                && e.get("type").and_then(|v| v.as_str()) == Some("file")
        });
        if !has_skill_md {
            return Err(ToolError::kinded(
                ToolErrorKind::InvalidInput,
                format!(
                    "source {source:?} does not contain a SKILL.md. \
                     Listed entries: {:?}",
                    entries
                        .iter()
                        .filter_map(|e| e.get("name").and_then(|v| v.as_str()))
                        .collect::<Vec<_>>(),
                ),
            ));
        }

        // Fresh start: if force=true and dir exists, blow it away
        // before write. Bounded to within our own data dir.
        if install_dir.exists() && force {
            let _ = std::fs::remove_dir_all(&install_dir);
        }
        std::fs::create_dir_all(&install_dir).map_err(|e| {
            ToolError::kinded(
                ToolErrorKind::Other,
                format!("failed to create {}: {e}", install_dir.display()),
            )
        })?;

        let mut written_files: Vec<String> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        for entry in &entries {
            let entry_name = entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let entry_type = entry
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            // Reject path traversal in entry names — defense in
            // depth; GitHub shouldn't return these but we don't
            // trust the upstream blindly.
            if entry_name.contains("..") || entry_name.contains('/') {
                skipped.push(format!("{entry_name} (suspicious name)"));
                continue;
            }
            // Files only. Subdirectories aren't recursively
            // installed in this first cut — most skills are
            // shallow. Subdirs can be a follow-up.
            if entry_type != "file" {
                skipped.push(format!("{entry_name} (type={entry_type}, only files supported in v1)"));
                continue;
            }
            let download_url = entry
                .get("download_url")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if download_url.is_empty() {
                skipped.push(format!("{entry_name} (no download_url)"));
                continue;
            }
            // Fetch each file individually.
            let file_resp = client
                .get(download_url)
                .send()
                .await
                .map_err(|e| {
                    ToolError::kinded(
                        ToolErrorKind::NetworkError,
                        format!("failed to fetch {entry_name}: {e}"),
                    )
                })?;
            if !file_resp.status().is_success() {
                return Err(ToolError::kinded(
                    ToolErrorKind::UpstreamError,
                    format!(
                        "fetching {entry_name} returned {}",
                        file_resp.status()
                    ),
                ));
            }
            let bytes = file_resp.bytes().await.map_err(|e| {
                ToolError::kinded(
                    ToolErrorKind::NetworkError,
                    format!("failed to read {entry_name}: {e}"),
                )
            })?;
            if bytes.len() > MAX_FILE_BYTES {
                return Err(ToolError::kinded(
                    ToolErrorKind::PayloadTooLarge,
                    format!(
                        "{entry_name} is {} bytes (cap is {})",
                        bytes.len(),
                        MAX_FILE_BYTES
                    ),
                ));
            }
            let dest = install_dir.join(entry_name);
            std::fs::write(&dest, &bytes).map_err(|e| {
                ToolError::kinded(
                    ToolErrorKind::Other,
                    format!("failed to write {}: {e}", dest.display()),
                )
            })?;
            written_files.push(entry_name.to_string());
        }

        // Register the new install dir + rescan.
        let discovered = {
            let mut reg = self.registry.write().await;
            reg.add_scan_dir(
                install_dir.clone(),
                crate::skills::SkillProvenance::Marketplace,
            );
            reg.discover().len()
        };

        let _ = self.app_handle.emit(
            "agent:skill-installed",
            json!({
                "source": source,
                "ref": git_ref,
                "installPath": install_dir.display().to_string(),
                "filesWritten": written_files,
                "filesSkipped": skipped,
                "registryTotal": discovered,
                "conversationId": self.conversation_id,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }),
        );

        tracing::info!(
            source = %source,
            ref_ = %git_ref,
            install_path = %install_dir.display(),
            files_written = written_files.len(),
            files_skipped = skipped.len(),
            registry_total = discovered,
            "[Bundle 21-D] installed skill from marketplace"
        );

        let elapsed = started.elapsed().as_millis() as u64;
        Ok(ToolOutput::new(
            json!({
                "ok": true,
                "source": source,
                "ref": git_ref,
                "installPath": install_dir.display().to_string(),
                "filesWritten": written_files,
                "filesSkipped": skipped,
                "registryReloaded": true,
                "registryTotal": discovered,
                "message": format!(
                    "Installed {source:?} → {} ({} files, {} skipped). Registry \
                     reloaded — skill is immediately available to skill_search.",
                    install_dir.display(),
                    written_files.len(),
                    skipped.len(),
                ),
            }),
            elapsed,
        ))
    }
}

// ───────────────────────────────────────────────────────────────────
// Tests — Bundle 21-D / 21-E
// ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_for_error_short() {
        assert_eq!(truncate_for_error("hi", 100), "hi");
    }

    #[test]
    fn truncate_for_error_long() {
        let out = truncate_for_error(&"a".repeat(500), 50);
        // '…' is 3 UTF-8 bytes, so byte len = 50 + 3 = 53.
        assert_eq!(out.len(), 53);
        assert!(out.ends_with('…'));
    }

    #[tokio::test]
    async fn skill_marketplace_search_rejects_empty_query() {
        let tool = SkillMarketplaceSearchTool::new(None, None);
        let err = tool
            .execute(json!({ "query": "" }))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("empty"));
    }

    #[tokio::test]
    async fn skill_marketplace_search_rejects_missing_query() {
        let tool = SkillMarketplaceSearchTool::new(None, None);
        let err = tool.execute(json!({})).await.unwrap_err();
        assert!(format!("{err}").contains("query"));
    }

    #[test]
    fn sort_by_popularity_orders_desc_and_is_stable() {
        use crate::skills_marketplace::SkillSummary;
        let mk = |name: &str, installs: u64| SkillSummary {
            id: name.into(), slug: name.into(), name: name.into(), source: "o/r".into(),
            installs, source_type: "github".into(), install_url: String::new(),
            url: String::new(), description: String::new(),
        };
        // `b` and `c` tie at 100 → stable sort keeps b before c (input order).
        let mut v = vec![mk("a", 0), mk("b", 100), mk("c", 100), mk("d", 5)];
        super::sort_by_popularity(&mut v);
        assert_eq!(
            v.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            vec!["b", "c", "d", "a"],
        );
    }

    #[test]
    fn to_result_json_maps_fields() {
        use crate::skills_marketplace::SkillSummary;
        let s = SkillSummary { id: "expo/skills/rn".into(), slug: "rn".into(), name: "RN".into(),
            source: "expo/skills".into(), installs: 42, source_type: "github".into(),
            install_url: "https://github.com/expo/skills".into(), url: String::new(),
            description: String::new() };
        let rows = super::to_result_json(&[s]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], "expo/skills/rn");
        assert_eq!(rows[0]["installs"], 42);
        assert_eq!(rows[0]["source"], "expo/skills");
    }
}
