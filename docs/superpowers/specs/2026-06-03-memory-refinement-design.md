# Memory Refinement Layer (P5) — Design Spec

> **Status:** Approved (design gate passed 2026-06-03). Next: `writing-plans` → two
> bisectable PRs, subagent-driven TDD execution.
> **Branch:** `pi/memory-refinement`.
> **Predecessors:** P1–P4 memory integration (specs `2026-06-02-memory-integration-*`).

## 1. Motivation

P1–P4 built the **generation** half of the Agent-Native memory system on the pi
path: facts (learning facets), reflections (distilled insights), `user_model`
(Pattern→Model persona), and daydreams (divergent free-association). End-to-end
verification (2026-06-03, `uclaw.log.2026-06-03`) confirmed all of it fires on the
pi engine.

That same verification surfaced two **data-proven** defects that only a
**convergent refinement** pass can fix — neither is a plumbing bug, both are the
natural entropy of an append-only memory:

1. **`reflections` accumulates near-duplicates.** The verify run produced multiple
   ~0.95-confidence rows all saying variants of *"the user values being remembered
   as a whole person / repeatedly tests memory consistency"*. Append-only reflection
   has no dedup, so semantically-redundant insights pile up and crowd the prompt
   budget.
2. **`user_model` drifts via extrapolation.** Promotion distilled *"30-year-old
   **product manager**"* — the user is an engineer. `run_promotion` reads a real
   facts/profile digest (`build_profile_digest`) but the LLM extrapolates beyond it.

The [Anthropic "Dreams" framework](https://platform.claude.com/docs/en/managed-agents/dreams)
names exactly this missing capability: **convergent memory curation** — periodically
dedup / reconcile / reorganize an accumulating memory store. uClaw's existing
`daydream` (P4) is the *divergent* opposite (free-association → new hypotheses); the
two are complementary, not substitutes. `mem.md` describes layered memory
(Hot/Warm/Cold/Archive) + importance weighting but **no consolidation pass** — P5
fills that gap and dovetails with P1-①'s `archive_pending` importance work.

## 2. Goals / Non-goals

**Goals**
- A periodic **consolidation pass** that dedups `reflections` and re-grounds
  `user_model`, applied **in-place with an audit trail** (reversible).
- Fix the `user_model` drift at its **source** (tighten the promotion prompt) *and*
  periodically (consolidation re-ground) — defense in depth.
- Two **daydream** refinements: better seeds (ground in reflections + user_model)
  and a **reflow** loop (high-value daydream → low-confidence reflection → enters
  the convergent pipeline).

**Non-goals (this phase)**
- No separate "reviewable store" + review UI (Anthropic-Dreams-faithful variant) —
  rejected for uClaw's lightweight single-user philosophy; in-place + audit is enough.
- No `facts`/`facets` consolidation (facets have their own cache + recall path;
  larger blast radius — deferred).
- No embedding-based clustering for dedup (reflections are few per single user;
  LLM-only one-shot is simpler — embedding prefilter is a documented future scale step).
- No `proactive/` subsystem reflow target (daydream reflows to `reflections` only).

## 3. Design decisions (locked)

| # | Decision | Choice | Rationale |
|---|---|---|---|
| D1 | Consolidation output / apply policy | **In-place + audit trail** (auto-apply, superseded rows retained/reversible) | Local single-user; prompt benefits immediately; lightweight philosophy. Risk bounded by conservative defenses + reversibility. |
| D2 | First-cut scope | **`reflections` + `user_model`** | The two data-proven pain points. Tight, shippable. |
| D3 | Dedup mechanism | **LLM-only, one call** (embedding prefilter deferred) | Single-user reflection counts are small (≤ tens); one LLM call dedups + re-grounds without embedder plumbing. `cosine_similarity` + `MemUEmbedder` already exist for a later scale step. |
| D4 | Trigger cadence | **`run_once` tail, every 100 turns**, gated on **live reflections ≥ 8** | Same turn-count axis as P3/P4. The count pre-gate means first real run ≈ turn 200 (≈1 reflection / 20 turns). |
| D5 | daydream optimizations | **Both** (seed quality + reflow) | User-approved; both edit `run_daydream`, so one PR. |

## 4. Architecture

Reuses the P3/P4 `ReflectionService` skeleton entirely: turn-count trigger in
`run_once`, `learning_llm` presence gate, daily-budget gate
(`today_learning_tokens` vs `learning_llm_daily_token_budget`), borrow-safe
(std `Mutex` dropped before every `.await`), best-effort (every failure logs +
returns, never breaks a live turn). **Zero new infrastructure.**

### PR1 — Consolidation pass (the headline)

**New:** `run_consolidation(state: &AppState)` in `reflection_service.rs`, mounted
in `run_once` immediately after the daydream gate:

```rust
const CONSOLIDATION_EVERY_N_TURNS: u64 = 100;
if should_run_reflection(turn_count, CONSOLIDATION_EVERY_N_TURNS) {
    run_consolidation(&state).await;
}
```

**Flow (`run_consolidation`):**
1. Gates (LLM present, daily budget) — same as `run_promotion`.
2. **Pre-gate:** `SELECT COUNT(*) FROM reflections WHERE archived_at IS NULL` — skip
   if `< MIN_REFLECTIONS_TO_CONSOLIDATE` (8). Nothing to consolidate.
3. Read (borrow-safe block, drop guard before await):
   - All live reflections, newest first, capped 40 → `Vec<(id, insight, confidence)>`.
   - Current `user_model` summary (`get_user_model`).
   - `build_profile_digest` (the grounded facts/profile — same source promotion uses).
4. **One LLM call** (`cost_tag = "memory_consolidation"`), system prompt instructs:
   merge near-duplicate reflections into a deduplicated set (each keeps the *highest*
   confidence of its cluster); re-ground the `user_model` **strictly** in the provided
   facts — do not invent unsupported details. Output JSON:
   ```json
   { "reflections": [ { "insight": "...", "confidence": 0.9 } ],
     "user_model": "..." }
   ```
5. **Parse** via `parse_consolidation_output(s) -> Option<ConsolidationResult>`
   (robust, markdown-fence tolerant; `None` on failure).
6. **Defenses — apply only if ALL hold** (else no-op, log, return; never corrupt
   existing memory):
   - parse succeeded,
   - `result.reflections` non-empty,
   - `result.reflections.len() <= input_live_count` (a dedup shrinks or holds; never grows),
   - `result.user_model` non-empty (when present).
7. **Apply — one transaction** (`conn.transaction()`):
   - `UPDATE reflections SET archived_at = datetime('now') WHERE archived_at IS NULL`
     (soft-delete the entire current live set),
   - `insert_reflection` for each merged reflection (fresh ids),
   - append the prior `user_model` summary to `user_model_history` (audit),
   - `upsert_user_model` with the re-grounded summary.
   - Commit. On any error → rollback (transaction drop), log, return.
8. `tracing::info!(before, after, "consolidation: merged N reflections → M, re-grounded user_model")`.

**Why archive-all-then-reinsert** (vs surgical per-cluster archive): the LLM returns
the *whole* deduped set, not a diff. Archiving the live set and inserting the merged
set is simpler, atomic, and the audit (`archived_at` rows + history) makes it fully
reversible. Reflections are few, so the rewrite is cheap.

**Root-cause fix (same PR):** tighten `PROMOTION_SYSTEM_PROMPT` to forbid
extrapolation beyond the digest ("only state what the facts support; do not infer
occupation/age/identity not present"). This stops drift at the **every-20-turn**
source; consolidation is the periodic deep clean. Defense in depth.

**Required adjacent edit (CLAUDE.md):** `recent_reflections` query gains
`WHERE archived_at IS NULL` — the prompt-injection path
(`PiPromptContext.reflections`) and consolidation's own "read live" must both skip
archived rows. This is the one place a missed edit would silently resurface archived
reflections.

### PR2 — daydream refinement (both edits in `run_daydream`)

**② Seed quality.** Extract a pure, unit-testable helper:
```rust
fn build_daydream_seed(reflections: &[String], user_model: Option<&str>, titles: &[String]) -> String
```
Seed = up to 2 recent reflection insights + the `user_model` summary + up to 3 random
`memory_nodes` titles (keep randomness to preserve divergence). `run_daydream`'s seed
block reads `recent_reflections(2)` + `get_user_model()` + the existing random-titles
query, then calls the helper.

**③ Reflow.** Change the daydream LLM contract to emit
`{ "content": "...", "worth_remembering": true|false }`; parse with
`parse_daydream_output(s) -> (String, bool)` (prose fallback → `worth_remembering=false`).
Always `insert_daydream(content)` + emit `agent:daydream` (unchanged UI behavior).
**Additionally**, when `worth_remembering`, `insert_reflection(content, 0.4, 0)` — a
low-confidence reflection. It then (a) becomes eligible for prompt injection and
(b) gets deduped/merged by PR1's consolidation. This closes the
divergent→convergent loop: daydream → maybe → reflection → consolidation.

> Note: 0.4 < real-reflection confidences (0.8–0.95), marking it speculative. No
> injection confidence-floor is added (YAGNI); if speculative reflections prove noisy
> later, add a floor to the injection query.

## 5. Data model — migration **V59** (additive)

Use **V59** — verified free 2026-06-03 (max landed migration = V58; no open PR; no
plan file claims it). Re-confirm at commit time against the *Active migration
registry* in `@CONTEXT.md` (this session may span the FTS5 `阶段4 PR9` landing, which
would also want a number). Same `const SQL_V59` + split(';')+execute pattern as
V57/V58.

```sql
-- P5: soft-delete marker for consolidated reflections (NULL = live).
ALTER TABLE reflections ADD COLUMN archived_at TEXT;

-- P5: audit trail of superseded user_model summaries (append before each re-ground).
CREATE TABLE IF NOT EXISTS user_model_history (
    id          TEXT PRIMARY KEY,
    summary     TEXT NOT NULL,
    replaced_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

Reversibility: archived reflections remain in `reflections` (just `archived_at IS NOT
NULL`) → un-archivable; every prior `user_model` summary is in `user_model_history` →
restorable. No data is destroyed.

## 6. Testing strategy

Pure functions via TDD (`#[cfg(test)]`, `:memory:` connections, schema applied by
local `apply_*` helpers — same pattern as P3/P4):
- `parse_consolidation_output` — valid JSON, markdown-fenced JSON, prose (→ `None`),
  missing/empty fields, confidence clamp.
- `build_daydream_seed` — mixes all three sources, handles empty reflections / absent
  user_model / empty titles, ordering.
- `parse_daydream_output` — JSON, fenced JSON, prose fallback (`worth_remembering=false`).
- `archived_at` filtering — `apply_reflections_schema` must add the column;
  `recent_reflections` returns only live rows; an archived row is excluded.
- `user_model_history` store CRUD — `apply_user_model_history_schema` + insert + read.
- Consolidation defenses — a pure `consolidation_should_apply(input_len, &result)`
  predicate unit-tested for the grow/empty/parse-fail guards.

LLM orchestration (`run_consolidation`, the `run_daydream` reflow wiring) is
build-green + manual-test only (needs a live provider + `AppState`), exactly as P3/P4.

## 7. Files touched

| File | PR | Change |
|---|---|---|
| `src-tauri/src/db/migrations.rs` | PR1 | V59 (`archived_at` + `user_model_history`) |
| `src-tauri/src/memory_graph/reflection_service.rs` | PR1 | `run_consolidation` + `parse_consolidation_output` + `consolidation_should_apply` + `user_model_history` store helpers + `CONSOLIDATION_SYSTEM_PROMPT` + run_once gate + tighten `PROMOTION_SYSTEM_PROMPT` + `recent_reflections` archived filter + `apply_reflections_schema` adds `archived_at` |
| `src-tauri/src/memory_graph/reflection_service.rs` | PR2 | `build_daydream_seed` + `parse_daydream_output` + `run_daydream` seed/reflow wiring + daydream prompt contract |

No `PiPromptContext` / pi-site changes (consolidation is read-side-transparent — the
injection path just sees fewer, cleaner reflection rows via the `archived_at` filter).

## 8. Risks & mitigations

| Risk | Mitigation |
|---|---|
| LLM over-merges (drops a distinct insight) | Conservative system prompt ("merge only near-identical"); reversible (`archived_at` rows retained); confidence = max of cluster. |
| LLM returns garbage / grows the set | `consolidation_should_apply` guards (parse-ok, non-empty, len ≤ input); no-op on fail. |
| Partial apply corrupts state | Single transaction; rollback on any error. |
| Reflow floods reflections with speculation | `worth_remembering` gate + low 0.4 confidence; consolidation later dedups them. |
| Daily budget exhausted mid-day | Same budget gate as reflection/promotion/daydream; consolidation skips. |
| V59 number collision with open PR | Confirm against *Active migration registry* before claiming. |

## 9. PR plan (bisectable)

- **PR1 `pi/memory-refinement`** — Consolidation pass: V59 migration + store helpers
  (TDD) → `parse_consolidation_output` + `consolidation_should_apply` (TDD) →
  `recent_reflections` archived filter (TDD) → `run_consolidation` + run_once gate +
  promotion-prompt fix (build-green) → commit group.
- **PR2** — daydream refinement: `build_daydream_seed` + `parse_daydream_output`
  (TDD) → `run_daydream` seed/reflow wiring (build-green) → commit.

One branch per plan, one commit per task, `## Commits (bisectable)` table in the PR.
Verification per commit: `cargo build 2>&1 | grep -E "^error"` empty · `cargo test
--lib reflection_service` green · warnings not increased · `Cargo.lock` never staged.

## 10. Future (out of scope)

- Embedding-prefilter dedup (`cosine_similarity` + `MemUEmbedder`) when reflections
  grow large.
- `facts`/`facets` consolidation.
- Proactive reflow target for high-value daydreams.
- Optional separate reviewable store + review UI if multi-user/audit needs grow.
