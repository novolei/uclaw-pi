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
}
