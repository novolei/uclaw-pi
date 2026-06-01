//! GitHub-sourced skill fetch. skillsmp.com returns no inline files — each result
//! carries a `githubUrl` (the skill's GitHub directory). This parses that URL and
//! pulls the skill's files via the GitHub contents API into a [`SkillDetail`] — the
//! same shape skills.sh's detail endpoint returns — so install + preview share one
//! provider-agnostic path (`write_skill_files`).
//!
//! Text-only (contents are `String`, matching skills.sh's `SkillFile`); binary
//! assets aren't supported, consistent with the existing pipeline. Same caps as the
//! legacy GitHub install tool: ≤32 files, ≤512 KB each, no path traversal, SKILL.md
//! required.

use super::{MarketplaceError, SkillDetail, SkillFile};

const UA: &str = "uClaw/0.1";
const TIMEOUT_MS: u64 = 30_000;
const MAX_FILE_BYTES: usize = 512 * 1024;
const MAX_FILES: usize = 32;

/// Parsed `github.com/{owner}/{repo}/tree/{branch}/{path}`.
struct GhRef {
    owner: String,
    repo: String,
    branch: String,
    path: String,
}

/// Parse a GitHub directory URL. Accepts the `/tree/` form skillsmp returns (and
/// the `/blob/` variant); rejects non-github or too-shallow URLs.
fn parse_github_url(url: &str) -> Result<GhRef, MarketplaceError> {
    let rest = url
        .trim()
        .strip_prefix("https://github.com/")
        .or_else(|| url.trim().strip_prefix("http://github.com/"))
        .ok_or_else(|| MarketplaceError::Invalid(format!("not a github.com URL: {url}")))?;
    let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    // owner / repo / ("tree"|"blob") / branch / path...
    if parts.len() < 5 || (parts[2] != "tree" && parts[2] != "blob") {
        return Err(MarketplaceError::Invalid(format!(
            "github URL must be .../{{owner}}/{{repo}}/tree/{{branch}}/{{path}}: {url}"
        )));
    }
    Ok(GhRef {
        owner: parts[0].to_string(),
        repo: parts[1].to_string(),
        branch: parts[3].to_string(),
        path: parts[4..].join("/"),
    })
}

/// Fetch a skill's files from its GitHub directory URL → [`SkillDetail`]. The
/// detail `hash` is the SKILL.md blob sha (a stable version id for update checks).
pub async fn fetch_github_skill(github_url: &str) -> Result<SkillDetail, MarketplaceError> {
    let gh = parse_github_url(github_url)?;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(TIMEOUT_MS))
        .user_agent(UA)
        .build()
        .map_err(|e| MarketplaceError::Http(e.to_string()))?;

    let list_url = format!(
        "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
        gh.owner,
        gh.repo,
        urlencoding::encode(&gh.path),
        urlencoding::encode(&gh.branch),
    );
    let resp = http
        .get(&list_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| MarketplaceError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(MarketplaceError::Http(format!(
            "github contents {} for {github_url}",
            resp.status()
        )));
    }
    let listing: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| MarketplaceError::Http(e.to_string()))?;
    let entries = listing
        .as_array()
        .ok_or_else(|| MarketplaceError::Invalid(format!("{github_url} is a file, not a skill dir")))?;
    if entries.len() > MAX_FILES {
        return Err(MarketplaceError::Invalid(format!(
            "skill dir has {} files (cap {MAX_FILES})",
            entries.len()
        )));
    }
    let has_skill_md = entries.iter().any(|e| {
        e.get("name").and_then(|v| v.as_str()) == Some("SKILL.md")
            && e.get("type").and_then(|v| v.as_str()) == Some("file")
    });
    if !has_skill_md {
        return Err(MarketplaceError::Invalid(format!("{github_url} has no SKILL.md")));
    }

    let mut files = Vec::new();
    let mut hash = String::new();
    for entry in entries {
        let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        let etype = entry.get("type").and_then(|v| v.as_str()).unwrap_or_default();
        // Flat files only; defense-in-depth on names (GitHub shouldn't return these).
        if etype != "file" || name.is_empty() || name.contains("..") || name.contains('/') {
            continue;
        }
        let download_url = entry.get("download_url").and_then(|v| v.as_str()).unwrap_or_default();
        if download_url.is_empty() {
            continue;
        }
        let fr = http
            .get(download_url)
            .send()
            .await
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        if !fr.status().is_success() {
            return Err(MarketplaceError::Http(format!("fetch {name}: {}", fr.status())));
        }
        let bytes = fr.bytes().await.map_err(|e| MarketplaceError::Http(e.to_string()))?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(MarketplaceError::Invalid(format!(
                "{name} is {} bytes (cap {MAX_FILE_BYTES})",
                bytes.len()
            )));
        }
        if name == "SKILL.md" {
            hash = entry
                .get("sha")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
        }
        files.push(SkillFile {
            path: name.to_string(),
            contents: String::from_utf8_lossy(&bytes).to_string(),
        });
    }

    Ok(SkillDetail {
        id: github_url.to_string(),
        source: format!("{}/{}", gh.owner, gh.repo),
        slug: format!("{}__{}__{}", gh.owner, gh.repo, gh.path.replace('/', "__")),
        hash,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tree_url() {
        let gh =
            parse_github_url("https://github.com/TuYv/ccpm/tree/main/preset-registry/skills/pdf").unwrap();
        assert_eq!(gh.owner, "TuYv");
        assert_eq!(gh.repo, "ccpm");
        assert_eq!(gh.branch, "main");
        assert_eq!(gh.path, "preset-registry/skills/pdf");
    }

    #[test]
    fn rejects_non_github_and_too_shallow() {
        assert!(parse_github_url("https://example.com/x/y/tree/main/z").is_err());
        assert!(parse_github_url("https://github.com/owner/repo").is_err()); // no /tree/branch/path
        assert!(parse_github_url("https://github.com/owner/repo/tree/main").is_err()); // no path
    }
}
