//! reqwest client for skillsmp.com /api/v1 — **search only** (the API exposes no
//! detail/audit/list; install + preview go via the result's `githubUrl`, see
//! `super::github`). The Bearer key is **optional** (anonymous = 50/day, keyed =
//! 500/day); a missing key just means the anonymous tier.

use std::time::Duration;

use super::{MarketplaceError, SkillSummary};

const DEFAULT_BASE: &str = "https://skillsmp.com";
const UA: &str = "uclaw-skills-marketplace";
const TIMEOUT_MS: u64 = 8000;

pub struct SkillsmpClient {
    base: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl SkillsmpClient {
    /// Production constructor: base = https://skillsmp.com.
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

    /// GET /api/v1/skills/search?q=&limit= — anonymous OK; a non-empty key raises
    /// the rate limit. **Resilient to a bad key:** since skillsmp search is keyless,
    /// a rejected key (401/403) retries once anonymously rather than failing — so a
    /// wrong/expired key never breaks search. Normalizes rows → `SkillSummary`
    /// (`install_url` = the row's `githubUrl`, the install/preview source).
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SkillSummary>, MarketplaceError> {
        let has_key = self.api_key.as_deref().is_some_and(|k| !k.is_empty());
        match self.search_inner(query, limit, has_key).await {
            Err(MarketplaceError::Http(s))
                if has_key && (s.contains("401") || s.contains("403")) =>
            {
                tracing::warn!("skillsmp: API key rejected ({s}); retrying anonymously");
                self.search_inner(query, limit, false).await
            }
            other => other,
        }
    }

    /// One search request; `with_key=false` forces the anonymous tier (the
    /// rejected-key fallback path).
    async fn search_inner(
        &self,
        query: &str,
        limit: usize,
        with_key: bool,
    ) -> Result<Vec<SkillSummary>, MarketplaceError> {
        let q = urlencoding::encode(query);
        let limit = limit.clamp(1, 100);
        // `sortBy=stars` asks skillsmp to rank by popularity (the API's supported
        // sort; see the skillsmp marketplace design spec). The backend tool also
        // sorts defensively, so order is correct even if the API ignores this.
        let url = format!("{}/api/v1/skills/search?q={q}&limit={limit}&sortBy=stars", self.base);
        let mut req = self.http.get(&url);
        if with_key {
            if let Some(key) = self.api_key.as_deref().filter(|k| !k.is_empty()) {
                req = req.bearer_auth(key);
            }
        }
        let resp = req
            .send()
            .await
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(MarketplaceError::Http(format!("status {}", resp.status())));
        }
        let body: SearchResponse = resp
            .json()
            .await
            .map_err(|e| MarketplaceError::Http(e.to_string()))?;
        Ok(body.data.skills.into_iter().map(SkillsmpRow::into_summary).collect())
    }
}

#[derive(serde::Deserialize, Default)]
struct SearchResponse {
    #[serde(default)]
    data: SearchData,
}
#[derive(serde::Deserialize, Default)]
struct SearchData {
    #[serde(default)]
    skills: Vec<SkillsmpRow>,
}

/// One row from skillsmp's search; mapped to the shared `SkillSummary`.
#[derive(serde::Deserialize)]
struct SkillsmpRow {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    description: String,
    #[serde(rename = "githubUrl", default)]
    github_url: String,
    #[serde(rename = "skillUrl", default)]
    skill_url: String,
    #[serde(default)]
    stars: u64,
}

impl SkillsmpRow {
    fn into_summary(self) -> SkillSummary {
        SkillSummary {
            slug: self.id.clone(), // skillsmp ids are already safe kebab slugs
            id: self.id,
            name: self.name,
            source: self.author,
            installs: self.stars, // stars is the closest "popularity" analog
            source_type: "github".to_string(),
            install_url: self.github_url, // the install/preview source
            url: self.skill_url,
            description: self.description,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn search_normalizes_skillsmp_rows() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/api/v1/skills/search?q=pdf&limit=5&sortBy=stars")
            .with_status(200)
            .with_body(
                r#"{"success":true,"data":{"skills":[{"id":"a-pdf-skill-md","name":"pdf","author":"TuYv","description":"PDF stuff","githubUrl":"https://github.com/TuYv/ccpm/tree/main/x","skillUrl":"https://skillsmp.com/skills/a","stars":7}]}}"#,
            )
            .create_async()
            .await;
        let c = SkillsmpClient::with_base(server.url(), None);
        let out = c.search("pdf", 5).await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "pdf");
        assert_eq!(out[0].source, "TuYv");
        assert_eq!(out[0].installs, 7);
        assert_eq!(out[0].install_url, "https://github.com/TuYv/ccpm/tree/main/x");
        assert_eq!(out[0].source_type, "github");
        assert_eq!(out[0].description, "PDF stuff");
        assert_eq!(out[0].slug, "a-pdf-skill-md");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn rejected_key_falls_back_to_anonymous() {
        let mut server = mockito::Server::new_async().await;
        // First request WITH the bad key → 401 (skillsmp rejects it).
        let m401 = server
            .mock("GET", "/api/v1/skills/search?q=pdf&limit=5&sortBy=stars")
            .match_header("authorization", "Bearer sk_bad")
            .with_status(401)
            .with_body(r#"{"success":false,"error":{"code":"INVALID_API_KEY"}}"#)
            .create_async()
            .await;
        // Retry WITHOUT auth (anonymous) → 200.
        let m200 = server
            .mock("GET", "/api/v1/skills/search?q=pdf&limit=5&sortBy=stars")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_status(200)
            .with_body(
                r#"{"data":{"skills":[{"id":"x-skill-md","name":"pdf","githubUrl":"https://github.com/o/r/tree/main/p"}]}}"#,
            )
            .create_async()
            .await;
        let c = SkillsmpClient::with_base(server.url(), Some("sk_bad".into()));
        let out = c.search("pdf", 5).await.unwrap();
        assert_eq!(out.len(), 1, "anonymous retry returns results despite the bad key");
        assert_eq!(out[0].name, "pdf");
        m401.assert_async().await;
        m200.assert_async().await;
    }

    #[tokio::test]
    async fn search_anonymous_sends_no_auth_header() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/api/v1/skills/search?q=x&limit=1&sortBy=stars")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_status(200)
            .with_body(r#"{"data":{"skills":[]}}"#)
            .create_async()
            .await;
        let c = SkillsmpClient::with_base(server.url(), None);
        let out = c.search("x", 1).await.unwrap();
        assert!(out.is_empty());
        m.assert_async().await;
    }
}
