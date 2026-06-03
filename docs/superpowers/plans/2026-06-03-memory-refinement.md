# Memory Refinement Layer (P5) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (user standing instruction "执行PR的时候使用subagent"). One implementer subagent per task; controller reviews at each task + PR boundary. Steps use checkbox (`- [ ]`).

**Goal:** Add the convergent **memory refinement** layer atop P1–P4's generation: a periodic **consolidation pass** that dedups `reflections` and re-grounds `user_model` in-place with an audit trail, plus two **daydream** refinements (better seeds + a reflow loop).

**Architecture:** Reuse the P3/P4 `ReflectionService` skeleton entirely — turn-count trigger in `run_once`, `learning_llm` presence gate, daily-budget gate, borrow-safe (std `Mutex` dropped before every `.await`), best-effort (every failure logs + returns). Zero new infrastructure. Spec: `docs/superpowers/specs/2026-06-03-memory-refinement-design.md`.

**Tech Stack:** Rust (Tauri v2) · `cargo test --lib reflection_service` · `rusqlite` · branch `pi/memory-refinement` (already created, has the spec commit).

**Verification per commit:** `cd src-tauri && cargo build 2>&1 | grep -E "^error"` (empty) · `cargo test --lib reflection_service 2>&1 | grep "test result"` (all pass) · warnings not increased · `Cargo.lock` NEVER staged · explicit-path `git add` only.

**Impact analysis (CLAUDE.md / GitNexus):** before editing `run_once`, `recent_reflections`, `run_daydream`, `PROMOTION_SYSTEM_PROMPT`, run `gitnexus_impact({target, direction:"upstream"})` and report blast radius. Index was refreshed 2026-06-03.

---

## File Structure

| File | PR | Responsibility |
|---|---|---|
| `src-tauri/src/db/migrations.rs` | PR1 | V59: `reflections.archived_at` + `user_model_history` table |
| `src-tauri/src/memory_graph/reflection_service.rs` | PR1 | `user_model_history` store helpers · `archived_at` filter on `recent_reflections` · `parse_consolidation_output` + `consolidation_should_apply` · `run_consolidation` + run_once gate · tighten `PROMOTION_SYSTEM_PROMPT` |
| `src-tauri/src/memory_graph/reflection_service.rs` | PR2 | `build_daydream_seed` + `parse_daydream_output` + `extract_json_object` reuse · `run_daydream` seed/reflow wiring + prompt contract |

No `PiPromptContext` / pi-site / `engine_sink` changes — consolidation is read-side-transparent (the injection path just sees fewer, cleaner rows via the `archived_at` filter).

---

## PR1 — Consolidation pass

### Task 1: V59 migration + `user_model_history` store (TDD)

**Files:** Modify `src-tauri/src/db/migrations.rs`, `src-tauri/src/memory_graph/reflection_service.rs`

- [ ] **Step 1: Confirm V59 free + registration pattern.** `grep -n "SQL_V58\|V58" src-tauri/src/db/migrations.rs` — read how V58 declares its `const SQL_V58`, how it's added to the migration list/dispatch, and confirm no V59 exists. Re-confirm against the *Active migration registry* (`@CONTEXT.md`) that no open PR (esp. the FTS5 `阶段4 PR9`) has claimed V59; if it has, use the next free integer and update this plan.

- [ ] **Step 2: Add V59 migration** mirroring V58's exact registration (const + list entry). The SQL (additive; split on `;` + execute loop, same as V57/V58):
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

- [ ] **Step 3: Write the failing test** (in `reflection_service.rs` `#[cfg(test)]`):
```rust
    #[test]
    fn user_model_history_inserts_and_reads_recent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        apply_user_model_history_schema(&conn);
        insert_user_model_history(&conn, "h1", "old summary one").unwrap();
        insert_user_model_history(&conn, "h2", "old summary two").unwrap();
        let recent = recent_user_model_history(&conn, 5).unwrap();
        assert_eq!(recent.len(), 2);
        assert!(recent.iter().any(|h| h.summary.contains("old summary two")));
    }
```

- [ ] **Step 4: Run** `cd src-tauri && cargo test --lib user_model_history 2>&1 | tail -8` — expect FAIL (helpers undefined).

- [ ] **Step 5: Implement the store helpers** in `reflection_service.rs` (mirror the `daydreams` store block at lines ~200–239):
```rust
/// One `user_model_history` row, as read back for audit/restore.
#[derive(Debug, Clone)]
pub struct UserModelHistoryRow {
    pub summary: String,
    pub replaced_at: String,
}

/// Apply the V59 `user_model_history` DDL to a bare connection (tests only; the
/// real table is created by the V59 migration).
pub fn apply_user_model_history_schema(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS user_model_history (
            id          TEXT PRIMARY KEY,
            summary     TEXT NOT NULL,
            replaced_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .expect("apply_user_model_history_schema");
}

/// Append one superseded user_model summary before a re-ground overwrites it.
pub fn insert_user_model_history(conn: &Connection, id: &str, summary: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO user_model_history (id, summary) VALUES (?1, ?2)",
        params![id, summary],
    )?;
    Ok(())
}

/// Most-recent superseded summaries, newest first, capped at `limit`.
pub fn recent_user_model_history(
    conn: &Connection,
    limit: usize,
) -> rusqlite::Result<Vec<UserModelHistoryRow>> {
    let mut stmt = conn.prepare(
        "SELECT summary, replaced_at FROM user_model_history
         ORDER BY replaced_at DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit as i64], |r| {
            Ok(UserModelHistoryRow { summary: r.get(0)?, replaced_at: r.get(1)? })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
```

- [ ] **Step 6: Run** `cargo test --lib user_model_history 2>&1 | tail -8` — PASS. Then `cargo build 2>&1 | grep -E "^error"` (empty).

- [ ] **Step 7: Commit**:
```bash
git add src-tauri/src/db/migrations.rs src-tauri/src/memory_graph/reflection_service.rs
git commit -m "feat(db): V59 reflections.archived_at + user_model_history table + store helpers"
```

### Task 2: `reflections.archived_at` soft-delete filter (TDD)

**Files:** Modify `src-tauri/src/memory_graph/reflection_service.rs`

- [ ] **Step 1: Update the test helper** `apply_reflections_schema` (lines ~90–102) so the freshly-created table includes the new column (the real path gets it via the V59 `ALTER`). Add `archived_at TEXT` to the `CREATE TABLE reflections`:
```rust
        "CREATE TABLE IF NOT EXISTS reflections (
            id                 TEXT PRIMARY KEY,
            insight            TEXT NOT NULL,
            confidence         REAL NOT NULL DEFAULT 0.5,
            source_event_count INTEGER NOT NULL DEFAULT 0,
            created_at         TEXT NOT NULL DEFAULT (datetime('now')),
            archived_at        TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_reflections_created ON reflections(created_at DESC);"
```

- [ ] **Step 2: Write the failing test**:
```rust
    #[test]
    fn recent_reflections_excludes_archived() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        apply_reflections_schema(&conn);
        insert_reflection(&conn, "r1", "live insight", 0.9, 10).unwrap();
        insert_reflection(&conn, "r2", "stale insight", 0.8, 10).unwrap();
        conn.execute(
            "UPDATE reflections SET archived_at = datetime('now') WHERE id = 'r2'",
            [],
        )
        .unwrap();
        let recent = recent_reflections(&conn, 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].insight, "live insight");
    }
```

- [ ] **Step 3: Run** `cargo test --lib recent_reflections_excludes_archived 2>&1 | tail -8` — FAIL (returns 2; no filter yet).

- [ ] **Step 4: Add the filter** to `recent_reflections` (lines ~122–139): add `WHERE archived_at IS NULL` to the SELECT:
```rust
        "SELECT insight, confidence, created_at
         FROM reflections
         WHERE archived_at IS NULL
         ORDER BY created_at DESC
         LIMIT ?1",
```

- [ ] **Step 5: Run** `cargo test --lib reflection_service 2>&1 | grep "test result"` — all PASS (new test + existing). `cargo build 2>&1 | grep -E "^error"` (empty).

- [ ] **Step 6: Commit**:
```bash
git add src-tauri/src/memory_graph/reflection_service.rs
git commit -m "feat(consolidation): reflections.archived_at soft-delete — recent_reflections skips archived rows"
```

### Task 3: `parse_consolidation_output` + `consolidation_should_apply` (TDD)

**Files:** Modify `src-tauri/src/memory_graph/reflection_service.rs`

- [ ] **Step 1: Write failing tests**:
```rust
    #[test]
    fn parse_consolidation_output_reads_reflections_and_user_model() {
        let s = r#"```json
        {"reflections":[{"insight":"a","confidence":0.9},{"insight":"b","confidence":0.7}],
         "user_model":"compact model"}
        ```"#;
        let r = parse_consolidation_output(s).expect("should parse");
        assert_eq!(r.reflections.len(), 2);
        assert_eq!(r.reflections[0].0, "a");
        assert!((r.reflections[0].1 - 0.9).abs() < 1e-9);
        assert_eq!(r.user_model.as_deref(), Some("compact model"));
    }

    #[test]
    fn parse_consolidation_output_none_on_prose_or_empty() {
        assert!(parse_consolidation_output("not json at all").is_none());
        assert!(parse_consolidation_output(r#"{"reflections":[]}"#).is_none());
    }

    #[test]
    fn consolidation_should_apply_guards_grow_and_empty() {
        let ok = ConsolidationResult { reflections: vec![("x".into(), 0.5)], user_model: None };
        assert!(consolidation_should_apply(3, &ok)); // 1 <= 3
        assert!(!consolidation_should_apply(0, &ok)); // 1 > 0 → grew
        let empty = ConsolidationResult { reflections: vec![], user_model: None };
        assert!(!consolidation_should_apply(5, &empty));
    }
```

- [ ] **Step 2: Run** `cargo test --lib consolidation 2>&1 | tail -8` — FAIL (undefined).

- [ ] **Step 3: Implement** (place near `parse_reflection_output`, ~line 47):
```rust
/// Parsed consolidation output: the deduplicated reflection set + the re-grounded
/// user_model (absent when the LLM omitted/blanked it).
#[derive(Debug, Clone)]
pub struct ConsolidationResult {
    pub reflections: Vec<(String, f64)>,
    pub user_model: Option<String>,
}

/// Extract the first `{...}` JSON object substring — tolerates ```json fences and
/// preamble/trailing prose. `None` when no balanced-looking braces are present.
fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    (end > start).then(|| &s[start..=end])
}

/// Parse the consolidation LLM output. Returns `None` (→ caller no-ops, never
/// corrupts memory) on any parse failure or an empty reflection set. Confidence is
/// clamped to `[0,1]`; blank insights are dropped; a blank/missing `user_model`
/// becomes `None`.
pub fn parse_consolidation_output(s: &str) -> Option<ConsolidationResult> {
    let obj = extract_json_object(s.trim())?;
    let v: serde_json::Value = serde_json::from_str(obj).ok()?;
    let arr = v.get("reflections")?.as_array()?;
    let mut reflections = Vec::new();
    for item in arr {
        let insight = item.get("insight").and_then(serde_json::Value::as_str).map(str::trim);
        let Some(insight) = insight.filter(|i| !i.is_empty()) else { continue };
        let conf = item
            .get("confidence")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        reflections.push((insight.to_string(), conf));
    }
    if reflections.is_empty() {
        return None;
    }
    let user_model = v
        .get("user_model")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(String::from);
    Some(ConsolidationResult { reflections, user_model })
}

/// Apply-guard: only apply when the result is non-empty AND did not GROW the set
/// (a dedup shrinks or holds). Prevents an LLM hallucination from inflating memory.
pub fn consolidation_should_apply(input_live_count: usize, result: &ConsolidationResult) -> bool {
    !result.reflections.is_empty() && result.reflections.len() <= input_live_count
}
```

- [ ] **Step 4: Run** `cargo test --lib consolidation 2>&1 | tail -8` — PASS. `cargo build 2>&1 | grep -E "^error"` (empty); warnings not increased (note: `extract_json_object` is used here, so no dead-code warning).

- [ ] **Step 5: Commit**:
```bash
git add src-tauri/src/memory_graph/reflection_service.rs
git commit -m "feat(consolidation): parse_consolidation_output + consolidation_should_apply guard (TDD)"
```

### Task 4: `run_consolidation` + run_once gate + promotion-prompt root-cause fix (build-green)

**Files:** Modify `src-tauri/src/memory_graph/reflection_service.rs`

Mirror `run_promotion`'s structure exactly (LLM gate → budget gate → borrow-safe read → LLM call → apply). No new test (needs a live provider + `AppState`); covered by build-green + manual test.

- [ ] **Step 1: Add constants + system prompt** (near the other `*_COST_TAG` consts):
```rust
/// Max tokens for the consolidation completion (a deduped set + a short user_model).
const CONSOLIDATION_MAX_TOKENS: u32 = 1024;
/// Cost tag prefix written into `cost_records.model` for consolidation LLM calls.
const CONSOLIDATION_COST_TAG: &str = "memory_consolidation";
/// Don't consolidate until at least this many live reflections have accumulated.
const MIN_REFLECTIONS_TO_CONSOLIDATE: i64 = 8;
/// Cap how many live reflections feed one consolidation prompt.
const CONSOLIDATION_READ_CAP: usize = 40;

/// System prompt for the convergent consolidation pass — the opposite of daydream.
const CONSOLIDATION_SYSTEM_PROMPT: &str = "\
You are curating an AI agent's long-term memory. You are given (1) a list of \
distilled reflections about one user, (2) the current user_model summary, and (3) \
the grounded facts the user_model must rest on. Do two things. (a) MERGE only \
near-identical or strongly-overlapping reflections into a single deduplicated \
reflection, keeping the highest confidence of each merged group; leave genuinely \
distinct insights untouched and NEVER invent new ones — the output set must be the \
same size or smaller. (b) Rewrite the user_model so every claim is STRICTLY \
supported by the provided facts: remove any occupation, age, identity, or \
preference not present in the facts; do not extrapolate. Respond with ONLY a JSON \
object, no prose, no markdown fences:\n\
{\"reflections\":[{\"insight\":\"<sentence>\",\"confidence\":<float 0.0-1.0>}],\"user_model\":\"<prose>\"}";
```

- [ ] **Step 2: Implement `run_consolidation`** (place after `run_promotion`):
```rust
/// Run one consolidation pass: dedup live reflections + re-ground the user_model in
/// one LLM call, then apply IN-PLACE with an audit trail (archive the superseded
/// reflections, append the prior user_model to `user_model_history`) inside a single
/// transaction. Mirrors [`run_promotion`]'s gates + borrow-safety. Best-effort end
/// to end; every failure path logs and returns. The apply-guards
/// ([`consolidation_should_apply`]) ensure a misbehaving LLM can never grow or blank
/// the memory.
pub async fn run_consolidation(state: &crate::app::AppState) {
    let Some(llm) = state.learning_llm.clone() else {
        tracing::debug!("consolidation: no learning_llm configured; skipping");
        return;
    };
    let daily_budget = {
        let cfg = state.memubot_config.read().await;
        cfg.memory_os.learning_llm_daily_token_budget
    };
    if daily_budget == 0 {
        tracing::debug!("consolidation: learning LLM daily budget is 0; skipping");
        return;
    }
    let spent = crate::cost_store::today_learning_tokens(&state.db);
    if spent >= daily_budget {
        tracing::debug!(spent, daily_budget, "consolidation: daily budget exhausted; skipping");
        return;
    }

    // Read inputs (hold the std Mutex only for the read; drop before await).
    let (live, current_user_model, digest) = {
        let Ok(conn) = state.db.lock() else { return };
        let live_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM reflections WHERE archived_at IS NULL", [], |r| r.get(0))
            .unwrap_or(0);
        if live_count < MIN_REFLECTIONS_TO_CONSOLIDATE {
            tracing::debug!(live_count, "consolidation: too few live reflections; skipping");
            return;
        }
        let live: Vec<(String, String, f64)> = match conn.prepare(
            "SELECT id, insight, confidence FROM reflections
             WHERE archived_at IS NULL ORDER BY created_at DESC LIMIT ?1",
        ) {
            Ok(mut stmt) => match stmt.query_map(params![CONSOLIDATION_READ_CAP as i64], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?))
            }) {
                Ok(rows) => rows.flatten().collect(),
                Err(e) => { tracing::warn!(error = %e, "consolidation: query_map failed; skipping"); return; }
            },
            Err(e) => { tracing::warn!(error = %e, "consolidation: prepare failed; skipping"); return; }
        };
        let current_user_model = get_user_model(&conn).ok().flatten();
        let digest = build_profile_digest(&conn);
        (live, current_user_model, digest)
    };
    if live.is_empty() {
        return;
    }

    let reflections_block = live
        .iter()
        .map(|(_, insight, conf)| format!("- ({conf:.2}) {insight}"))
        .collect::<Vec<_>>()
        .join("\n");
    let user_prompt = format!(
        "Reflections (one per line, with confidence):\n{reflections_block}\n\n\
         Current user_model:\n{}\n\nGrounded facts:\n{}\n\nCurate now.",
        current_user_model.as_deref().unwrap_or("(none yet)"),
        digest.as_deref().unwrap_or("(no facts yet)"),
    );

    let output = match llm
        .complete_text(CONSOLIDATION_COST_TAG, CONSOLIDATION_SYSTEM_PROMPT, &user_prompt, CONSOLIDATION_MAX_TOKENS)
        .await
    {
        Ok(o) => o,
        Err(e) => { tracing::warn!(error = %e, "consolidation: LLM call failed; skipping"); return; }
    };

    let Some(result) = parse_consolidation_output(&output.text) else {
        tracing::warn!("consolidation: unparseable LLM output; skipping (memory untouched)");
        return;
    };
    if !consolidation_should_apply(live.len(), &result) {
        tracing::warn!(
            input = live.len(), output = result.reflections.len(),
            "consolidation: apply-guard rejected output; skipping (memory untouched)"
        );
        return;
    }

    // Apply in ONE transaction (no await inside — guard held safely).
    let before = live.len();
    let after = result.reflections.len();
    {
        let mut guard = match state.db.lock() { Ok(g) => g, Err(_) => return };
        let tx = match guard.transaction() {
            Ok(t) => t,
            Err(e) => { tracing::warn!(error = %e, "consolidation: begin tx failed; skipping"); return; }
        };
        let apply = (|| -> rusqlite::Result<()> {
            tx.execute("UPDATE reflections SET archived_at = datetime('now') WHERE archived_at IS NULL", [])?;
            for (insight, conf) in &result.reflections {
                let id = uuid::Uuid::new_v4().to_string();
                insert_reflection(&tx, &id, insight, *conf, 0)?;
            }
            if let Some(new_um) = &result.user_model {
                if let Some(prior) = &current_user_model {
                    insert_user_model_history(&tx, &uuid::Uuid::new_v4().to_string(), prior)?;
                }
                upsert_user_model(&tx, new_um)?;
            }
            Ok(())
        })();
        match apply.and_then(|()| tx.commit()) {
            Ok(()) => {}
            Err(e) => { tracing::warn!(error = %e, "consolidation: apply tx failed; rolled back"); return; }
        }
    }
    tracing::info!(before, after, "consolidation: deduped reflections + re-grounded user_model");
}
```

- [ ] **Step 2b: Impact analysis before editing `run_once`** — `gitnexus_impact({target:"run_once", direction:"upstream"})`; report callers (expect: only the `engine_sink` turn-count trigger). Proceed if not HIGH/CRITICAL.

- [ ] **Step 3: Add the run_once gate** — in `run_once`, immediately after the daydream gate block (the `if should_run_reflection(turn_count, DAYDREAM_EVERY_N_TURNS) { run_daydream(...).await; }` at ~line 452–455), add:
```rust
    // P5: every 100 agent turns, run the convergent consolidation pass (dedup
    // reflections + re-ground user_model). Gated again inside on a min-count
    // pre-check, so an early sparse `reflections` table just no-ops.
    const CONSOLIDATION_EVERY_N_TURNS: u64 = 100;
    if should_run_reflection(turn_count, CONSOLIDATION_EVERY_N_TURNS) {
        run_consolidation(&state).await;
    }
```

- [ ] **Step 4: Tighten `PROMOTION_SYSTEM_PROMPT`** (root-cause fix for the user_model drift) — change the final sentence (line ~261–262) from:
```rust
fences, no preamble). Prefer concrete, high-signal traits over hedging. Keep it \
under ~120 words.";
```
to:
```rust
fences, no preamble). Prefer concrete, high-signal traits over hedging. CRITICAL: \
state ONLY what the provided facts support — do NOT infer or invent occupation, \
age, identity, or preferences that are not explicitly present in the input. Keep \
it under ~120 words.";
```

- [ ] **Step 5: Verify** — `cargo build 2>&1 | grep -E "^error"` (empty); `cargo test --lib reflection_service 2>&1 | grep "test result"` (all pass); warnings not increased. Then `gitnexus_detect_changes()` — confirm only the expected symbols changed.

- [ ] **Step 6: Commit**:
```bash
git add src-tauri/src/memory_graph/reflection_service.rs
git commit -m "feat(consolidation): run_consolidation pass + run_once gate + promotion no-extrapolation fix

Every 100 agent turns (min 8 live reflections), one LLM call dedups reflections and
re-grounds user_model strictly in the facts. Applied in-place in a single transaction
with an audit trail (archived_at rows + user_model_history); apply-guards reject any
output that grows or blanks memory. Also tightens PROMOTION_SYSTEM_PROMPT to stop the
every-turn drift at its source. Not injected — read-side-transparent."
```

**→ PR1 complete.** Open PR with a `## Commits (bisectable)` table (Tasks 1–4).

---

## PR2 — daydream refinement

### Task 1: `build_daydream_seed` + `parse_daydream_output` (TDD)

**Files:** Modify `src-tauri/src/memory_graph/reflection_service.rs`

- [ ] **Step 1: Write failing tests**:
```rust
    #[test]
    fn build_daydream_seed_mixes_all_sources_and_caps() {
        let refl = vec!["insight one".to_string(), "insight two".to_string(), "insight three".to_string()];
        let titles = vec!["t1".to_string(), "t2".to_string(), "t3".to_string(), "t4".to_string()];
        let seed = build_daydream_seed(&refl, Some("the user model"), &titles);
        assert!(seed.contains("insight one") && seed.contains("insight two"));
        assert!(!seed.contains("insight three")); // capped at 2 reflections
        assert!(seed.contains("the user model"));
        assert!(seed.contains("t1") && seed.contains("t3"));
        assert!(!seed.contains("t4")); // capped at 3 titles
    }

    #[test]
    fn build_daydream_seed_handles_missing_user_model_and_empty_reflections() {
        let seed = build_daydream_seed(&[], None, &["only-title".to_string()]);
        assert!(seed.contains("only-title"));
        assert!(!seed.to_lowercase().contains("user model"));
    }

    #[test]
    fn parse_daydream_output_reads_json_and_falls_back_to_prose() {
        let (c, w) = parse_daydream_output(r#"{"content":"a leap","worth_remembering":true}"#);
        assert_eq!(c, "a leap");
        assert!(w);
        let (c2, w2) = parse_daydream_output("just a plain prose daydream");
        assert_eq!(c2, "just a plain prose daydream");
        assert!(!w2);
    }
```

- [ ] **Step 2: Run** `cargo test --lib daydream_seed 2>&1 | tail -8` and `cargo test --lib parse_daydream 2>&1 | tail -8` — FAIL.

- [ ] **Step 3: Implement** (near the daydream consts; `extract_json_object` already exists from PR1 Task 3):
```rust
/// Build the daydream seed from grounded sources + some randomness. Up to 2 recent
/// reflections + the user_model + up to 3 random titles. Keeping random titles
/// preserves divergence; the reflections/user_model make associations more coherent.
fn build_daydream_seed(reflections: &[String], user_model: Option<&str>, titles: &[String]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for r in reflections.iter().take(2) {
        parts.push(format!("Reflection: {}", r.chars().take(400).collect::<String>()));
    }
    if let Some(um) = user_model.map(str::trim).filter(|u| !u.is_empty()) {
        parts.push(format!("User model: {}", um.chars().take(400).collect::<String>()));
    }
    for t in titles.iter().take(3) {
        parts.push(t.chars().take(400).collect::<String>());
    }
    parts.join("\n")
}

/// Parse the daydream output `{content, worth_remembering}`. Prose (no JSON) falls
/// back to `(prose, false)` — a free-form daydream that doesn't reflow.
pub fn parse_daydream_output(s: &str) -> (String, bool) {
    if let Some(obj) = extract_json_object(s.trim()) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(obj) {
            if let Some(content) = v
                .get("content")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|c| !c.is_empty())
            {
                let worth = v.get("worth_remembering").and_then(serde_json::Value::as_bool).unwrap_or(false);
                return (content.to_string(), worth);
            }
        }
    }
    (s.trim().to_string(), false)
}
```

- [ ] **Step 4: Run** both filters — PASS. `cargo build 2>&1 | grep -E "^error"` (empty).

- [ ] **Step 5: Commit**:
```bash
git add src-tauri/src/memory_graph/reflection_service.rs
git commit -m "feat(daydream): build_daydream_seed + parse_daydream_output helpers (TDD)"
```

### Task 2: wire `run_daydream` — better seed + reflow (build-green)

**Files:** Modify `src-tauri/src/memory_graph/reflection_service.rs`

- [ ] **Step 1: Impact analysis** — `gitnexus_impact({target:"run_daydream", direction:"upstream"})`; expect only `run_once` calls it. Proceed if not HIGH/CRITICAL.

- [ ] **Step 2: Update `DAYDREAM_SYSTEM_PROMPT`** (lines ~557–562) to pin the new JSON contract — append the output shape, keeping the divergent framing:
```rust
const DAYDREAM_SYSTEM_PROMPT: &str = "\
You are free-associating. Be creative and speculative. From these memories \
(some are durable reflections about the user, some are random), generate ONE novel \
hypothesis, connection, or idea — something non-obvious that links them or leaps \
off from them. Favour the surprising over the safe. Respond with ONLY a JSON \
object, no prose, no markdown fences:\n\
{\"content\":\"<one short paragraph>\",\"worth_remembering\":<true if this is a \
genuinely useful durable insight about the user/project, false if it's just a fun leap>}";
```

- [ ] **Step 3: Replace the seed block** in `run_daydream` (lines ~607–640). Read the recent reflections + user_model + random titles inside the borrow-safe block, then build the seed via the helper:
```rust
    // Build the seed from grounded sources + randomness (hold the std Mutex only
    // for the reads; drop before the await).
    let seed = {
        let Ok(conn) = state.db.lock() else { return };
        let reflections: Vec<String> = recent_reflections(&conn, 2)
            .map(|rows| rows.into_iter().map(|r| r.insight).collect())
            .unwrap_or_default();
        let user_model = get_user_model(&conn).ok().flatten();
        let titles: Vec<String> = match conn.prepare(
            "SELECT title FROM memory_nodes
             WHERE title IS NOT NULL AND title != ''
             ORDER BY RANDOM() LIMIT ?1",
        ) {
            Ok(mut stmt) => match stmt.query_map(params![DAYDREAM_SEED_TITLES], |r| r.get::<_, String>(0)) {
                Ok(rows) => rows.flatten().collect(),
                Err(e) => { tracing::warn!(error = %e, "daydream: title query_map failed; skipping"); return; }
            },
            Err(e) => { tracing::warn!(error = %e, "daydream: title prepare failed; skipping"); return; }
        };
        if reflections.is_empty() && titles.is_empty() && user_model.is_none() {
            tracing::debug!("daydream: no seed material; skipping");
            return;
        }
        build_daydream_seed(&reflections, user_model.as_deref(), &titles)
    };
```

- [ ] **Step 4: Parse the output + reflow.** After the LLM `complete_text` call returns `output`, replace the direct `insert_daydream(&output.text)` usage with the parsed form, and reflow when worth-remembering. Locate the persist+emit block (after the LLM call) and make it:
```rust
    let (content, worth_remembering) = parse_daydream_output(&output.text);
    if content.is_empty() {
        tracing::debug!("daydream: empty content after parse; skipping");
        return;
    }
    let id = uuid::Uuid::new_v4().to_string();
    {
        let Ok(conn) = state.db.lock() else { return };
        if let Err(e) = insert_daydream(&conn, &id, &content) {
            tracing::warn!(error = %e, "daydream: insert_daydream failed");
            return;
        }
        // P5 reflow: a high-value daydream re-enters the convergent pipeline as a
        // low-confidence reflection (0.4 < real reflections), where consolidation
        // will later dedup/merge it. Closes the divergent→convergent loop.
        if worth_remembering {
            let rid = uuid::Uuid::new_v4().to_string();
            if let Err(e) = insert_reflection(&conn, &rid, &content, 0.4, 0) {
                tracing::warn!(error = %e, "daydream: reflow insert_reflection failed");
            } else {
                tracing::info!(%rid, "daydream: reflowed a high-value daydream into a reflection");
            }
        }
    }
    // Keep the EXISTING `app.emit("agent:daydream", …)` call verbatim — same event
    // name and same payload keys the UI already consumes (e.g. `content`,
    // `created_at`); only make its `content` field carry the parsed `content`
    // variable (was `output.text`/`text`). Do NOT drop or rename existing keys.
    tracing::info!(%id, worth_remembering, chars = content.len(), "daydream: free-associated a new daydream");
```
> First READ the current tail of `run_daydream` (~lines 641–690): it builds `user_prompt` from `seed`, calls `complete_text` into some `output`/`text` var, then inserts + emits. Keep the `user_prompt`/`complete_text` call as-is; only the seed block (Step 3) and the persist/reflow/emit tail (Step 4) change. Match the actual variable name the current code uses for the LLM result.

- [ ] **Step 5: Verify** — `cargo build 2>&1 | grep -E "^error"` (empty); `cargo test --lib reflection_service 2>&1 | grep "test result"` (all pass); warnings not increased; `gitnexus_detect_changes()` confirms only `run_daydream` + the new helpers changed.

- [ ] **Step 6: Commit**:
```bash
git add src-tauri/src/memory_graph/reflection_service.rs
git commit -m "feat(daydream): ground seeds in reflections+user_model + reflow high-value daydreams to reflections

Seed now mixes recent reflections + user_model + random titles (more coherent while
staying divergent). The daydream LLM emits {content, worth_remembering}; a worth-keeping
daydream is also inserted as a low-confidence (0.4) reflection, so it enters prompt
injection + the consolidation pipeline. UI emit unchanged."
```

**→ PR2 complete.** Open PR with a `## Commits (bisectable)` table (Tasks 1–2).

---

## Self-Review

- **Spec coverage:** consolidation pass (PR1 T4) ✓ · in-place + audit (archived_at T2 + user_model_history T1 + transaction T4) ✓ · LLM-only one-call (T4) ✓ · cadence 100 + min-8 (T4) ✓ · defenses (T3 guard + T4 wiring) ✓ · promotion root-cause fix (T4 S4) ✓ · daydream seed quality (PR2 T1/T2) ✓ · daydream reflow (PR2 T2) ✓ · V59 (T1) ✓ · recent_reflections archived filter (T2) ✓.
- **Type consistency:** `ConsolidationResult{reflections: Vec<(String,f64)>, user_model: Option<String>}` used identically in T3 + T4. `parse_daydream_output -> (String, bool)` matches T2 usage. `insert_reflection(&Connection,...)` accepts `&tx` via deref coercion. `recent_reflections` returns `ReflectionRow{insight,..}` (used in T2 seed via `.insight`). `apply_reflections_schema` column add (T2) keeps existing `insert_reflection` signature.
- **Placeholder scan:** every code step has complete code; the only "adapt to surrounding lines" note (PR2 T2 S4) is bounded + explicit about what to preserve.
- **Borrow-safety:** every `state.db.lock()` guard is dropped before the next `.await`; the T4 transaction block contains no await.
- **Migration safety:** V59 additive (`ALTER ADD COLUMN` + `CREATE IF NOT EXISTS`); re-confirm number at commit (T1 S1).

## Execution Handoff

Subagent-Driven (user standing instruction). One implementer subagent per task; controller reviews build/tests/warnings + spec-compliance at each task, opens PR1 after Task 4, PR2 after its Task 2. Two-stage review (spec then quality) at PR boundaries.
