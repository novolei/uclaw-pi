//! [R2 消息核心闭环] Backend-only persistence for the PiEngine chat path.
//!
//! F2 (stateless pi): uClaw SQLite is the **single source of truth** — pi keeps
//! no session on disk, so the streamed conversation must be written into uClaw's
//! `messages` table (no pi storage, no double-write) for `get_messages` to render
//! it 1:1 after the frontend's `chat:stream-complete` → refresh.
//!
//! Both writes go through [`persist_chat_text_message`]: the user message on send
//! (`tauri_commands::send_message`, gated) and the assistant message on complete
//! (`engine_sink::TauriEventSink::emit`, gated). UI stays read-only (R2 scope).

use rusqlite::Connection;

use crate::agent::types::ContentBlock;

/// Per-turn token/cost/duration for an assistant row → the `input_tokens`,
/// `output_tokens`, `cost_usd`, `duration_ms` columns `get_agent_session_messages`
/// reads into `usage` + `durationMs` (the "⚡ 耗时 · N 输入 · M 输出 · $费用" badge).
/// `None` fields write SQL NULL.
#[derive(Default)]
pub struct TurnUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<i64>,
}

impl TurnUsage {
    /// Parse an `agent:turn_cost` payload (`{inputTokens, outputTokens,
    /// costUsd:"$x", durationMs}`) into the columns. Zero token counts collapse to
    /// `None` so a no-usage turn doesn't render a "0 输入" badge.
    #[must_use]
    pub fn from_turn_cost(v: &serde_json::Value) -> Self {
        let positive = |n: i64| (n > 0).then_some(n);
        Self {
            input_tokens: v.get("inputTokens").and_then(serde_json::Value::as_i64).and_then(positive),
            output_tokens: v.get("outputTokens").and_then(serde_json::Value::as_i64).and_then(positive),
            cost_usd: v
                .get("costUsd")
                .and_then(serde_json::Value::as_str)
                .and_then(|s| s.trim_start_matches('$').parse::<f64>().ok()),
            duration_ms: v.get("durationMs").and_then(serde_json::Value::as_i64),
        }
    }
}

/// Insert one message into `messages`, encoding `text` as the same
/// `Option<Vec<ContentBlock>>` JSON shape `get_messages` parses — so it
/// round-trips to `[{"type":"text","text":…}]` (snake_case, the wire shape
/// `NativeBlockRenderer` consumes). `created_at` is RFC3339 like the legacy path
/// (`agent/session.rs`). Callers generate `id`.
///
/// Only the base columns + `reasoning` are written; the migration-added
/// `tool_activities_json` / `model` columns are nullable and left to history
/// enrichment (a later slice). Errors propagate so callers can log them.
pub fn persist_chat_text_message(
    conn: &Connection,
    id: &str,
    conversation_id: &str,
    role: &str,
    text: &str,
    reasoning: Option<&str>,
) -> rusqlite::Result<()> {
    let blocks: Option<Vec<ContentBlock>> = Some(vec![ContentBlock::Text {
        text: text.to_owned(),
    }]);
    let content = serde_json::to_string(&blocks).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, created_at, reasoning) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            id,
            conversation_id,
            role,
            content,
            chrono::Utc::now().to_rfc3339(),
            reasoning,
        ],
    )?;
    Ok(())
}

/// [R3 agent path] Persist into `agent_messages` (the Agent view's table, read by
/// `get_agent_session_messages`). `session_id` must reference an existing
/// `agent_sessions` row (FK). Two shapes MUST match that reader exactly or rows
/// are silently dropped: `content` is JSON `[{"type":"text",…}]` (parsed as
/// `Vec<ContentBlock>`), and `created_at` is **epoch millis `i64`** — the column
/// is `INTEGER` and the reader does `row.get::<i64>` then `filter_map(.ok())`,
/// which discards any row whose read fails (an RFC3339 string vanishes every
/// refresh — the "agent messages disappear after the turn ends" bug).
pub fn persist_agent_text_message(
    conn: &Connection,
    id: &str,
    session_id: &str,
    role: &str,
    text: &str,
    reasoning: Option<&str>,
    usage: &TurnUsage,
) -> rusqlite::Result<()> {
    // Content shape is role-specific, matching the legacy backend + the Agent view:
    // • user → PLAIN TEXT. The user bubble renders `message.content` directly
    //   (AgentMessages.tsx:704 → parseAttachedFiles), so a JSON array would show
    //   literally as `[{"type":"text",…}]`. Legacy user rows are plain text ("hi").
    // • assistant → JSON `[{"type":"text",…}]`, which the assistant branch parses
    //   via NativeBlockRenderer.
    // (The "messages disappear" bug was `created_at` typing, NOT the content shape —
    //  see the doc above; that's why user can safely be plain text again.)
    let content = if role == "user" {
        text.to_owned()
    } else if let Some(r) = reasoning.filter(|s| !s.is_empty()) {
        // Assistant WITH thinking → a leading `thinking` block then the text block,
        // matching the legacy shape NativeBlockRenderer renders
        // (`{"type":"thinking","thinking":…,"signature":null}`). Built as raw JSON
        // so it round-trips through the `Vec<ContentBlock>` reader.
        serde_json::json!([
            { "type": "thinking", "thinking": r, "signature": null },
            { "type": "text", "text": text },
        ])
        .to_string()
    } else {
        let blocks: Option<Vec<ContentBlock>> = Some(vec![ContentBlock::Text {
            text: text.to_owned(),
        }]);
        serde_json::to_string(&blocks).unwrap_or_else(|_| "[]".into())
    };
    conn.execute(
        "INSERT INTO agent_messages \
         (id, session_id, role, content, created_at, reasoning, \
          input_tokens, output_tokens, cost_usd, duration_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            id,
            session_id,
            role,
            content,
            // Epoch millis (i64) — agent_messages.created_at is INTEGER and the
            // reader reads it as i64; an RFC3339 string is dropped (see doc above).
            chrono::Utc::now().timestamp_millis(),
            reasoning,
            // Token/cost/duration columns → get_agent_session_messages builds
            // `usage` + `durationMs` from these for the metadata badge. None ⇒ NULL.
            usage.input_tokens,
            usage.output_tokens,
            usage.cost_usd,
            usage.duration_ms,
        ],
    )?;
    Ok(())
}

// NOTE: the workspace-cwd resolvers (`space_cwd_for_agent_session` /
// `_for_conversation`) moved to `services::workspace_service` — workspace
// resolution is a distinct concern from message persistence (ADR 2026-05-31).

/// Fire-and-forget: feed one just-persisted conversation turn into bucket_seal's
/// hierarchical memory tree (`canonicalize::chat` → seal cascade) so the live
/// agent/chat stream actually populates the openhuman bucket-seal store — closing
/// the "registered as default but zero ingest" gap.
///
/// `namespace` is hard-wired to `"global"` so ingested turns share the tree the
/// prompt-recall supplement queries (`route_recall_in(..., "global", ...)`);
/// otherwise the data would be unreachable by recall. Spawned on Tauri's runtime
/// (safe even from the engine thread's synchronous EventSink callback, where
/// `tokio::spawn` would panic). Best-effort: empty text is skipped and any error
/// is logged and dropped — it never blocks or fails the turn.
pub fn spawn_bucket_seal_ingest(
    adapter: std::sync::Arc<crate::memory_bucket_seal::BucketSealAdapter>,
    session_id: String,
    author: String,
    text: String,
) {
    if text.trim().is_empty() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if let Err(e) = adapter
            .ingest_chat_turn("global", &author, &text, Some(&session_id))
            .await
        {
            tracing::debug!(error = %e, session_id = %session_id, role = %author, "bucket_seal chat ingest failed");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `messages` columns this module writes (mirrors db/migrations.rs base
    /// table + the `reasoning` migration column). No FK so the test is isolated.
    fn make_schema(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE messages (
                id              TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role            TEXT NOT NULL CHECK(role IN ('user','assistant','system')),
                content         TEXT NOT NULL,
                created_at      TEXT NOT NULL,
                reasoning       TEXT
            );",
        )
        .unwrap();
    }

    /// [R2 Done-when#3] A persisted message reads back as the exact
    /// `Option<Vec<ContentBlock>>` shape `get_messages` parses, so the chat-path
    /// loop renders 1:1.
    #[test]
    fn persisted_text_round_trips_as_get_messages_shape() {
        let conn = Connection::open_in_memory().unwrap();
        make_schema(&conn);

        persist_chat_text_message(&conn, "m1", "c1", "assistant", "hello world", Some("ponder"))
            .unwrap();

        let (role, content, reasoning): (String, String, Option<String>) = conn
            .query_row(
                "SELECT role, content, reasoning FROM messages WHERE id = 'm1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();

        assert_eq!(role, "assistant");
        assert_eq!(reasoning.as_deref(), Some("ponder"));

        // Parse exactly as get_messages does (Option<Vec<ContentBlock>>).
        let blocks: Option<Vec<ContentBlock>> = serde_json::from_str(&content).unwrap();
        let blocks = blocks.expect("Some(blocks)");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Text { text } => assert_eq!(text, "hello world"),
            other => panic!("expected Text block, got {other:?}"),
        }
        // Raw JSON is the snake_case wire shape NativeBlockRenderer switches on.
        assert!(content.contains("\"type\":\"text\""), "got: {content}");
    }

    #[test]
    fn role_check_constraint_accepts_user_and_assistant() {
        let conn = Connection::open_in_memory().unwrap();
        make_schema(&conn);
        persist_chat_text_message(&conn, "u1", "c1", "user", "hi", None).unwrap();
        persist_chat_text_message(&conn, "a1", "c1", "assistant", "yo", None).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages WHERE conversation_id='c1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
    }

    /// Regression: agent rows MUST store `created_at` as epoch-millis `i64`.
    /// `get_agent_session_messages` reads it with `row.get::<i64>` and
    /// `filter_map(.ok())` — an RFC3339 string fails that read and the row is
    /// silently dropped, so every pi agent message vanished on refresh.
    #[test]
    fn agent_created_at_reads_as_i64_so_rows_are_not_dropped() {
        let conn = Connection::open_in_memory().unwrap();
        // Minimal agent_messages with the real INTEGER created_at affinity.
        conn.execute_batch(
            "CREATE TABLE agent_messages (
                id         TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role       TEXT NOT NULL,
                content    TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                reasoning  TEXT,
                input_tokens INTEGER, output_tokens INTEGER, cost_usd REAL, duration_ms INTEGER
            );",
        )
        .unwrap();

        persist_agent_text_message(&conn, "m1", "s1", "assistant", "hello", None, &TurnUsage::default())
            .unwrap();

        // The exact read the reader performs — must not error.
        let (ts, content): (i64, String) = conn
            .query_row(
                "SELECT created_at, content FROM agent_messages WHERE id = 'm1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("created_at must read as i64 (else the row vanishes on refresh)");
        assert!(ts > 1_700_000_000_000, "expected epoch millis, got {ts}");

        // Stored affinity is integer, matching legacy rows + the INTEGER column.
        let ty: String = conn
            .query_row("SELECT typeof(created_at) FROM agent_messages WHERE id='m1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(ty, "integer", "created_at must be stored as integer, not text");

        // content is still the ContentBlock JSON the Agent renderer parses.
        let blocks: Option<Vec<ContentBlock>> = serde_json::from_str(&content).unwrap();
        assert_eq!(blocks.expect("Some(blocks)").len(), 1);
    }

    /// Role decides the content shape: user → plain text (the user bubble renders
    /// `message.content` raw, so JSON would show literally), assistant → JSON
    /// ContentBlocks (parsed by NativeBlockRenderer).
    #[test]
    fn agent_user_is_plain_text_assistant_is_json() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE agent_messages (
                id TEXT PRIMARY KEY, session_id TEXT NOT NULL, role TEXT NOT NULL,
                content TEXT NOT NULL, created_at INTEGER NOT NULL, reasoning TEXT,
                input_tokens INTEGER, output_tokens INTEGER, cost_usd REAL, duration_ms INTEGER
            );",
        )
        .unwrap();

        persist_agent_text_message(&conn, "u1", "s1", "user", "ls -a", None, &TurnUsage::default())
            .unwrap();
        persist_agent_text_message(&conn, "a1", "s1", "assistant", "done", None, &TurnUsage::default())
            .unwrap();

        let user_content: String = conn
            .query_row("SELECT content FROM agent_messages WHERE id='u1'", [], |r| r.get(0))
            .unwrap();
        // Plain text — NOT a JSON array (a JSON array renders literally in the bubble).
        assert_eq!(user_content, "ls -a");

        let asst_content: String = conn
            .query_row("SELECT content FROM agent_messages WHERE id='a1'", [], |r| r.get(0))
            .unwrap();
        let blocks: Option<Vec<ContentBlock>> = serde_json::from_str(&asst_content).unwrap();
        match &blocks.expect("Some(blocks)")[0] {
            ContentBlock::Text { text } => assert_eq!(text, "done"),
            other => panic!("expected Text block, got {other:?}"),
        }
    }

    /// End-to-end of the metadata badge persist: an `agent:turn_cost` payload →
    /// TurnUsage → the four columns `get_agent_session_messages` reads into
    /// `usage` + `durationMs`. (The badge: ⚡ 耗时 · N 输入 · M 输出 · $费用.)
    #[test]
    fn agent_assistant_persists_turn_cost_columns() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE agent_messages (
                id TEXT PRIMARY KEY, session_id TEXT NOT NULL, role TEXT NOT NULL,
                content TEXT NOT NULL, created_at INTEGER NOT NULL, reasoning TEXT,
                input_tokens INTEGER, output_tokens INTEGER, cost_usd REAL, duration_ms INTEGER
            );",
        )
        .unwrap();

        // Parse the cached agent:turn_cost payload exactly as engine_sink does.
        let usage = TurnUsage::from_turn_cost(&serde_json::json!({
            "conversationId": "s1",
            "inputTokens": 19119,
            "outputTokens": 65,
            "costUsd": "$0.0027",
            "durationMs": 1200,
        }));
        persist_agent_text_message(&conn, "a1", "s1", "assistant", "hi", None, &usage).unwrap();

        let (it, ot, cost, dur): (Option<i64>, Option<i64>, Option<f64>, Option<i64>) = conn
            .query_row(
                "SELECT input_tokens, output_tokens, cost_usd, duration_ms \
                 FROM agent_messages WHERE id = 'a1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(it, Some(19119));
        assert_eq!(ot, Some(65));
        assert!((cost.unwrap() - 0.0027).abs() < 1e-9, "cost {cost:?}");
        assert_eq!(dur, Some(1200));
    }

    /// Zero token counts collapse to None (no "0 输入" badge), but duration stays.
    #[test]
    fn turn_usage_drops_zero_tokens_keeps_duration() {
        let u = TurnUsage::from_turn_cost(&serde_json::json!({
            "inputTokens": 0, "outputTokens": 0, "costUsd": "$0.0000", "durationMs": 50,
        }));
        assert_eq!(u.input_tokens, None);
        assert_eq!(u.output_tokens, None);
        assert_eq!(u.duration_ms, Some(50));
    }
}
