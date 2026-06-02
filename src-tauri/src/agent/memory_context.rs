//! `load_context` — the single seam that assembles the agent prompt's memory
//! block. Consolidates the previously-duplicated assembly in `tauri_commands`
//! (the main send path + the background recall task). Behavior-preserving:
//! same three sources (graph recall + session KV + browser-task memory), same
//! `agent:memory-recall` event, same `record_used_skills` side-effect.
//!
//! The inputs are deliberately narrow (`&MemoryRecallEngine` + `&MemoryStore` +
//! a pre-computed `browser_ctx`) so the function serves both the `AppState`
//! main path and the background `tokio::spawn` task that has no `AppState`.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::memory::MemoryStore;
use crate::memory_adapter::MemoryAdapter;
use crate::memory_graph::recall::MemoryRecallEngine;

/// Optional adapter-recall supplement (off when `MemoryContextInputs.adapter_recall`
/// is `None`). AppState-free — holds the adapters map directly so the background
/// path can use it too. Routes via `memory_adapter::router::route_recall_in`.
pub struct AdapterRecall<'a> {
    pub adapters: &'a HashMap<String, Arc<dyn MemoryAdapter>>,
    pub default_backend: &'a str,
    pub backend: &'a str,
    pub limit: usize,
}

/// Narrow inputs both call sites can supply. The caller pre-builds
/// `recall_engine` (the build deps differ per site) and `browser_ctx` (the two
/// sites use different browser-memory fns).
pub struct MemoryContextInputs<'a> {
    pub recall_engine: &'a MemoryRecallEngine,
    pub memory_store: &'a MemoryStore,
    pub space_id: &'a str,
    /// Used for BOTH the `session:<id>` namespace and the event `conversationId`.
    pub conversation_id: &'a str,
    pub query: &'a str,
    /// Pre-computed per site.
    pub browser_ctx: Option<String>,
    /// When `Some`, append a `<recalled_memories>` block from this adapter
    /// backend (off when `None` → unchanged behavior).
    pub adapter_recall: Option<AdapterRecall<'a>>,
}

/// Result of [`load_context`]. The caller emits `recall_event` with its own
/// `app_handle` and routes `context` to `set_memory_context` (or the cache).
pub struct LoadedMemoryContext {
    pub context: Option<String>,
    pub recall_event: Option<Value>,
}

/// Concatenate the three optional blocks in the canonical order
/// (graph → session → browser). Returns `None` when nothing is present.
fn compose_memory_context(
    graph_block: Option<String>,
    session_block: Option<&str>,
    browser_block: Option<&str>,
    adapter_block: Option<&str>,
) -> Option<String> {
    let mut out = graph_block.unwrap_or_default();
    if let Some(s) = session_block {
        out.push_str(s);
    }
    if let Some(b) = browser_block {
        out.push_str(b);
    }
    if let Some(a) = adapter_block {
        out.push_str(a);
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Typed pi prompt-context. Dimensions in priority order (facts highest,
/// gbrain lowest). `compose` truncates each dimension to its per-dim cap, then
/// concatenates by priority under `total_budget` chars (lower-priority dropped
/// when the budget would overflow). Phase 3 adds `reflections` / `user_model`.
#[derive(Default)]
pub struct PiPromptContext {
    pub facts: Option<String>,
    pub genes: Option<String>,
    /// Phase 3 — recent distilled reflections (mem.md Reflection Agent). Ranked
    /// above raw `recall`: a reflection is a high-value, already-distilled
    /// insight, so it earns budget before verbose recall snippets.
    pub reflections: Option<String>,
    pub recall: Option<String>,
    pub gbrain: Option<String>,
}

impl PiPromptContext {
    pub fn compose(self, total_budget: usize) -> Option<String> {
        const CAP_FACTS: usize = 1_500;
        const CAP_GENES: usize = 2_500;
        const CAP_REFLECTIONS: usize = 2_000;
        const CAP_RECALL: usize = 8_000;
        const CAP_GBRAIN: usize = 2_000;
        let dims = [
            (self.facts, CAP_FACTS),
            (self.genes, CAP_GENES),
            (self.reflections, CAP_REFLECTIONS),
            (self.recall, CAP_RECALL),
            (self.gbrain, CAP_GBRAIN),
        ];
        let mut kept: Vec<String> = Vec::new();
        let mut used = 0usize;
        for (block, cap) in dims {
            let Some(mut b) = block else { continue };
            if b.trim().is_empty() { continue; }
            if b.len() > cap {
                let mut n = cap;
                while n > 0 && !b.is_char_boundary(n) { n -= 1; }
                b.truncate(n);
            }
            if used + b.len() > total_budget { break; }
            used += b.len();
            kept.push(b);
        }
        if kept.is_empty() { None } else { Some(kept.join("\n\n")) }
    }
}

/// Recall from the configured adapter backend and format a bounded
/// `<recalled_memories>` block. Best-effort: any router/adapter error logs a
/// warning and yields `None` (never fails the turn).
async fn recall_adapter_block(ar: &AdapterRecall<'_>, query: &str) -> Option<String> {
    let opts = crate::memory_adapter::RecallOptsIpc::default();
    let hits = match crate::memory_adapter::route_recall_in(
        ar.adapters,
        ar.default_backend,
        Some(ar.backend),
        "global",
        query,
        ar.limit,
        &opts,
    )
    .await
    {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(backend = %ar.backend, error = %e, "adapter recall supplement failed; skipping");
            return None;
        }
    };
    if hits.is_empty() {
        return None;
    }
    // Observability: the supplement is otherwise silent on success, so this is
    // how you confirm prompt_recall_backend is actually injecting into the prompt.
    tracing::info!(
        backend = %ar.backend,
        hits = hits.len(),
        "adapter recall supplement added to prompt"
    );
    let mut block = String::from("<recalled_memories>\n");
    for h in &hits {
        let snippet: String = h.content.chars().take(200).collect();
        block.push_str(&format!("- [{}] {}: {}\n", h.category, h.key, snippet));
    }
    block.push_str("</recalled_memories>\n");
    Some(block)
}

/// Assemble the prompt memory block from graph recall + session KV + the
/// caller-supplied browser context. On recall-plan failure, logs a warning and
/// returns empty (matches the prior "proceed without memory context" path).
pub async fn load_context(inputs: MemoryContextInputs<'_>) -> LoadedMemoryContext {
    let plan = match inputs
        .recall_engine
        .build_recall_plan(inputs.space_id, inputs.query, false)
        .await
    {
        Ok(plan) => plan,
        Err(e) => {
            tracing::warn!(error = %e, "Memory recall failed, proceeding without memory context");
            return LoadedMemoryContext { context: None, recall_event: None };
        }
    };

    let total = plan.boot.len()
        + plan.triggered.len()
        + plan.relevant.len()
        + plan.expanded.len()
        + plan.recent.len();

    // Session-scoped memory (LIKE match) — independent of the graph total, so a
    // session memory still injects even when graph recall is empty.
    let session_block: Option<String> = {
        let session_ns = format!("session:{}", inputs.conversation_id);
        let session_memories = inputs.memory_store.search(inputs.query, Some(&session_ns), 5);
        if session_memories.is_empty() {
            None
        } else {
            let mut ctx = String::from("<session_memories>\n");
            for m in &session_memories {
                ctx.push_str(&format!("- [{}] {}\n", m.kind, m.value));
            }
            ctx.push_str("</session_memories>\n");
            tracing::info!(
                session_memories = session_memories.len(),
                "Session-scoped memories injected"
            );
            Some(ctx)
        }
    };

    // Optional adapter-recall supplement (off unless inputs.adapter_recall set).
    let adapter_block: Option<String> = match &inputs.adapter_recall {
        Some(ar) => recall_adapter_block(ar, inputs.query).await,
        None => None,
    };

    if total > 0 {
        let budget = inputs.recall_engine.config().token_budget;
        let graph_block = MemoryRecallEngine::format_recall_for_prompt(&plan, budget);
        let context = compose_memory_context(
            Some(graph_block),
            session_block.as_deref(),
            inputs.browser_ctx.as_deref(),
            adapter_block.as_deref(),
        );

        let skills_count = plan
            .boot
            .iter()
            .chain(plan.triggered.iter())
            .chain(plan.relevant.iter())
            .chain(plan.expanded.iter())
            .filter(|c| c.kind == crate::memory_graph::models::MemoryNodeKind::Procedure)
            .count();
        let items: Vec<Value> = plan
            .boot
            .iter()
            .chain(plan.triggered.iter())
            .chain(plan.relevant.iter())
            .chain(plan.expanded.iter())
            .take(20)
            .map(|c| {
                serde_json::json!({
                    "nodeId": c.node_id,
                    "title": c.title,
                    "kind": c.kind,
                    "source": c.source,
                })
            })
            .collect();
        let recall_event = Some(serde_json::json!({
            "totalCandidates": total,
            "skillsCount": skills_count,
            "bootCount": plan.boot.len(),
            "triggeredCount": plan.triggered.len(),
            "relevantCount": plan.relevant.len(),
            "expandedCount": plan.expanded.len(),
            "recentCount": plan.recent.len(),
            "items": items,
            "conversationId": inputs.conversation_id,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));

        // Bump usage_count on every learned skill we emitted (soft ranking
        // signal, best-effort).
        inputs.recall_engine.record_used_skills(&plan);
        if context.is_some() {
            tracing::info!(total_candidates = total, "Memory recall injected into system prompt");
        }
        LoadedMemoryContext { context, recall_event }
    } else {
        // No graph recall — still inject session + browser + adapter aux memory.
        let context = compose_memory_context(
            None,
            session_block.as_deref(),
            inputs.browser_ctx.as_deref(),
            adapter_block.as_deref(),
        );
        LoadedMemoryContext { context, recall_event: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_orders_graph_session_browser() {
        let out =
            compose_memory_context(Some("<g>\n".into()), Some("<s>\n"), Some("<b>\n"), None).unwrap();
        assert_eq!(out, "<g>\n<s>\n<b>\n");
    }

    #[test]
    fn compose_all_none_is_none() {
        assert!(compose_memory_context(None, None, None, None).is_none());
    }

    #[test]
    fn compose_session_only_when_no_graph() {
        let out = compose_memory_context(None, Some("<s>\n"), None, None).unwrap();
        assert_eq!(out, "<s>\n");
    }

    #[test]
    fn compose_browser_only() {
        let out = compose_memory_context(None, None, Some("<b>\n"), None).unwrap();
        assert_eq!(out, "<b>\n");
    }

    #[test]
    fn compose_empty_graph_string_with_no_aux_is_none() {
        // A graph_block of "" (defensive) + no aux → None, not Some("").
        assert!(compose_memory_context(Some(String::new()), None, None, None).is_none());
    }

    #[test]
    fn compose_appends_adapter_block_last() {
        let out = compose_memory_context(
            Some("<g>\n".into()),
            Some("<s>\n"),
            Some("<b>\n"),
            Some("<a>\n"),
        )
        .unwrap();
        assert_eq!(out, "<g>\n<s>\n<b>\n<a>\n");
    }

    #[tokio::test]
    async fn recall_adapter_block_formats_hits() {
        use crate::memory::MemoryStore;
        use crate::memory_adapter::{LegacyKvAdapter, MemoryCategory};
        use rusqlite::Connection;
        use std::sync::Mutex;

        let conn = Connection::open_in_memory().unwrap();
        let store = MemoryStore::new(Arc::new(Mutex::new(conn)));
        store.ensure_table();
        let kv: Arc<dyn MemoryAdapter> = Arc::new(LegacyKvAdapter::new(Arc::new(store)));
        kv.store("global", "k1", "needle content here", MemoryCategory::Core, None)
            .await
            .unwrap();
        let mut adapters: HashMap<String, Arc<dyn MemoryAdapter>> = HashMap::new();
        adapters.insert(kv.name().to_string(), kv);

        let ar = AdapterRecall {
            adapters: &adapters,
            default_backend: "legacy_kv",
            backend: "legacy_kv",
            limit: 5,
        };
        let block = recall_adapter_block(&ar, "needle")
            .await
            .expect("should find the stored entry");
        assert!(block.contains("<recalled_memories>"));
        assert!(block.contains("needle content"));
    }

    #[tokio::test]
    async fn recall_adapter_block_unknown_backend_is_none() {
        let adapters: HashMap<String, Arc<dyn MemoryAdapter>> = HashMap::new();
        let ar = AdapterRecall {
            adapters: &adapters,
            default_backend: "legacy_kv",
            backend: "ghost",
            limit: 5,
        };
        assert!(recall_adapter_block(&ar, "q").await.is_none());
    }

    #[test]
    fn pi_context_orders_by_priority_and_skips_empty() {
        // Priority order: facts → genes → reflections → recall → gbrain.
        let out = PiPromptContext {
            facts: Some("F".into()), genes: Some("G".into()),
            reflections: Some("X".into()),
            recall: Some("R".into()), gbrain: Some("B".into()),
        }.compose(10_000);
        assert_eq!(out.as_deref(), Some("F\n\nG\n\nX\n\nR\n\nB"));
        assert!(PiPromptContext::default().compose(10_000).is_none());
        assert!(PiPromptContext { gbrain: Some("   ".into()), ..Default::default() }
            .compose(10_000).is_none());
    }
    #[test]
    fn pi_context_total_budget_drops_low_priority() {
        let out = PiPromptContext {
            facts: Some("AAAA".into()), genes: Some("BBBB".into()),
            recall: Some("CCCC".into()), ..Default::default()
        }.compose(8);
        let s = out.unwrap();
        assert!(s.contains("AAAA") && s.contains("BBBB") && !s.contains("CCCC"));
    }
    #[test]
    fn pi_context_per_dim_cap_truncates_recall_on_char_boundary() {
        // recall block far over CAP_RECALL must be truncated to <= cap, never
        // splitting a UTF-8 codepoint.
        let huge = "电".repeat(5000); // 3 bytes each = 15000 bytes > CAP_RECALL
        let out = PiPromptContext { recall: Some(huge), ..Default::default() }
            .compose(1_000_000);
        let s = out.unwrap();
        assert!(s.len() <= 8_000, "recall capped to CAP_RECALL=8000, got {}", s.len());
        assert!(std::str::from_utf8(s.as_bytes()).is_ok(), "no broken codepoint");
    }
}
