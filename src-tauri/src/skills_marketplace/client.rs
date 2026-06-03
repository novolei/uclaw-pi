//! reqwest client for skills.sh /api/v1 (Bearer auth).

use std::time::Duration;
use super::{MarketplaceError, SkillSummary, SkillDetail, SkillAudit};

const DEFAULT_BASE: &str = "https://skills.sh";
const UA: &str = "uclaw-skills-marketplace";
const TIMEOUT_MS: u64 = 8000;

pub struct SkillsShClient {
    base: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl SkillsShClient {
    /// Production constructor: base = https://skills.sh.
    #[must_use]
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_base(DEFAULT_BASE.to_string(), api_key)
    }

    /// Test/override constructor.
    #[must_use]
    pub fn with_base(base: String, api_key: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(TIMEOUT_MS))
            .user_agent(UA)
            .build()
            .expect("reqwest client");
        Self { base, api_key, http }
    }

    fn key(&self) -> Result<&str, MarketplaceError> {
        self.api_key.as_deref().filter(|k| !k.is_empty()).ok_or(MarketplaceError::MissingApiKey)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, MarketplaceError> {
        let key = self.key()?;
        let url = format!("{}{}", self.base, path);
        let resp = self.http.get(&url).bearer_auth(key).send().await
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(MarketplaceError::Http(format!("status {}", resp.status())));
        }
        resp.json::<T>().await.map_err(|e| MarketplaceError::Http(e.to_string()))
    }

    /// Search skills.sh in whichever mode the key situation allows:
    /// - **keyed** → `GET /api/v1/skills/search` (Bearer); richer endpoint.
    /// - **anonymous** (no key) → `GET /api/search`; the public, keyless endpoint
    ///   (rate-limited ~60/hr). skills.sh requires NO API key for search — only
    ///   the `/api/v1/*` endpoints do.
    ///
    /// **Resilient to a bad key:** a keyed search rejected with 401/403 retries
    /// once anonymously, so a wrong/expired key never breaks search.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SkillSummary>, MarketplaceError> {
        if self.api_key.as_deref().is_some_and(|k| !k.is_empty()) {
            match self.search_keyed(query, limit).await {
                Err(MarketplaceError::Http(s)) if s.contains("401") || s.contains("403") => {
                    tracing::warn!("skills.sh: API key rejected ({s}); retrying anonymously");
                    self.search_anonymous(query, limit).await
                }
                other => other,
            }
        } else {
            self.search_anonymous(query, limit).await
        }
    }

    /// Keyed search: `GET /api/v1/skills/search?q=&limit=` (Bearer; `{data:[…]}`).
    async fn search_keyed(&self, query: &str, limit: usize) -> Result<Vec<SkillSummary>, MarketplaceError> {
        #[derive(serde::Deserialize)]
        struct Wrap { #[serde(default)] data: Vec<SkillSummary> }
        let q = urlencoding::encode(query);
        let limit = limit.clamp(1, 200);
        let w: Wrap = self.get_json(&format!("/api/v1/skills/search?q={q}&limit={limit}")).await?;
        Ok(w.data)
    }

    /// Anonymous search: `GET /api/search?q=&limit=` (no auth). Response shape
    /// `{skills:[{id,skillId,name,installs,source}]}`. `id` is the GitHub source
    /// (`owner/repo/path`) — carried into `install_url` so a keyless install can
    /// fetch the files straight from GitHub (see `commands::skills_marketplace`).
    async fn search_anonymous(&self, query: &str, limit: usize) -> Result<Vec<SkillSummary>, MarketplaceError> {
        #[derive(serde::Deserialize)]
        struct Wrap { #[serde(default)] skills: Vec<AnonRow> }
        #[derive(serde::Deserialize)]
        struct AnonRow {
            #[serde(default)] id: String,
            #[serde(rename = "skillId", default)] skill_id: String,
            #[serde(default)] name: String,
            #[serde(default)] installs: u64,
            #[serde(default)] source: String,
        }
        let q = urlencoding::encode(query);
        let limit = limit.clamp(1, 200);
        let url = format!("{}/api/search?q={q}&limit={limit}", self.base);
        let resp = self.http.get(&url).send().await
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(MarketplaceError::Http(format!("status {}", resp.status())));
        }
        let w: Wrap = resp.json().await.map_err(|e| MarketplaceError::Http(e.to_string()))?;
        Ok(w.skills.into_iter().map(|r| SkillSummary {
            slug: if r.skill_id.is_empty() { r.id.clone() } else { r.skill_id },
            name: r.name,
            source: r.source,
            installs: r.installs,
            source_type: "github".to_string(),
            install_url: r.id.clone(), // `owner/repo/path` → GitHub install source
            url: String::new(),
            description: String::new(),
            id: r.id,
        }).collect())
    }

    /// GET /api/v1/skills?view=&page=&per_page=
    pub async fn list(&self, view: &str, page: usize, per_page: usize) -> Result<Vec<SkillSummary>, MarketplaceError> {
        #[derive(serde::Deserialize)]
        struct Wrap { #[serde(default)] data: Vec<SkillSummary> }
        let view = match view { "trending" | "hot" | "all-time" => view, _ => "all-time" };
        let per_page = per_page.clamp(1, 500);
        let w: Wrap = self.get_json(&format!("/api/v1/skills?view={view}&page={page}&per_page={per_page}")).await?;
        Ok(w.data)
    }

    /// GET /api/v1/skills/{id}  (id = "source/slug")
    pub async fn detail(&self, id: &str) -> Result<SkillDetail, MarketplaceError> {
        self.get_json(&format!("/api/v1/skills/{id}")).await
    }

    /// GET /api/v1/skills/audit/{id}
    pub async fn audit(&self, id: &str) -> Result<SkillAudit, MarketplaceError> {
        self.get_json(&format!("/api/v1/skills/audit/{id}")).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn search_parses_data_array() {
        let mut server = mockito::Server::new_async().await;
        let m = server.mock("GET", "/api/v1/skills/search?q=react&limit=5")
            .match_header("authorization", "Bearer sk_test")
            .with_status(200)
            .with_body(r#"{"data":[{"id":"expo/skills/react-native","slug":"react-native","name":"React Native","source":"expo/skills","installs":3842}],"count":1}"#)
            .create_async().await;

        let c = SkillsShClient::with_base(server.url(), Some("sk_test".into()));
        let out = c.search("react", 5).await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].slug, "react-native");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn no_key_searches_anonymous_endpoint() {
        let mut server = mockito::Server::new_async().await;
        // Keyless search hits /api/search (no auth) and parses {skills:[…]}.
        let m = server.mock("GET", "/api/search?q=excel&limit=3")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_status(200)
            .with_body(r#"{"query":"excel","skills":[{"id":"claude-office-skills/skills/excel-automation","skillId":"excel-automation","name":"excel-automation","installs":9529,"source":"claude-office-skills/skills"}],"count":1}"#)
            .create_async().await;

        let c = SkillsShClient::with_base(server.url(), None);
        let out = c.search("excel", 3).await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].slug, "excel-automation");
        assert_eq!(out[0].source, "claude-office-skills/skills");
        assert_eq!(out[0].installs, 9529);
        // `id` (owner/repo/path) is carried into install_url for keyless GitHub install.
        assert_eq!(out[0].install_url, "claude-office-skills/skills/excel-automation");
        assert_eq!(out[0].source_type, "github");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn rejected_key_falls_back_to_anonymous_search() {
        let mut server = mockito::Server::new_async().await;
        // Keyed request → 401 (key rejected by /api/v1).
        let m401 = server.mock("GET", "/api/v1/skills/search?q=excel&limit=3")
            .match_header("authorization", "Bearer sk_bad")
            .with_status(401)
            .with_body(r#"{"error":"authentication_required"}"#)
            .create_async().await;
        // Retry → anonymous /api/search succeeds.
        let m200 = server.mock("GET", "/api/search?q=excel&limit=3")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_status(200)
            .with_body(r#"{"skills":[{"id":"o/r/excel","skillId":"excel","name":"excel","installs":7,"source":"o/r"}]}"#)
            .create_async().await;

        let c = SkillsShClient::with_base(server.url(), Some("sk_bad".into()));
        let out = c.search("excel", 3).await.unwrap();
        assert_eq!(out.len(), 1, "bad key falls back to anonymous results");
        assert_eq!(out[0].slug, "excel");
        m401.assert_async().await;
        m200.assert_async().await;
    }

    #[tokio::test]
    async fn detail_parses_files() {
        let mut server = mockito::Server::new_async().await;
        let m = server.mock("GET", "/api/v1/skills/expo/skills/react-native")
            .with_status(200)
            .with_body(r#"{"id":"expo/skills/react-native","source":"expo/skills","slug":"react-native","hash":"abc","files":[{"path":"SKILL.md","contents":"---\nname: rn\n---\nbody"}]}"#)
            .create_async().await;
        let c = SkillsShClient::with_base(server.url(), Some("sk_test".into()));
        let d = c.detail("expo/skills/react-native").await.unwrap();
        assert_eq!(d.files.len(), 1);
        assert_eq!(d.files[0].path, "SKILL.md");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn audit_parses_risk_level() {
        let mut server = mockito::Server::new_async().await;
        let m = server.mock("GET", "/api/v1/skills/audit/expo/skills/react-native")
            .with_status(200)
            .with_body(r#"{"audits":[{"status":"pass","riskLevel":"LOW","summary":"ok"}]}"#)
            .create_async().await;
        let c = SkillsShClient::with_base(server.url(), Some("sk_test".into()));
        let a = c.audit("expo/skills/react-native").await.unwrap();
        assert_eq!(a.audits.len(), 1);
        assert_eq!(a.audits[0].risk_level, "LOW");
        m.assert_async().await;
    }
}
