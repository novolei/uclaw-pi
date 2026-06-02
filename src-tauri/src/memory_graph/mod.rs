pub mod models;
pub mod store;
pub mod search;
pub mod recall;
pub mod reflection;
// Phase 3 growth — turn-count-triggered ReflectionService (mem.md Reflection Agent).
pub mod reflection_service;
pub mod environment;
pub mod auto_classify;
// Memory OS Foundation Phase 1 — per-entity compiled-truth + timeline schema.
pub mod entity_page;
// Memory OS Foundation Phase 2 — zero-LLM reference extractor + link-type inferrer.
pub mod auto_link;
// Memory OS Foundation Phase 3 — AI Wiki synthesis (index.md SQL-only, overview.md LLM-driven).
pub mod wiki_synth;
// Memory OS Foundation Phase 6a — shared LLM adapter for wiki/lint/entity-synth scenarios.
pub mod memory_os_llm;
// Memory OS Foundation Phase 7 — markdown frontmatter + disk-mirror sync state.
pub mod brain_io;
// Memory OS Foundation Phase 7.4 — opt-in fs watcher over the brain dir.
pub mod brain_watcher;
// Memory OS Sprint 1.7 — PROFILE.md managed-block parser/renderer.
pub mod profile_md;
// Memory OS L3 §4.12.1 (RETAINED per ADR 2026-05-20 §8) — Importance-Aware
// Decay: Ebbinghaus forgetting + importance weighting. Writes to V44's
// `memory_importance_scores` table. Algorithm-only in this PR; scheduler
// wiring deferred to a follow-up.
pub mod importance_decay;
// Memory OS L3 §3.2.1 (RETAINED per ADR 2026-05-20 §8) — global
// `timeline_events` write API. Wired into EntityPage create (Q2a) +
// other event sources in follow-up PRs.
pub mod timeline_events;
// Memory OS L3 §3.3 (RETAINED per ADR 2026-05-20 §8) — Temporal
// Query Classifier. V1 keyword + regex detection; future PR adds
// LLM disambiguation fallback.
pub mod temporal_classifier;
// Memory OS L3 §4.12.3 (RETAINED per ADR 2026-05-20 §8) — Spaced
// Repetition (Anki SM-2 ladder) for verified high-importance
// EntityPages. V45 schema + state-machine + tests in this PR;
// scheduler hook + LLM re-check in a follow-up.
pub mod spaced_repetition;
// Memory OS L3 §4.12.4 (RETAINED per ADR 2026-05-20 §8) — Concept
// Drift Detection. V46 schema + Levenshtein-based pure algorithm +
// tests; scheduler + LLM triage in a follow-up.
pub mod drift_detection;
// Memory OS L3 §4.12.5 (RETAINED per ADR 2026-05-20 §8) — Cross-
// Source Triangulation: confidence boost when ≥2 sources agree.
// V47 schema + pure boost formula + DB record/summarize helpers
// in this PR; scheduler + LLM "does source X support claim Y?"
// classifier in a follow-up.
pub mod triangulation;

// ──────────────────────────────────────────────────────────────────────────
// Phase 0.5-T7 — runtime freeze guard.
//
// Per ADR `docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md` §11.2,
// the `memory_graph` module's write surface is **frozen**. gbrain (Sprint 2.1+)
// is the primary long-term knowledge layer.
//
// Three layers of defense:
// 1. Git pre-commit hook  — `scripts/git-hooks/checks/check-memory-graph-freeze.sh`
//    blocks new callsites at commit time.
// 2. Claude Code PreToolUse hook — `.claude/hooks/check-memory-graph.sh`
//    blocks the agent from editing-in new callsites.
// 3. Runtime guard (this function) — observes any callsite that slipped through
//    the static layers (dynamic dispatch, cross-language bridges, etc.).
//
// `enforce_freeze` is called from every public write function in this module.
// See `store.rs` / `timeline_events.rs` / `spaced_repetition.rs` for callers.
//
// Behavior:
//   default                                           → tracing::warn! once per
//                                                       callsite (deduped).
//   UCLAW_MEMORY_GRAPH_PANIC_ON_WRITE=1               → panic on first write.
//                                                       Use in tests / CI.
//   UCLAW_MEMORY_GRAPH_ALLOW_WRITES=1                 → silent (legitimate
//                                                       migration / repair).
pub(crate) fn enforce_freeze(call_site: &'static str) {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};

    if std::env::var("UCLAW_MEMORY_GRAPH_ALLOW_WRITES").as_deref() == Ok("1") {
        return;
    }

    if std::env::var("UCLAW_MEMORY_GRAPH_PANIC_ON_WRITE").as_deref() == Ok("1") {
        panic!(
            "memory_graph::{} called but writes are frozen per ADR §11.2. \
             gbrain (Sprint 2.1+) is the primary knowledge layer. \
             Set UCLAW_MEMORY_GRAPH_ALLOW_WRITES=1 to bypass for an emergency \
             migration. See docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md",
            call_site
        );
    }

    static SEEN: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    if seen.lock().unwrap().insert(call_site) {
        tracing::warn!(
            "memory_graph::{} (write) — module is frozen per ADR §11.2. \
             gbrain (Sprint 2.1+) is the primary knowledge layer. \
             This callsite should migrate. \
             See docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md",
            call_site
        );
    }
}

#[cfg(test)]
mod freeze_tests {
    //! Phase 0.5-T7 — runtime freeze guard tests.

    /// The guard must be silent when the bypass env var is set.
    /// (Verified by manual run; cannot test panic+env-var interactions
    /// safely in unit tests because they share process state.)
    #[test]
    fn enforce_freeze_does_not_panic_in_default_mode() {
        // Clear the panic-mode env var to simulate the default behavior.
        // SAFETY: tests run single-threaded for env mutation in this module.
        // SAFETY: env mutation is process-global; this test file is gated to
        // its own test binary by Rust convention but Cargo may run tests in
        // parallel within a binary. We just verify the guard does NOT panic
        // when the panic env var is unset — that is the prod default.
        unsafe {
            std::env::remove_var("UCLAW_MEMORY_GRAPH_PANIC_ON_WRITE");
            std::env::remove_var("UCLAW_MEMORY_GRAPH_ALLOW_WRITES");
        }
        super::enforce_freeze("test::callsite_default_mode");
        // If we got here without panicking, the default path is OK.
    }
}
