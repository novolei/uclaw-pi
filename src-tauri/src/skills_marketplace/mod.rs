//! skills.sh marketplace integration — HTTP client + install service.
//! See docs/superpowers/specs/2026-06-01-skills-sh-marketplace-design.md.

pub mod client;
pub mod github;
pub mod install;
pub mod skillsmp;

use serde::{Deserialize, Serialize};

/// One row from skills.sh `/api/v1/skills` or `/skills/search`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSummary {
    pub id: String,        // "{source}/{slug}", e.g. "expo/skills/react-native"
    pub slug: String,
    pub name: String,
    pub source: String,    // "owner/repo"
    #[serde(default)]
    pub installs: u64,
    #[serde(rename = "sourceType", default)]
    pub source_type: String,
    #[serde(rename = "installUrl", default)]
    pub install_url: String,
    #[serde(default)]
    pub url: String,
    /// Short description (skillsmp.com returns one; skills.sh search does not → "").
    #[serde(default)]
    pub description: String,
}

/// One file from the detail endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFile {
    pub path: String,
    pub contents: String,
}

/// `/api/v1/skills/{source}/{slug}` detail (with inline files).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDetail {
    pub id: String,
    pub source: String,
    pub slug: String,
    #[serde(default)]
    pub hash: String,
    pub files: Vec<SkillFile>,
}

/// Audit verdict from `/api/v1/skills/audit/{source}/{slug}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillAudit {
    #[serde(default)]
    pub audits: Vec<SkillAuditEntry>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillAuditEntry {
    #[serde(default)]
    pub status: String,
    #[serde(rename = "riskLevel", default)]
    pub risk_level: String, // "LOW" | "MEDIUM" | "HIGH"
    #[serde(default)]
    pub summary: String,
}

/// Where to install a skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallScope {
    Global,
    Workspace,
}

/// Which marketplace backend a command targets. `Default` = `SkillsSh` for
/// back-compat (legacy callers that omit a provider keep their skills.sh
/// behavior, which needs no `source`); the FRONTEND provider selector defaults
/// to skillsmp (keyless) and always passes an explicit provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MarketplaceProvider {
    /// skills.sh /api/v1 — Bearer key required; inline-file detail + audit.
    #[default]
    SkillsSh,
    /// skillsmp.com /api/v1 — search-only, keyless; install/preview via GitHub.
    Skillsmp,
}

/// Errors surfaced to the UI.
#[derive(Debug, thiserror::Error)]
pub enum MarketplaceError {
    #[error("skills.sh API key not set")]
    MissingApiKey,
    #[error("skills.sh request failed: {0}")]
    Http(String),
    #[error("install failed: {0}")]
    Install(String),
    #[error("invalid skill id/path: {0}")]
    Invalid(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skill_summary_parses_search_item() {
        let v = serde_json::json!({
            "id": "expo/skills/react-native", "slug": "react-native", "name": "React Native",
            "source": "expo/skills", "installs": 3842, "sourceType": "github",
            "installUrl": "https://github.com/expo/skills", "url": "https://skills.sh/expo/skills/react-native"
        });
        let s: SkillSummary = serde_json::from_value(v).unwrap();
        assert_eq!(s.id, "expo/skills/react-native");
        assert_eq!(s.installs, 3842);
        assert_eq!(s.slug, "react-native");
    }
}
