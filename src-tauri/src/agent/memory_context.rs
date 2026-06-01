//! `load_context` — the single seam that assembles the agent prompt's memory
//! block. Consolidates the previously-duplicated assembly in `tauri_commands`
//! (the main send path + the background recall task). Behavior-preserving:
//! same three sources (graph recall + session KV + browser-task memory), same
//! `agent:memory-recall` event, same `record_used_skills` side-effect.
//!
//! The inputs are deliberately narrow (`&MemoryRecallEngine` + `&MemoryStore` +
//! a pre-computed `browser_ctx`) so the function serves both the `AppState`
//! main path and the background `tokio::spawn` task that has no `AppState`.

use serde_json::Value;

use crate::memory::MemoryStore;
use crate::memory_graph::recall::MemoryRecallEngine;

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
) -> Option<String> {
    let mut out = graph_block.unwrap_or_default();
    if let Some(s) = session_block {
        out.push_str(s);
    }
    if let Some(b) = browser_block {
        out.push_str(b);
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
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

    if total > 0 {
        let budget = inputs.recall_engine.config().token_budget;
        let graph_block = MemoryRecallEngine::format_recall_for_prompt(&plan, budget);
        let context = compose_memory_context(
            Some(graph_block),
            session_block.as_deref(),
            inputs.browser_ctx.as_deref(),
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
        // No graph recall — still inject session + browser aux memory if present.
        let context =
            compose_memory_context(None, session_block.as_deref(), inputs.browser_ctx.as_deref());
        LoadedMemoryContext { context, recall_event: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_orders_graph_session_browser() {
        let out = compose_memory_context(Some("<g>\n".into()), Some("<s>\n"), Some("<b>\n")).unwrap();
        assert_eq!(out, "<g>\n<s>\n<b>\n");
    }

    #[test]
    fn compose_all_none_is_none() {
        assert!(compose_memory_context(None, None, None).is_none());
    }

    #[test]
    fn compose_session_only_when_no_graph() {
        let out = compose_memory_context(None, Some("<s>\n"), None).unwrap();
        assert_eq!(out, "<s>\n");
    }

    #[test]
    fn compose_browser_only() {
        let out = compose_memory_context(None, None, Some("<b>\n")).unwrap();
        assert_eq!(out, "<b>\n");
    }

    #[test]
    fn compose_empty_graph_string_with_no_aux_is_none() {
        // A graph_block of "" (defensive) + no aux → None, not Some("").
        assert!(compose_memory_context(Some(String::new()), None, None).is_none());
    }
}
