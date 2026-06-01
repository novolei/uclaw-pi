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

    /// GET /api/v1/skills/search?q=&limit=
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SkillSummary>, MarketplaceError> {
        #[derive(serde::Deserialize)]
        struct Wrap { #[serde(default)] data: Vec<SkillSummary> }
        let q = urlencoding::encode(query);
        let limit = limit.clamp(1, 200);
        let w: Wrap = self.get_json(&format!("/api/v1/skills/search?q={q}&limit={limit}")).await?;
        Ok(w.data)
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
    async fn missing_key_errors_without_http() {
        let c = SkillsShClient::with_base("http://unused".into(), None);
        assert!(matches!(c.search("x", 5).await, Err(MarketplaceError::MissingApiKey)));
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
