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
    /// When `Some`, run a hybrid `MemoryCore` search for `query` and put its
    /// `<relevant_memories>` block first in the context.
    pub core: Option<Arc<crate::memory_core::MemoryCore>>,
}

/// Result of [`load_context`]. The caller emits `recall_event` with its own
/// `app_handle` and routes `context` to `set_memory_context` (or the cache).
pub struct LoadedMemoryContext {
    pub context: Option<String>,
    pub recall_event: Option<Value>,
}

/// Concatenate the optional blocks in the canonical order
/// core → adapter → graph → session → browser. Returns `None` when nothing is
/// present.
///
/// Why this order: downstream truncation (`PiPromptContext::compose`'s recall cap)
/// cuts from the tail, so the small, query-matched blocks go first and the long
/// graph block can no longer push them out. The core block (MemoryCore hybrid
/// hits) is the highest-precision source, so it leads; the adapter block
/// (≤ limit × 200 chars, also query-matched) follows; then the graph recall
/// block, and finally the session and browser blocks.
fn compose_memory_context(
    core_block: Option<&str>,
    graph_block: Option<String>,
    session_block: Option<&str>,
    browser_block: Option<&str>,
    adapter_block: Option<&str>,
) -> Option<String> {
    let mut out = core_block.map(str::to_string).unwrap_or_default();
    if let Some(a) = adapter_block {
        out.push_str(a);
    }
    out.push_str(&graph_block.unwrap_or_default());
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

/// Typed pi prompt-context. Dimensions in priority order (facts highest,
/// gbrain lowest). `compose` truncates each dimension to its per-dim cap, then
/// concatenates by priority under `total_budget` chars (lower-priority dropped
/// when the budget would overflow). Phase 3 adds `reflections` / `user_model`.
#[derive(Default)]
pub struct PiPromptContext {
    pub facts: Option<String>,
    /// Phase 3 (P3-②) — the distilled `user_model` (Pattern→Model layer). The
    /// core "who you are" persona/preferences summary: small + high-value, so it
    /// ranks just below `facts` and above everything else.
    pub user_model: Option<String>,
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
        const CAP_USER_MODEL: usize = 1_200;
        const CAP_GENES: usize = 2_500;
        const CAP_REFLECTIONS: usize = 2_000;
        const CAP_RECALL: usize = 8_000;
        const CAP_GBRAIN: usize = 2_000;
        let dims = [
            (self.facts, CAP_FACTS),
            (self.user_model, CAP_USER_MODEL),
            (self.genes, CAP_GENES),
            (self.reflections, CAP_REFLECTIONS),
            (self.recall, CAP_RECALL),
            (self.gbrain, CAP_GBRAIN),
        ];
        // Caps and the total budget count CHARACTERS, not bytes: with byte
        // counting a CJK block (3 bytes/char) got a third of its intended room.
        let mut kept: Vec<String> = Vec::new();
        let mut used = 0usize;
        for (block, cap) in dims {
            let Some(mut b) = block else { continue };
            if b.trim().is_empty() { continue; }
            let mut chars = b.chars().count();
            if chars > cap {
                let cut = b.char_indices().nth(cap).map_or(b.len(), |(i, _)| i);
                b.truncate(cut);
                chars = cap;
            }
            if used + chars > total_budget { break; }
            used += chars;
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

/// Start a `MemoryCore` search on the blocking pool (the search is sync and can
/// take hundreds of ms with models loaded). Resolves to the raw hits (they feed
/// both the prompt block and the recall event); any DB error / join error logs
/// a warning and yields no hits.
fn spawn_core_search(
    core: Option<Arc<crate::memory_core::MemoryCore>>,
    query: &str,
) -> Option<tokio::task::JoinHandle<Vec<crate::memory_core::MemoryHit>>> {
    let core = core?;
    let q = query.to_string();
    Some(tokio::task::spawn_blocking(move || {
        match core.search(&q, core.config().limit) {
            Ok(hits) => hits,
            Err(e) => {
                tracing::warn!(error = %e, "MemoryCore search failed; skipping");
                Vec::new()
            }
        }
    }))
}

/// Upper bound on how long `load_context` waits for the MemoryCore search,
/// measured from when the search was spawned. Kept well under the pi path's
/// `PI_RECALL_DEADLINE_MS` (1500 ms) so a slow search drops only the core block
/// instead of the whole memory context. 1300 ms fits English/skill queries
/// (p50 ~1.1 s on CPU) while leaving ~200 ms headroom for composing the prompt.
const CORE_SEARCH_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1300);

/// Await the core search until `deadline`. On timeout, logs a warning and yields
/// no hits; the blocking task is detached (not aborted) and finishes in the background.
async fn join_core_search(
    h: Option<tokio::task::JoinHandle<Vec<crate::memory_core::MemoryHit>>>,
    deadline: tokio::time::Instant,
) -> Vec<crate::memory_core::MemoryHit> {
    let Some(h) = h else { return Vec::new() };
    match tokio::time::timeout_at(deadline, h).await {
        Ok(Ok(b)) => b,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "MemoryCore search task failed; skipping");
            Vec::new()
        }
        Err(_) => {
            tracing::warn!(
                timeout_ms = CORE_SEARCH_TIMEOUT.as_millis() as u64,
                "MemoryCore search timed out; dropping core block"
            );
            Vec::new()
        }
    }
}

/// Collapse whitespace and cut to `max` chars (char-safe), appending "…" when cut.
fn snippet_of(content: &str, max: usize) -> String {
    let flat = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let mut t: String = flat.chars().take(max).collect();
        t.push('\u{2026}');
        t
    }
}

/// Build the `agent:memory-recall` payload. `plan` is `None` when graph recall
/// failed or found nothing (zero counts, empty items) so the schema is stable.
fn build_recall_event(
    plan: Option<&crate::memory_graph::recall::MemoryRecallPlan>,
    total: usize,
    core_hits: &[crate::memory_core::MemoryHit],
    core_mode: Value,
    conversation_id: &str,
) -> Value {
    let (skills_count, items, boot, triggered, relevant, expanded, recent) = match plan {
        Some(plan) => {
            let all = || {
                plan.boot
                    .iter()
                    .chain(plan.triggered.iter())
                    .chain(plan.relevant.iter())
                    .chain(plan.expanded.iter())
            };
            let skills = all()
                .filter(|c| c.kind == crate::memory_graph::models::MemoryNodeKind::Procedure)
                .count();
            let items: Vec<Value> = all()
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
            (
                skills,
                items,
                plan.boot.len(),
                plan.triggered.len(),
                plan.relevant.len(),
                plan.expanded.len(),
                plan.recent.len(),
            )
        }
        None => (0, Vec::new(), 0, 0, 0, 0, 0),
    };
    serde_json::json!({
        "totalCandidates": total,
        "skillsCount": skills_count,
        "bootCount": boot,
        "triggeredCount": triggered,
        "relevantCount": relevant,
        "expandedCount": expanded,
        "recentCount": recent,
        "items": items,
        "coreHits": core_hits_json(core_hits),
        "mode": core_mode,
        "conversationId": conversation_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })
}

/// JSON entries for the recall event's `coreHits`. Chunk hits have no title, so
/// fall back to the first ~24 chars of the snippet (never the source ref).
fn core_hits_json(hits: &[crate::memory_core::MemoryHit]) -> Vec<Value> {
    hits.iter()
        .map(|h| {
            let snippet = snippet_of(&h.content, 120);
            let title = if h.title.trim().is_empty() {
                snippet.chars().take(24).collect::<String>()
            } else {
                h.title.clone()
            };
            serde_json::json!({
                "title": title,
                "snippet": snippet,
                "source": h.source,
                "score": h.score,
                "scoreKind": if h.reranked { "rerank" } else { "rrf" },
            })
        })
        .collect()
}

/// Assemble the prompt memory block from graph recall + session KV + the
/// caller-supplied browser context. On recall-plan failure, logs a warning and
/// returns empty (matches the prior "proceed without memory context" path).
pub async fn load_context(inputs: MemoryContextInputs<'_>) -> LoadedMemoryContext {
    // Spawned first so the MemoryCore search runs concurrently with graph recall.
    let core_deadline = tokio::time::Instant::now() + CORE_SEARCH_TIMEOUT;
    let core_handle = spawn_core_search(inputs.core.clone(), inputs.query);
    let core_mode: Value = match inputs.core.as_ref() {
        Some(c) => serde_json::to_value(c.status().mode).unwrap_or_else(|_| Value::from("keyword")),
        None => Value::from("keyword"),
    };
    let plan = match inputs
        .recall_engine
        .build_recall_plan(inputs.space_id, inputs.query, false)
        .await
    {
        Ok(plan) => plan,
        Err(e) => {
            tracing::warn!(error = %e, "Memory recall failed, proceeding without memory context");
            let core_hits = join_core_search(core_handle, core_deadline).await;
            let core_block = crate::memory_core::MemoryCore::render_block(&core_hits);
            let context = compose_memory_context(core_block.as_deref(), None, None, None, None);
            let recall_event = (!core_hits.is_empty()).then(|| {
                build_recall_event(None, 0, &core_hits, core_mode.clone(), inputs.conversation_id)
            });
            return LoadedMemoryContext { context, recall_event };
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

    let core_hits = join_core_search(core_handle, core_deadline).await;
    let core_block: Option<String> = crate::memory_core::MemoryCore::render_block(&core_hits);
    if total > 0 || !core_hits.is_empty() {
        let budget = inputs.recall_engine.config().token_budget;
        let graph_block = (total > 0)
            .then(|| MemoryRecallEngine::format_recall_for_prompt(&plan, budget));
        let context = compose_memory_context(
            core_block.as_deref(),
            graph_block,
            session_block.as_deref(),
            inputs.browser_ctx.as_deref(),
            adapter_block.as_deref(),
        );

        let recall_event = Some(build_recall_event(
            Some(&plan),
            total,
            &core_hits,
            core_mode,
            inputs.conversation_id,
        ));

        if total > 0 {
            inputs.recall_engine.record_used_skills(&plan);
        }
        if context.is_some() {
            tracing::info!(
                total_candidates = total,
                core_hits = core_hits.len(),
                "Memory recall injected into system prompt"
            );
        }
        LoadedMemoryContext { context, recall_event }
    } else {
        // No graph recall — still inject session + browser + adapter aux memory.
        let context = compose_memory_context(
            core_block.as_deref(),
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

    fn mk_hit(title: &str, content: &str) -> crate::memory_core::MemoryHit {
        crate::memory_core::MemoryHit {
            doc_id: "x".into(),
            source: "chunk".into(),
            title: title.into(),
            content: content.into(),
            score: 0.5,
            reranked: false,
        }
    }

    #[test]
    fn core_hits_json_snippet_and_title_fallback() {
        let long = format!("a  b\n{}", "字".repeat(200));
        let v = core_hits_json(&[mk_hit("", &long), mk_hit("T", "x")]);
        let snip = v[0]["snippet"].as_str().unwrap();
        assert_eq!(snip.chars().count(), 121);
        assert!(snip.starts_with("a b 字") && snip.ends_with('\u{2026}'));
        assert_eq!(v[0]["title"].as_str().unwrap().chars().count(), 24);
        assert_eq!(v[1]["title"], "T");
        assert_eq!(v[1]["snippet"], "x");
        assert_eq!(v[0]["scoreKind"], "rrf");
        let mut r = mk_hit("T", "x");
        r.reranked = true;
        assert_eq!(core_hits_json(&[r])[0]["scoreKind"], "rerank");
        // exactly 120 chars: no ellipsis; 121: ellipsis
        assert_eq!(snippet_of(&"a".repeat(120), 120), "a".repeat(120));
        assert_eq!(snippet_of(&"a".repeat(121), 120).chars().count(), 121);
    }

    #[test]
    fn build_recall_event_none_plan_has_stable_schema() {
        let ev = build_recall_event(None, 0, &[mk_hit("T", "x")], Value::from("hybrid"), "c9");
        assert_eq!(ev["mode"], "hybrid");
        assert_eq!(ev["totalCandidates"], 0);
        assert_eq!(ev["items"].as_array().unwrap().len(), 0);
        assert_eq!(ev["coreHits"].as_array().unwrap().len(), 1);
        assert_eq!(ev["conversationId"], "c9");
    }

    #[tokio::test]
    async fn recall_event_emitted_when_graph_plan_fails() {
        use crate::memory::MemoryStore;
        use crate::memory_core::{index, MemoryCore, MemoryCoreConfig};
        use crate::memory_graph::{recall::{MemoryRecallConfig, MemoryRecallEngine}, store::MemoryGraphStore};
        use rusqlite::Connection;
        use std::sync::Mutex;

        // Graph DB with no tables -> build_recall_plan errors.
        let gstore = Arc::new(MemoryGraphStore::new(Arc::new(Mutex::new(Connection::open_in_memory().unwrap()))));
        let engine = MemoryRecallEngine::new(gstore, MemoryRecallConfig::default());
        assert!(engine.build_recall_plan("default", "q", false).await.is_err(), "precondition");
        let kv = MemoryStore::new(Arc::new(Mutex::new(Connection::open_in_memory().unwrap())));
        kv.ensure_table();
        let cconn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations_up_to(&cconn, 61).unwrap();
        index::sync(&cconn, &[index::SourceDoc {
            doc_id: "graph:a".into(), source: "graph", source_id: "a".into(),
            title: "家庭".into(), content: "用户的儿子叫刘书维".into(), updated_at: None,
        }], None).unwrap();
        let core = Arc::new(MemoryCore::new(Arc::new(Mutex::new(cconn)), std::path::PathBuf::from("/nonexistent"), MemoryCoreConfig::default()));
        let loaded = load_context(MemoryContextInputs {
            recall_engine: &engine, memory_store: &kv, space_id: "default", conversation_id: "c1",
            query: "我儿子叫什么名字", browser_ctx: None, adapter_recall: None, core: Some(core),
        }).await;
        let ev = loaded.recall_event.expect("event despite graph failure");
        assert_eq!(ev["coreHits"].as_array().unwrap().len(), 1);
        assert_eq!(ev["totalCandidates"], 0);
        assert!(loaded.context.unwrap().contains("刘书维"));
    }

    #[tokio::test]
    async fn recall_event_carries_core_hits_when_graph_empty() {
        use crate::memory::MemoryStore;
        use crate::memory_core::{index, MemoryCore, MemoryCoreConfig};
        use crate::memory_graph::{recall::{MemoryRecallConfig, MemoryRecallEngine}, store::MemoryGraphStore};
        use rusqlite::Connection;
        use std::sync::Mutex;

        let gconn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations_up_to(&gconn, 61).unwrap();
        let gstore = Arc::new(MemoryGraphStore::new(Arc::new(Mutex::new(gconn))));
        let engine = MemoryRecallEngine::new(gstore, MemoryRecallConfig::default());
        let kv_conn = Connection::open_in_memory().unwrap();
        let kv = MemoryStore::new(Arc::new(Mutex::new(kv_conn)));
        kv.ensure_table();

        let cconn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations_up_to(&cconn, 61).unwrap();
        index::sync(
            &cconn,
            &[index::SourceDoc {
                doc_id: "graph:a".into(),
                source: "graph",
                source_id: "a".into(),
                title: "家庭".into(),
                content: "用户的儿子叫刘书维".into(),
                updated_at: None,
            }],
            None,
        )
        .unwrap();
        let core = Arc::new(MemoryCore::new(
            Arc::new(Mutex::new(cconn)),
            std::path::PathBuf::from("/nonexistent"),
            MemoryCoreConfig::default(),
        ));

        let loaded = load_context(MemoryContextInputs {
            recall_engine: &engine,
            memory_store: &kv,
            space_id: "default",
            conversation_id: "c1",
            query: "我儿子叫什么名字",
            browser_ctx: None,
            adapter_recall: None,
            core: Some(core),
        })
        .await;
        let ev = loaded.recall_event.expect("event emitted for core hits");
        let hits = ev["coreHits"].as_array().unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["title"], "家庭");
        assert_eq!(hits[0]["source"], "graph");
        assert!(hits[0]["snippet"].as_str().unwrap().contains("刘书维"));
        assert_eq!(ev["mode"], "keyword");
        assert_eq!(ev["totalCandidates"], 0);
        assert!(loaded.context.unwrap().contains("刘书维"));
    }

    #[test]
    fn compose_orders_graph_session_browser() {
        let out =
            compose_memory_context(None, Some("<g>\n".into()), Some("<s>\n"), Some("<b>\n"), None).unwrap();
        assert_eq!(out, "<g>\n<s>\n<b>\n");
    }

    #[test]
    fn compose_all_none_is_none() {
        assert!(compose_memory_context(None, None, None, None, None).is_none());
    }

    #[test]
    fn compose_session_only_when_no_graph() {
        let out = compose_memory_context(None, None, Some("<s>\n"), None, None).unwrap();
        assert_eq!(out, "<s>\n");
    }

    #[test]
    fn compose_browser_only() {
        let out = compose_memory_context(None, None, None, Some("<b>\n"), None).unwrap();
        assert_eq!(out, "<b>\n");
    }

    #[test]
    fn compose_empty_graph_string_with_no_aux_is_none() {
        // A graph_block of "" (defensive) + no aux → None, not Some("").
        assert!(compose_memory_context(None, Some(String::new()), None, None, None).is_none());
    }

    #[test]
    fn compose_puts_adapter_block_first() {
        let out = compose_memory_context(
            None,
            Some("<g>\n".into()),
            Some("<s>\n"),
            Some("<b>\n"),
            Some("<a>\n"),
        )
        .unwrap();
        assert_eq!(out, "<a>\n<g>\n<s>\n<b>\n");
    }

    #[test]
    fn compose_puts_core_block_first() {
        let out = compose_memory_context(
            Some("<c>\n"),
            Some("<g>\n".into()),
            Some("<s>\n"),
            Some("<b>\n"),
            Some("<a>\n"),
        )
        .unwrap();
        assert_eq!(out, "<c>\n<a>\n<g>\n<s>\n<b>\n");
    }

    #[tokio::test]
    async fn join_core_search_times_out_without_blocking() {
        let slow = tokio::task::spawn_blocking(|| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            vec![mk_hit("T", "late")]
        });
        let t = std::time::Instant::now();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(50);
        assert!(join_core_search(Some(slow), deadline).await.is_empty());
        assert!(t.elapsed() < std::time::Duration::from_millis(400), "must not wait for the task");

        let fast = tokio::task::spawn_blocking(|| vec![mk_hit("T", "hit")]);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        assert_eq!(join_core_search(Some(fast), deadline).await.len(), 1);
        assert!(join_core_search(None, deadline).await.is_empty());
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
        // Priority order: facts → user_model → genes → reflections → recall → gbrain.
        let out = PiPromptContext {
            facts: Some("F".into()), user_model: Some("U".into()),
            genes: Some("G".into()),
            reflections: Some("X".into()),
            recall: Some("R".into()), gbrain: Some("B".into()),
        }.compose(10_000);
        assert_eq!(out.as_deref(), Some("F\n\nU\n\nG\n\nX\n\nR\n\nB"));
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
        // recall block far over CAP_RECALL must be truncated to <= cap chars,
        // never splitting a UTF-8 codepoint.
        let huge = "电".repeat(20_000);
        let out = PiPromptContext { recall: Some(huge), ..Default::default() }
            .compose(1_000_000);
        let s = out.unwrap();
        assert_eq!(s.chars().count(), 8_000, "recall capped to CAP_RECALL=8000 chars");
        assert!(std::str::from_utf8(s.as_bytes()).is_ok(), "no broken codepoint");
    }

    #[test]
    fn pi_context_caps_count_chars_not_bytes() {
        // 5000 CJK chars = 15000 bytes: under the 8000-char cap, so kept whole.
        // (Byte counting used to cut this to ~2,666 chars.)
        let cjk = "电".repeat(5_000);
        let s = PiPromptContext { recall: Some(cjk), ..Default::default() }
            .compose(12_000)
            .unwrap();
        assert_eq!(s.chars().count(), 5_000);
        // The total budget is in chars too: 1500 + 5000 CJK chars fit in 7000.
        let s = PiPromptContext {
            facts: Some("事".repeat(1_500)),
            recall: Some("电".repeat(5_000)),
            ..Default::default()
        }
        .compose(7_000)
        .unwrap();
        assert!(s.contains('电'), "recall must survive a char-denominated budget");
    }
}
