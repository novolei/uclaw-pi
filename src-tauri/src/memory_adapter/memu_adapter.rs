// SPDX-License-Identifier: MIT
//! `MemUAdapter` — wraps `crate::memu::client::MemUClient` behind the
//! `MemoryAdapter` trait. **Recall-focused:** memU is a semantic/episodic store
//! (no stable `(namespace, key)` addressing), so `recall`/`store`/`list` are
//! real while `get`/`delete`/`clear_namespace`/`namespace_summaries` are
//! minimal-with-warn (memU cannot key-address). See
//! `docs/superpowers/specs/2026-06-01-memu-adapter-design.md`.
//!
//! Point `MemoryRecallConfig.prompt_recall_backend = "memu"` to use this as the
//! agent prompt's semantic-recall source (via the piece-2 supplement).
//!
//! Testability: `MemUClient` wraps a live Python subprocess, so the
//! bridge-calling methods (`recall`/`store`/`list`) are integration-only; the
//! pure mapping functions below carry the unit tests.

use std::sync::Arc;

use async_trait::async_trait;

use super::traits::MemoryAdapter;
use super::types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};
use crate::memory::EnrichedMemoryItem;
use crate::memu::client::MemUClient;

const ADAPTER_NAME: &str = "memu";

/// Map the trait's category onto a memU `memory_type`.
fn category_to_memory_type(cat: &MemoryCategory) -> String {
    match cat {
        MemoryCategory::Core => "knowledge".to_string(),
        MemoryCategory::Conversation => "event".to_string(),
        MemoryCategory::Daily => "event".to_string(),
        MemoryCategory::Custom(s) => s.clone(),
    }
}

/// Reverse of [`category_to_memory_type`] for hydration. (`Daily` is lossy — it
/// shares `"event"` with `Conversation` and hydrates back as `Conversation`.)
fn memory_type_to_category(mt: &str) -> MemoryCategory {
    match mt {
        "knowledge" | "profile" => MemoryCategory::Core,
        "event" => MemoryCategory::Conversation,
        other => MemoryCategory::Custom(other.to_string()),
    }
}

/// memU scopes by `{"user_id": ...}`; map the namespace onto it (global = None).
fn user_scope(ns: Option<&str>) -> Option<serde_json::Value> {
    ns.map(|n| serde_json::json!({ "user_id": n }))
}

/// Map a typed `retrieve_with_context` item to the trait's entry.
fn enriched_to_entry(item: EnrichedMemoryItem, ns: Option<&str>, sid: Option<&str>) -> MemoryEntry {
    let id = item
        .metadata
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    MemoryEntry {
        id,
        key: String::new(),
        content: item.content,
        namespace: ns.map(|s| s.to_string()),
        category: memory_type_to_category(&item.memory_type),
        timestamp: item.created_at.unwrap_or_default(),
        session_id: sid.map(|s| s.to_string()),
        score: Some(item.relevance_score),
    }
}

/// Defensively map an untyped `list_items` JSON row to an entry. Missing fields
/// fall back to sensible defaults — never panics.
fn value_to_entry(v: &serde_json::Value, ns: Option<&str>, sid: Option<&str>) -> MemoryEntry {
    let get_str = |k: &str| v.get(k).and_then(|x| x.as_str()).map(|s| s.to_string());
    let content = get_str("content")
        .or_else(|| get_str("memory_content"))
        .unwrap_or_default();
    let id = get_str("id").unwrap_or_default();
    let memory_type = get_str("memory_type").unwrap_or_default();
    let timestamp = get_str("created_at").unwrap_or_default();
    MemoryEntry {
        id,
        key: String::new(),
        content,
        namespace: ns.map(|s| s.to_string()),
        category: memory_type_to_category(&memory_type),
        timestamp,
        session_id: sid.map(|s| s.to_string()),
        score: None,
    }
}

/// Wraps `MemUClient` and exposes it through the `MemoryAdapter` trait.
pub struct MemUAdapter {
    client: Arc<MemUClient>,
}

impl MemUAdapter {
    pub fn new(client: Arc<MemUClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl MemoryAdapter for MemUAdapter {
    fn name(&self) -> &str {
        ADAPTER_NAME
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        opts: RecallOpts<'_>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        // `include_categories=false` skips the LLM enrichment pass → fast path.
        let items = self
            .client
            .retrieve_with_context(query, None, limit, false)
            .await
            .map_err(|e| anyhow::anyhow!("memu recall: {}", e))?;
        Ok(items
            .into_iter()
            .map(|it| enriched_to_entry(it, opts.namespace, opts.session_id))
            .collect())
    }

    async fn store(
        &self,
        namespace: &str,
        _key: &str,
        content: &str,
        category: MemoryCategory,
        _session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        // memU has no key concept; `key` is dropped. The item is created
        // directly (bypassing the LLM memorize pipeline).
        let memory_type = category_to_memory_type(&category);
        self.client
            .create_item(
                &memory_type,
                content,
                vec![category.to_string()],
                user_scope(Some(namespace)),
            )
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("memu store: {}", e))
    }

    async fn list(
        &self,
        namespace: Option<&str>,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let memory_type = category.map(category_to_memory_type);
        let res = self
            .client
            .list_items(
                None,
                memory_type.as_deref(),
                Some(50),
                Some(0),
                user_scope(namespace),
            )
            .await
            .map_err(|e| anyhow::anyhow!("memu list: {}", e))?;
        Ok(res
            .items
            .iter()
            .map(|v| value_to_entry(v, namespace, session_id))
            .collect())
    }

    async fn get(&self, _namespace: &str, _key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        tracing::debug!("memu: get-by-key unsupported (semantic store, no stable key)");
        Ok(None)
    }

    async fn delete(&self, _namespace: &str, _key: &str) -> anyhow::Result<bool> {
        tracing::debug!("memu: delete-by-key unsupported (memU deletes by item id)");
        Ok(false)
    }

    async fn clear_namespace(&self, _namespace: &str) -> anyhow::Result<u64> {
        tracing::debug!("memu: clear_namespace unsupported (no bulk delete by scope)");
        Ok(0)
    }

    async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>> {
        // memU categories are not namespaces; nothing meaningful to report.
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_round_trips() {
        assert_eq!(
            memory_type_to_category(&category_to_memory_type(&MemoryCategory::Core)),
            MemoryCategory::Core
        );
        assert_eq!(
            memory_type_to_category(&category_to_memory_type(&MemoryCategory::Conversation)),
            MemoryCategory::Conversation
        );
        let c = MemoryCategory::Custom("skill".to_string());
        assert_eq!(memory_type_to_category(&category_to_memory_type(&c)), c);
        // Daily is lossy — it shares "event" with Conversation (documented).
        assert_eq!(category_to_memory_type(&MemoryCategory::Daily), "event");
    }

    #[test]
    fn enriched_maps_fields() {
        let item = EnrichedMemoryItem {
            content: "the launch plan".to_string(),
            memory_type: "knowledge".to_string(),
            relevance_score: 0.73,
            categories: vec![],
            metadata: serde_json::json!({ "id": "mem_1" }),
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
        };
        let e = enriched_to_entry(item, Some("ns1"), Some("sess1"));
        assert_eq!(e.id, "mem_1");
        assert_eq!(e.content, "the launch plan");
        assert_eq!(e.category, MemoryCategory::Core);
        assert_eq!(e.score, Some(0.73));
        assert_eq!(e.timestamp, "2026-01-01T00:00:00Z");
        assert_eq!(e.namespace.as_deref(), Some("ns1"));
        assert_eq!(e.session_id.as_deref(), Some("sess1"));
        assert!(e.key.is_empty());
    }

    #[test]
    fn enriched_missing_id_is_empty() {
        let item = EnrichedMemoryItem {
            content: "x".to_string(),
            memory_type: "event".to_string(),
            relevance_score: 0.1,
            categories: vec![],
            metadata: serde_json::json!({}),
            created_at: None,
        };
        let e = enriched_to_entry(item, None, None);
        assert!(e.id.is_empty());
        assert_eq!(e.category, MemoryCategory::Conversation);
        assert_eq!(e.timestamp, "");
    }

    #[test]
    fn value_to_entry_defensive() {
        let v = serde_json::json!({ "content": "c", "id": "x", "memory_type": "event" });
        let e = value_to_entry(&v, Some("ns"), None);
        assert_eq!(e.content, "c");
        assert_eq!(e.id, "x");
        assert_eq!(e.category, MemoryCategory::Conversation);
        assert_eq!(e.namespace.as_deref(), Some("ns"));
        // Empty object → no panic, empty content.
        let empty = value_to_entry(&serde_json::json!({}), None, None);
        assert_eq!(empty.content, "");
        assert!(empty.id.is_empty());
    }
}
