// SPDX-License-Identifier: MIT
//! `GbrainAdapter` — wraps the typed `crate::gbrain::browse` client (over the
//! `mcp__gbrain__*` MCP tools) behind the `MemoryAdapter` trait. gbrain is the
//! primary durable-knowledge wiki; `recall`/`store`/`list`/`get` are real
//! (gbrain supports get-by-slug), while `delete`/`clear_namespace`/
//! `namespace_summaries` are minimal-with-warn (no delete / bulk-op tool). See
//! `docs/superpowers/specs/2026-06-01-gbrain-adapter-design.md`.
//!
//! Point `MemoryRecallConfig.prompt_recall_backend = "gbrain"` to use this as
//! the agent prompt's durable-knowledge recall source (via the piece-2 supplement).
//!
//! Registration is unconditional: the adapter holds the always-present
//! `mcp_manager` handle and each call checks gbrain availability via `browse::`
//! (returns `GbrainError::NotConnected` when offline), so a gbrain that connects
//! after boot becomes usable without a restart. Testability: the `browse::`
//! calls hit a live MCP server, so the pure mapping fns below carry the tests.

use async_trait::async_trait;

use super::traits::MemoryAdapter;
use super::types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};
use crate::gbrain::browse::{self, PageDetail, PageSummary, SearchHit};
use crate::mcp::SharedMcpManager;

const ADAPTER_NAME: &str = "gbrain";

/// Map `(namespace, key)` to a gbrain kebab-case slug.
fn slugify(ns: &str, key: &str) -> String {
    let raw = format!("{ns}-{key}");
    let mut out = String::with_capacity(raw.len());
    let mut prev_dash = false;
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// `search` hit → entry (the recall mapping).
fn hit_to_entry(h: SearchHit, ns: Option<&str>, sid: Option<&str>) -> MemoryEntry {
    MemoryEntry {
        id: h.slug.clone(),
        key: h.slug,
        content: h.snippet,
        namespace: ns.map(|s| s.to_string()),
        category: MemoryCategory::Core,
        timestamp: String::new(),
        session_id: sid.map(|s| s.to_string()),
        score: Some(h.similarity),
    }
}

/// `list_pages` summary → entry.
fn summary_to_entry(p: PageSummary, ns: Option<&str>, sid: Option<&str>) -> MemoryEntry {
    MemoryEntry {
        id: p.slug.clone(),
        key: p.slug,
        content: p.title,
        namespace: ns.map(|s| s.to_string()),
        category: MemoryCategory::Core,
        timestamp: p.updated_at.unwrap_or_default(),
        session_id: sid.map(|s| s.to_string()),
        score: None,
    }
}

/// `get_page` detail → entry.
fn detail_to_entry(d: PageDetail, ns: Option<&str>, sid: Option<&str>) -> MemoryEntry {
    MemoryEntry {
        id: d.slug.clone(),
        key: d.slug,
        content: d.compiled_truth,
        namespace: ns.map(|s| s.to_string()),
        category: MemoryCategory::Core,
        timestamp: d.updated_at.unwrap_or_default(),
        session_id: sid.map(|s| s.to_string()),
        score: None,
    }
}

/// Wraps the gbrain MCP client through the `MemoryAdapter` trait.
pub struct GbrainAdapter {
    mcp: SharedMcpManager,
}

impl GbrainAdapter {
    pub fn new(mcp: SharedMcpManager) -> Self {
        Self { mcp }
    }
}

#[async_trait]
impl MemoryAdapter for GbrainAdapter {
    fn name(&self) -> &str {
        ADAPTER_NAME
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        opts: RecallOpts<'_>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let hits = browse::search(&self.mcp, query, limit as u32, 0)
            .await
            .map_err(|e| anyhow::anyhow!("gbrain recall: {}", e.to_command_string()))?;
        Ok(hits
            .into_iter()
            .map(|h| hit_to_entry(h, opts.namespace, opts.session_id))
            .collect())
    }

    async fn store(
        &self,
        namespace: &str,
        key: &str,
        content: &str,
        _category: MemoryCategory,
        _session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        browse::put_page(&self.mcp, &slugify(namespace, key), content)
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("gbrain store: {}", e.to_command_string()))
    }

    async fn list(
        &self,
        namespace: Option<&str>,
        _category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let pages = browse::list_pages(&self.mcp, 50, None, None, None, None)
            .await
            .map_err(|e| anyhow::anyhow!("gbrain list: {}", e.to_command_string()))?;
        Ok(pages
            .into_iter()
            .map(|p| summary_to_entry(p, namespace, session_id))
            .collect())
    }

    async fn get(&self, namespace: &str, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        // gbrain's get_page is fuzzy; any error (incl. not-found) → None.
        match browse::get_page(&self.mcp, &slugify(namespace, key)).await {
            Ok(d) => Ok(Some(detail_to_entry(d, Some(namespace), None))),
            Err(e) => {
                tracing::debug!(error = %e.to_command_string(), "gbrain get: not retrievable");
                Ok(None)
            }
        }
    }

    async fn delete(&self, _namespace: &str, _key: &str) -> anyhow::Result<bool> {
        tracing::debug!("gbrain: delete unsupported (no delete tool)");
        Ok(false)
    }

    async fn clear_namespace(&self, _namespace: &str) -> anyhow::Result<u64> {
        tracing::debug!("gbrain: clear_namespace unsupported (no bulk delete)");
        Ok(0)
    }

    async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_kebabs() {
        assert_eq!(slugify("ns x", "Key/1"), "ns-x-key-1");
        assert_eq!(slugify("a", "b"), "a-b");
        // no leading / trailing / doubled dashes
        assert_eq!(slugify(" pad ", "  key  "), "pad-key");
        assert_eq!(slugify("UPPER", "Case"), "upper-case");
    }

    #[test]
    fn hit_maps_fields() {
        let h = SearchHit {
            slug: "openai-gpt5".to_string(),
            title: "OpenAI GPT-5".to_string(),
            snippet: "released in...".to_string(),
            similarity: 0.88,
        };
        let e = hit_to_entry(h, Some("ns1"), Some("s1"));
        assert_eq!(e.id, "openai-gpt5");
        assert_eq!(e.key, "openai-gpt5");
        assert_eq!(e.content, "released in...");
        assert_eq!(e.category, MemoryCategory::Core);
        assert_eq!(e.score, Some(0.88));
        assert_eq!(e.namespace.as_deref(), Some("ns1"));
        assert_eq!(e.session_id.as_deref(), Some("s1"));
        assert_eq!(e.timestamp, "");
    }

    #[test]
    fn summary_and_detail_map() {
        let p = PageSummary {
            slug: "p1".to_string(),
            title: "Page One".to_string(),
            page_type: "entity".to_string(),
            updated_at: Some("2026-01-02T00:00:00Z".to_string()),
        };
        let e = summary_to_entry(p, None, None);
        assert_eq!(e.id, "p1");
        assert_eq!(e.content, "Page One");
        assert_eq!(e.timestamp, "2026-01-02T00:00:00Z");
        assert_eq!(e.score, None);

        let d = PageDetail {
            slug: "p2".to_string(),
            title: "Page Two".to_string(),
            page_type: "entity".to_string(),
            compiled_truth: "the truth".to_string(),
            frontmatter: serde_json::json!({}),
            created_at: None,
            updated_at: None,
            tags: vec![],
            raw_markdown: String::new(),
        };
        let e2 = detail_to_entry(d, Some("ns"), None);
        assert_eq!(e2.content, "the truth");
        assert_eq!(e2.timestamp, "");
        assert_eq!(e2.category, MemoryCategory::Core);
    }
}
