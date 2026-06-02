//! ReflectionService — Phase 3 growth (mem.md "Reflection Agent").
//!
//! Every N agent turns (turn-count trigger, NOT a wall-clock loop), distill the
//! recent conversation into a `Reflection{insight, confidence}` row. Recent
//! reflections are injected back into the pi prompt (see
//! `agent/memory_context.rs::PiPromptContext.reflections`).
//!
//! The trigger lives in `engine_sink::persist_assistant`: an `AtomicU64` on
//! `AppState` counts agent turns and fire-and-forget spawns [`run_once`] when
//! `should_run_reflection(count, N)`. `run_once` is best-effort end to end —
//! any error logs and returns, never panics, so a reflection failure can never
//! break a live turn.
//!
//! ## What's pure vs orchestrated
//!
//! Pure (unit-tested): [`parse_reflection_output`], [`should_run_reflection`],
//! and the reflections store (`apply_reflections_schema` / [`insert_reflection`]
//! / [`recent_reflections`]). The LLM orchestration in [`run_once`] is
//! build-green only (it needs a live provider + `AppState`).
//!
//! ## P3-②
//!
//! A later PR adds a `run_promotion` pass (facts → `user_model`) called at the
//! end of `run_once`; the hook-point marker is left in place below.

use rusqlite::{params, Connection};

/// How many recent `agent_messages` rows feed one reflection pass.
const REFLECTION_EVENT_WINDOW: usize = 50;
/// Max tokens for the reflection completion (one short JSON object).
const REFLECTION_MAX_TOKENS: u32 = 512;
/// Cost tag prefix written into `cost_records.model` for reflection LLM calls.
const REFLECTION_COST_TAG: &str = "memory_reflection";
/// Max tokens for the promotion completion (one short persona summary).
const PROMOTION_MAX_TOKENS: u32 = 400;
/// Cost tag prefix written into `cost_records.model` for promotion LLM calls.
const PROMOTION_COST_TAG: &str = "memory_promotion";
/// Fixed singleton id for the `user_model` row (one row per install).
const USER_MODEL_ID: &str = "default";

/// Parse the LLM's reflection completion into `(insight, confidence)`.
///
/// Expects `{"insight": "...", "confidence": 0.8}`. On any parse failure (the
/// model returned prose, wrapped the JSON in markdown, etc.) it falls back to
/// `(s.trim(), 0.5)` so the pass still records *something* useful rather than
/// dropping the turn. Confidence is clamped to `[0.0, 1.0]`.
pub fn parse_reflection_output(s: &str) -> (String, f64) {
    match serde_json::from_str::<serde_json::Value>(s.trim()) {
        Ok(v) => {
            let insight = v
                .get("insight")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|i| !i.is_empty());
            match insight {
                Some(insight) => {
                    let conf = v
                        .get("confidence")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.5)
                        .clamp(0.0, 1.0);
                    (insight.to_string(), conf)
                }
                // Valid JSON but no usable `insight` field → treat the raw text
                // as the insight (defensive; matches the prose fallback).
                None => (s.trim().to_string(), 0.5),
            }
        }
        Err(_) => (s.trim().to_string(), 0.5),
    }
}

/// Turn-count trigger predicate: fire on every `n`-th turn. `n == 0` (disabled)
/// and `count == 0` (no turns yet) never fire.
pub fn should_run_reflection(count: u64, n: u64) -> bool {
    n > 0 && count > 0 && count % n == 0
}

/// One reflection row, as read back for prompt injection.
#[derive(Debug, Clone)]
pub struct ReflectionRow {
    pub insight: String,
    pub confidence: f64,
    pub created_at: String,
}

/// Apply the V57 `reflections` DDL to a bare connection. Mirrors the migration
/// block exactly; used by unit tests against `:memory:` so they don't drag in
/// the whole migration stack.
pub fn apply_reflections_schema(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS reflections (
            id                 TEXT PRIMARY KEY,
            insight            TEXT NOT NULL,
            confidence         REAL NOT NULL DEFAULT 0.5,
            source_event_count INTEGER NOT NULL DEFAULT 0,
            created_at         TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_reflections_created ON reflections(created_at DESC);",
    )
    .expect("apply_reflections_schema");
}

/// Insert one reflection. `created_at` is left to the column default
/// (`datetime('now')`). Best-effort callers map the `Result` to a log line.
pub fn insert_reflection(
    conn: &Connection,
    id: &str,
    insight: &str,
    confidence: f64,
    source_event_count: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO reflections (id, insight, confidence, source_event_count)
         VALUES (?1, ?2, ?3, ?4)",
        params![id, insight, confidence, source_event_count],
    )?;
    Ok(())
}

/// Most-recent reflections, newest first, capped at `limit`.
pub fn recent_reflections(conn: &Connection, limit: usize) -> rusqlite::Result<Vec<ReflectionRow>> {
    let mut stmt = conn.prepare(
        "SELECT insight, confidence, created_at
         FROM reflections
         ORDER BY created_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit as i64], |r| {
            Ok(ReflectionRow {
                insight: r.get(0)?,
                confidence: r.get(1)?,
                created_at: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ─── user_model store (P3-②: Pattern→Model layer) ──────────────────────────

/// Apply the V57 `user_model` DDL to a bare connection. Mirrors the migration
/// block exactly; used by unit tests against `:memory:` so they don't drag in
/// the whole migration stack. (The real table is created by the V57 migration.)
pub fn apply_user_model_schema(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS user_model (
            id          TEXT PRIMARY KEY,
            summary     TEXT NOT NULL,
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .expect("apply_user_model_schema");
}

/// Upsert the singleton `user_model` row (fixed id [`USER_MODEL_ID`]). Each
/// promotion pass overwrites the previous summary so there is always exactly one
/// row. Best-effort callers map the `Result` to a log line.
pub fn upsert_user_model(conn: &Connection, summary: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO user_model (id, summary, updated_at)
         VALUES (?1, ?2, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET
             summary = excluded.summary,
             updated_at = excluded.updated_at",
        params![USER_MODEL_ID, summary],
    )?;
    Ok(())
}

/// Read the singleton `user_model` summary, if one has been distilled. Returns
/// `Ok(None)` when the table is empty (no promotion has run yet).
pub fn get_user_model(conn: &Connection) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT summary FROM user_model WHERE id = ?1",
        params![USER_MODEL_ID],
        |r| r.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
}

// ─── daydreams store (P4: divergent free-association pass) ──────────────────

/// One daydream row, as read back for the UI surface (`agent:daydream` event /
/// a recent-daydreams view). Mirrors the V58 `daydreams` columns we read.
#[derive(Debug, Clone)]
pub struct DaydreamRow {
    pub content: String,
    pub created_at: String,
}

/// Apply the V58 `daydreams` DDL to a bare connection. Mirrors the migration
/// block exactly; used by unit tests against `:memory:` so they don't drag in
/// the whole migration stack. (The real table is created by the V58 migration.)
pub fn apply_daydreams_schema(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS daydreams (
            id          TEXT PRIMARY KEY,
            content     TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_daydreams_created ON daydreams(created_at DESC);",
    )
    .expect("apply_daydreams_schema");
}

/// Insert one daydream. `created_at` is left to the column default
/// (`datetime('now')`). Best-effort callers map the `Result` to a log line.
pub fn insert_daydream(conn: &Connection, id: &str, content: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO daydreams (id, content) VALUES (?1, ?2)",
        params![id, content],
    )?;
    Ok(())
}

/// Most-recent daydreams, newest first, capped at `limit`.
pub fn recent_daydreams(conn: &Connection, limit: usize) -> rusqlite::Result<Vec<DaydreamRow>> {
    let mut stmt = conn.prepare(
        "SELECT content, created_at
         FROM daydreams
         ORDER BY created_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit as i64], |r| {
            Ok(DaydreamRow {
                content: r.get(0)?,
                created_at: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// System prompt for the reflection pass. Pins the JSON output contract so
/// `parse_reflection_output` has a stable shape to parse.
const REFLECTION_SYSTEM_PROMPT: &str = "\
You are a reflection agent. Read the recent conversation between a user and an \
AI assistant and distill ONE durable, high-value insight about the user, their \
goals, working style, or the project — something worth remembering across future \
sessions. Ignore one-off chit-chat. Respond with ONLY a JSON object, no prose, \
no markdown fences:\n\
{\"insight\": \"<one concise sentence>\", \"confidence\": <float 0.0-1.0>}";

/// System prompt for the promotion pass. Distills the user's learned facets +
/// profile facts into one compact persona/preferences summary (the Pattern→Model
/// layer). Plain prose out — this is stored verbatim as the `user_model` and
/// injected into the pi prompt, so no JSON contract here.
const PROMOTION_SYSTEM_PROMPT: &str = "\
You are a user-modeling agent. You are given a list of learned facts, rules, and \
preferences about a single user (their identity, tooling, working style, vetoes, \
and goals). Synthesize them into ONE compact, durable user model: a few sentences \
capturing who this user is and how they prefer to work, so a future assistant can \
serve them well without re-learning. Write plain prose (no JSON, no markdown \
fences, no preamble). Prefer concrete, high-signal traits over hedging. Keep it \
under ~120 words.";

/// Read up to [`REFLECTION_EVENT_WINDOW`] recent `agent_messages` (role+content,
/// newest first), then re-order oldest→newest into a transcript string for the
/// LLM. Returns `None` when there's nothing to reflect on.
fn build_recent_transcript(conn: &Connection) -> Option<String> {
    let mut stmt = conn
        .prepare(
            "SELECT role, content FROM agent_messages
             ORDER BY created_at DESC LIMIT ?1",
        )
        .ok()?;
    let mut rows: Vec<(String, String)> = stmt
        .query_map(params![REFLECTION_EVENT_WINDOW as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .ok()?
        .filter_map(Result::ok)
        .collect();
    if rows.is_empty() {
        return None;
    }
    // Query was DESC (newest first); flip to chronological for the transcript.
    rows.reverse();
    let mut transcript = String::new();
    for (role, content) in &rows {
        // Bound any single message so one huge paste can't blow the prompt.
        let snippet: String = content.chars().take(2_000).collect();
        transcript.push_str(role);
        transcript.push_str(": ");
        transcript.push_str(&snippet);
        transcript.push('\n');
    }
    Some(transcript)
}

/// Read the user's learned facets (`user_profile_facets.class/name/value`) +
/// learned profile facts (`memory_nodes WHERE kind='user_profile'`.title) into a
/// single newline-delimited digest for the promotion LLM. Returns `None` when
/// BOTH sources are empty (nothing to model yet). Each line is bounded so a huge
/// stored value can't blow the prompt.
fn build_profile_digest(conn: &Connection) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();

    if let Ok(mut stmt) =
        conn.prepare("SELECT class, name, value FROM user_profile_facets ORDER BY class, name")
    {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        }) {
            for (class, name, value) in rows.flatten() {
                let value: String = value.chars().take(400).collect();
                lines.push(format!("- [{class}] {name}: {value}"));
            }
        }
    }

    if let Ok(mut stmt) = conn.prepare(
        "SELECT title FROM memory_nodes WHERE kind = 'user_profile' ORDER BY created_at DESC",
    ) {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for title in rows.flatten() {
                let title: String = title.chars().take(400).collect();
                lines.push(format!("- {title}"));
            }
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Run one reflection pass: read recent turns → ask the LLM for a JSON insight →
/// persist a `reflections` row. Best-effort end to end; every failure path logs
/// and returns. Spawned fire-and-forget from the turn-count trigger.
pub async fn run_once(app: tauri::AppHandle) {
    use tauri::Manager;

    let Some(state) = app.try_state::<crate::app::AppState>() else {
        return;
    };

    // LLM presence gate — no configured learning LLM ⇒ nothing to do.
    let Some(llm) = state.learning_llm.clone() else {
        tracing::debug!("reflection: no learning_llm configured; skipping");
        return;
    };

    // Daily-budget gate — reuse the learning extractor's budget knob + the
    // `cost_store::today_learning_tokens` rollup. Skip when disabled (budget 0)
    // or already over budget for the day. (Reflection's own spend lands under
    // the `memory_reflection:` cost prefix, so it doesn't itself count toward
    // `memory_learning%`; this gate is the shared "daily learning budget burned"
    // signal, matching the plan's reuse of the extractor pattern.)
    let daily_budget = {
        let cfg = state.memubot_config.read().await;
        cfg.memory_os.learning_llm_daily_token_budget
    };
    if daily_budget == 0 {
        tracing::debug!("reflection: learning LLM daily budget is 0; skipping");
        return;
    }
    let spent = crate::cost_store::today_learning_tokens(&state.db);
    if spent >= daily_budget {
        tracing::debug!(
            spent,
            daily_budget,
            "reflection: daily learning budget exhausted; skipping"
        );
        return;
    }

    // Read recent turns. Hold the std::sync::Mutex only for the read, then drop
    // it before the `.await` (the guard is not `Send`).
    let (transcript, event_count) = {
        let Ok(conn) = state.db.lock() else {
            return;
        };
        let Some(t) = build_recent_transcript(&conn) else {
            tracing::debug!("reflection: no recent agent_messages; skipping");
            return;
        };
        let n = t.lines().count() as i64;
        (t, n)
    };

    let user_prompt = format!(
        "Recent conversation (oldest first):\n\n{transcript}\n\nDistill one insight as JSON."
    );
    let output = match llm
        .complete_text(
            REFLECTION_COST_TAG,
            REFLECTION_SYSTEM_PROMPT,
            &user_prompt,
            REFLECTION_MAX_TOKENS,
        )
        .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!(error = %e, "reflection: LLM call failed; skipping");
            return;
        }
    };

    let (insight, confidence) = parse_reflection_output(&output.text);
    if insight.trim().is_empty() {
        tracing::debug!("reflection: empty insight after parse; skipping");
        return;
    }

    let id = uuid::Uuid::new_v4().to_string();
    {
        let Ok(conn) = state.db.lock() else {
            return;
        };
        if let Err(e) = insert_reflection(&conn, &id, &insight, confidence, event_count) {
            tracing::warn!(error = %e, "reflection: insert_reflection failed");
            return;
        }
    }
    tracing::info!(
        %id,
        confidence,
        events = event_count,
        "reflection: distilled and stored a new reflection"
    );

    // P3-②: distill facts + profile nodes → user_model here, after the
    // reflection pass, so both distillations share one `run_once` trigger. The
    // run_once budget/LLM gates above already passed; `run_promotion` is itself
    // best-effort and re-reads the budget defensively.
    run_promotion(&state).await;
}

/// Run one promotion pass: read the user's learned facets + profile facts → ask
/// the LLM for a compact persona/preferences summary → upsert the singleton
/// `user_model` row. Best-effort end to end; every failure path logs and
/// returns. Called at the tail of [`run_once`] (the Pattern→Model layer of
/// mem.md's Event→Fact→Pattern→Model chain).
pub async fn run_promotion(state: &crate::app::AppState) {
    // LLM presence gate — no configured learning LLM ⇒ nothing to do.
    let Some(llm) = state.learning_llm.clone() else {
        tracing::debug!("promotion: no learning_llm configured; skipping");
        return;
    };

    // Daily-budget gate — same shared "daily learning budget burned" signal as
    // the reflection pass (the extractor's budget knob + `today_learning_tokens`
    // rollup). Defensive re-check: `run_once` already gated, but `run_promotion`
    // is `pub` and best-effort, so it owns its own gate.
    let daily_budget = {
        let cfg = state.memubot_config.read().await;
        cfg.memory_os.learning_llm_daily_token_budget
    };
    if daily_budget == 0 {
        tracing::debug!("promotion: learning LLM daily budget is 0; skipping");
        return;
    }
    let spent = crate::cost_store::today_learning_tokens(&state.db);
    if spent >= daily_budget {
        tracing::debug!(
            spent,
            daily_budget,
            "promotion: daily learning budget exhausted; skipping"
        );
        return;
    }

    // Read the profile digest. Hold the std::sync::Mutex only for the read, then
    // drop it before the `.await` (the guard is not `Send`).
    let digest = {
        let Ok(conn) = state.db.lock() else {
            return;
        };
        let Some(d) = build_profile_digest(&conn) else {
            tracing::debug!("promotion: no facets or profile nodes; skipping");
            return;
        };
        d
    };

    let user_prompt = format!(
        "Learned facts, rules, and preferences about the user:\n\n{digest}\n\n\
         Synthesize the user model now."
    );
    let output = match llm
        .complete_text(
            PROMOTION_COST_TAG,
            PROMOTION_SYSTEM_PROMPT,
            &user_prompt,
            PROMOTION_MAX_TOKENS,
        )
        .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!(error = %e, "promotion: LLM call failed; skipping");
            return;
        }
    };

    let summary = output.text.trim();
    if summary.is_empty() {
        tracing::debug!("promotion: empty summary from LLM; skipping");
        return;
    }

    {
        let Ok(conn) = state.db.lock() else {
            return;
        };
        if let Err(e) = upsert_user_model(&conn, summary) {
            tracing::warn!(error = %e, "promotion: upsert_user_model failed");
            return;
        }
    }
    tracing::info!(
        chars = summary.len(),
        "promotion: distilled facts + profile nodes into user_model"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reflection_extracts_insight_and_confidence() {
        // LLM was asked to output JSON {"insight": "...", "confidence": 0.8}.
        let (insight, conf) = parse_reflection_output(
            r#"{"insight":"user is building an agent framework","confidence":0.82}"#,
        );
        assert_eq!(insight, "user is building an agent framework");
        assert!((conf - 0.82).abs() < 1e-6);
        // Unparseable → confidence defaults to 0.5, insight is the trimmed raw text.
        let (i2, c2) = parse_reflection_output("just some prose");
        assert_eq!(i2, "just some prose");
        assert!((c2 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn reflections_store_inserts_and_reads_recent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        apply_reflections_schema(&conn);
        insert_reflection(&conn, "id1", "insight A", 0.7, 50).unwrap();
        insert_reflection(&conn, "id2", "insight B", 0.9, 60).unwrap();
        let recent = recent_reflections(&conn, 5).unwrap();
        assert_eq!(recent.len(), 2);
        assert!(recent.iter().any(|r| r.insight == "insight B"));
    }

    #[test]
    fn should_run_reflection_every_n_turns() {
        assert!(should_run_reflection(20, 20)); // count, n
        assert!(should_run_reflection(40, 20));
        assert!(!should_run_reflection(19, 20));
        assert!(!should_run_reflection(0, 20)); // 0 never fires
    }

    #[test]
    fn user_model_upserts_single_row() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        apply_user_model_schema(&conn);
        upsert_user_model(&conn, "Ryan, engineer, Rust").unwrap();
        upsert_user_model(&conn, "Ryan Liu, Apple PKG PD, Rust+SwiftUI").unwrap();
        assert_eq!(
            get_user_model(&conn).unwrap().as_deref(),
            Some("Ryan Liu, Apple PKG PD, Rust+SwiftUI")
        );
    }

    #[test]
    fn daydreams_store_inserts_and_reads_recent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        apply_daydreams_schema(&conn);
        insert_daydream(&conn, "id1", "what if agents dream in graphs?").unwrap();
        insert_daydream(&conn, "id2", "rust borrow-checker as a memory model").unwrap();
        let recent = recent_daydreams(&conn, 5).unwrap();
        assert_eq!(recent.len(), 2);
        assert!(recent.iter().any(|d| d.content.contains("borrow-checker")));
    }
}
