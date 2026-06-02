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

/// System prompt for the reflection pass. Pins the JSON output contract so
/// `parse_reflection_output` has a stable shape to parse.
const REFLECTION_SYSTEM_PROMPT: &str = "\
You are a reflection agent. Read the recent conversation between a user and an \
AI assistant and distill ONE durable, high-value insight about the user, their \
goals, working style, or the project — something worth remembering across future \
sessions. Ignore one-off chit-chat. Respond with ONLY a JSON object, no prose, \
no markdown fences:\n\
{\"insight\": \"<one concise sentence>\", \"confidence\": <float 0.0-1.0>}";

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

    // P3-②: run_promotion(state).await — distill facts → user_model here, after
    // the reflection pass, so both distillations share one `run_once` trigger.
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
}
