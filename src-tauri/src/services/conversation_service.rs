//! Conversation-domain SQL as a Tauri-independent service — the business logic
//! behind the `commands::conversation` thin commands.
//!
//! Operates on a borrowed `&Connection` (never `AppState`/`State`) so it
//! unit-tests without Tauri. Extracted from the legacy `tauri_commands.rs` god
//! file per the code-organization ADR (2026-05-31). Only the methods with inline
//! SQL live here: `list_recent_threads` (chat + agent UNION over
//! `conversations`/`agent_sessions`), `get_messages` (read + ContentBlock
//! reprojection over `messages`), and `toggle_star` (read-modify-write on
//! `conversations`). The create/list/delete-by-`session_manager` commands have
//! no SQL to lift and stay thin in `commands::conversation`.

use rusqlite::Connection;

use crate::agent::types::ContentBlock;
use crate::error::Error;
use crate::ipc::{MessageResponse, RecentThread};

/// SQL-backed conversation queries (the `conversations` / `agent_sessions` /
/// `messages` tables).
pub trait ConversationService {
    /// Merge recent chat conversations and agent sessions into a single
    /// updated_at-DESC list, capped at 20. Chat rows come from `conversations`
    /// (joined to `spaces` for the workspace name); agent rows from
    /// `agent_sessions`. Returns the cross-domain browse-palette summary.
    fn list_recent_threads(&self, conn: &Connection) -> Result<Vec<RecentThread>, Error>;

    /// Read all messages for a conversation from `messages` (the source of
    /// truth), reprojecting the persisted `content` blob into both a flat text
    /// projection and the original ordered `ContentBlock`s.
    fn get_messages(
        &self,
        conn: &Connection,
        conversation_id: &str,
    ) -> Result<Vec<MessageResponse>, Error>;

    /// Flip a conversation's `starred` flag (read-modify-write). Returns the new
    /// starred state.
    fn toggle_star(&self, conn: &Connection, conversation_id: &str) -> Result<bool, Error>;
}

/// The SQLite-backed implementation (the only one in production).
pub struct DbConversation;

impl ConversationService for DbConversation {
    fn list_recent_threads(&self, conn: &Connection) -> Result<Vec<RecentThread>, Error> {
        let mut out: Vec<RecentThread> = Vec::new();

        // Chat conversations — JOIN spaces for workspace name
        let mut stmt = conn
            .prepare(
                "SELECT
                    c.id, c.title, c.metadata_json,
                    COALESCE(s.name, 'default') AS workspace_name,
                    COALESCE(s.id, 'default') AS workspace_id,
                    (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) AS msg_count,
                    c.updated_at
                 FROM conversations c
                 LEFT JOIN spaces s ON s.id = c.space_id
                 WHERE COALESCE(c.is_agent, 0) = 0
                 ORDER BY c.updated_at DESC
                 LIMIT 20",
            )
            .map_err(|e| Error::Internal(format!("prepare chat list: {}", e)))?;
        let rows = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let title: Option<String> = row.get(1)?;
                let metadata_json: Option<String> = row.get(2)?;
                let workspace_name: String = row.get(3)?;
                let workspace_id: String = row.get(4)?;
                let msg_count: i64 = row.get(5)?;
                let updated_at: String = row.get(6)?;
                Ok((id, title, metadata_json, workspace_name, workspace_id, msg_count, updated_at))
            })
            .map_err(|e| Error::Internal(format!("query chat list: {}", e)))?;
        for r in rows.flatten() {
            let (id, title, metadata_json, ws_name, ws_id, msg_count, updated_at) = r;
            let (emoji, pending) = parse_title_metadata(metadata_json.as_deref());
            out.push(RecentThread {
                id,
                kind: "chat".into(),
                title: title.unwrap_or_else(|| "(untitled)".into()),
                title_emoji: emoji,
                title_pending: pending,
                workspace_name: ws_name,
                workspace_id: ws_id,
                message_count: msg_count.max(0) as u32,
                updated_at,
            });
        }
        drop(stmt);

        // Agent sessions — title_emoji/title_pending columns don't exist on this
        // schema (V8 migration not present); use NULL placeholders so the query
        // succeeds without a migration.
        let mut stmt = conn
            .prepare(
                "SELECT
                    s.id, s.title,
                    NULL AS title_emoji, NULL AS title_pending,
                    COALESCE(sp.name, 'default') AS workspace_name,
                    COALESCE(sp.id, 'default') AS workspace_id,
                    s.message_count,
                    s.updated_at
                 FROM agent_sessions s
                 LEFT JOIN spaces sp ON sp.id = s.space_id
                 ORDER BY s.updated_at DESC
                 LIMIT 20",
            )
            .map_err(|e| Error::Internal(format!("prepare agent list: {}", e)))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?.unwrap_or_else(|| "(untitled)".into()),
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<i64>>(3)?.map(|v| v != 0),
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            })
            .map_err(|e| Error::Internal(format!("query agent list: {}", e)))?;
        for r in rows.flatten() {
            let (id, title, emoji, pending, ws_name, ws_id, msg_count, updated_at) = r;
            let updated_at_rfc = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(updated_at)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default();
            out.push(RecentThread {
                id,
                kind: "agent".into(),
                title,
                title_emoji: emoji,
                title_pending: pending,
                workspace_name: ws_name,
                workspace_id: ws_id,
                message_count: msg_count.max(0) as u32,
                updated_at: updated_at_rfc,
            });
        }
        drop(stmt);

        // Sort merged list by updated_at DESC, cap at 20.
        // Both sides emit RFC3339 now, but normalize defensively to epoch-ms.
        out.sort_by(|a, b| to_epoch_ms(&b.updated_at).cmp(&to_epoch_ms(&a.updated_at)));
        out.truncate(20);
        Ok(out)
    }

    fn get_messages(
        &self,
        conn: &Connection,
        conversation_id: &str,
    ) -> Result<Vec<MessageResponse>, Error> {
        // Always read from SQLite as the source of truth so messages survive
        // across app restarts and include reasoning + tool activities.
        let mut stmt = conn
            .prepare(
                "SELECT id, role, content, reasoning, tool_activities_json, model, created_at \
                 FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC",
            )
            .map_err(|e| Error::Internal(format!("prepare get_messages: {}", e)))?;

        let rows = stmt
            .query_map(rusqlite::params![conversation_id], |row| {
                let id: String = row.get(0)?;
                let role: String = row.get(1)?;
                let raw_content: String = row.get(2)?;
                let reasoning: Option<String> = row.get(3)?;
                let tool_activities_json: Option<String> = row.get(4)?;
                let model: Option<String> = row.get(5)?;
                let created_at: String = row.get(6)?;
                Ok((id, role, raw_content, reasoning, tool_activities_json, model, created_at))
            })
            .map_err(|e| Error::Internal(format!("query get_messages: {}", e)))?;

        let mut out: Vec<MessageResponse> = Vec::new();
        for row in rows.flatten() {
            let (id, role, raw_content, reasoning, tool_activities_json, model, created_at) = row;

            // Parse `content` once. Two persisted shapes have been seen historically:
            //   - JSON of Option<Vec<ContentBlock>> — written by add_message_with_meta
            //     via serde_json::to_string(&session.messages.last().map(|m| &m.content))
            //   - JSON of Vec<ContentBlock> — written by older code paths
            //   - Plain text — pre-V5 rows
            let parsed_blocks: Option<Vec<ContentBlock>> =
                serde_json::from_str::<Option<Vec<ContentBlock>>>(&raw_content)
                    .ok()
                    .flatten()
                    .or_else(|| serde_json::from_str::<Vec<ContentBlock>>(&raw_content).ok());

            // Flat text projection — joins all Text blocks. Used by the legacy
            // renderer + minimap snippets.
            let content_text: String = parsed_blocks
                .as_ref()
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|b| {
                            if let ContentBlock::Text { text } = b {
                                Some(text.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or(raw_content);

            let tool_activities = tool_activities_json
                .as_deref()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

            out.push(MessageResponse {
                id,
                conversation_id: conversation_id.to_string(),
                role,
                content: content_text,
                created_at,
                reasoning,
                tool_activities,
                model,
                content_blocks: parsed_blocks,
            });
        }
        Ok(out)
    }

    fn toggle_star(&self, conn: &Connection, conversation_id: &str) -> Result<bool, Error> {
        let current: bool = conn
            .query_row(
                "SELECT COALESCE(starred, 0) FROM conversations WHERE id = ?1",
                rusqlite::params![conversation_id],
                |row| row.get::<_, i32>(0),
            )
            .unwrap_or(0)
            != 0;

        let new_starred = !current;
        conn.execute(
            "UPDATE conversations SET starred = ?1 WHERE id = ?2",
            rusqlite::params![new_starred as i32, conversation_id],
        )
        .map_err(Error::Database)?;

        Ok(new_starred)
    }
}

/// Parse an `updated_at` string into epoch milliseconds. Accepts a bare i64-ms
/// integer string (legacy agent format) or an RFC3339 timestamp; returns 0 on
/// parse failure so unknown formats sort to the bottom rather than crashing.
fn to_epoch_ms(s: &str) -> i64 {
    if let Ok(n) = s.parse::<i64>() {
        return n;
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return dt.timestamp_millis();
    }
    0
}

/// Parse the conversation `metadata_json` blob for `emoji` and `title_pending`.
/// The blob looks like `{"title":"…","emoji":"🎨","title_pending":false}`.
fn parse_title_metadata(meta: Option<&str>) -> (Option<String>, Option<bool>) {
    let Some(raw) = meta else {
        return (None, None);
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw) else {
        return (None, None);
    };
    let emoji = parsed.get("emoji").and_then(|v| v.as_str()).map(|s| s.to_string());
    let pending = parsed.get("title_pending").and_then(|v| v.as_bool());
    (emoji, pending)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory DB with just the columns the service touches across the three
    /// tables (`conversations`, `agent_sessions`, `messages`, `spaces`).
    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE spaces (
                id TEXT PRIMARY KEY,
                name TEXT,
                icon TEXT
            );
            CREATE TABLE conversations (
                id TEXT PRIMARY KEY,
                title TEXT,
                metadata_json TEXT,
                space_id TEXT,
                is_agent INTEGER DEFAULT 0,
                starred INTEGER DEFAULT 0,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE agent_sessions (
                id TEXT PRIMARY KEY,
                title TEXT,
                space_id TEXT,
                message_count INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                reasoning TEXT,
                tool_activities_json TEXT,
                model TEXT,
                created_at TEXT NOT NULL
            );",
        )
        .unwrap();
        c
    }

    #[test]
    fn list_recent_threads_merges_chat_and_agent_sorted_desc() {
        let c = conn();
        c.execute(
            "INSERT INTO spaces (id, name, icon) VALUES ('ws1', 'Project', '📁')",
            [],
        )
        .unwrap();
        // Chat conversation (RFC3339 updated_at, older).
        c.execute(
            "INSERT INTO conversations (id, title, metadata_json, space_id, is_agent, starred, updated_at)
             VALUES ('chat1', 'Hello', '{\"emoji\":\"🎨\",\"title_pending\":true}', 'ws1', 0, 0, '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        // Two messages so the COUNT subquery returns 2.
        c.execute_batch(
            "INSERT INTO messages (id, conversation_id, role, content, created_at)
                VALUES ('m1', 'chat1', 'user', 'hi', 't1');
             INSERT INTO messages (id, conversation_id, role, content, created_at)
                VALUES ('m2', 'chat1', 'assistant', 'yo', 't2');",
        )
        .unwrap();
        // Agent session (epoch-ms updated_at, newer → sorts first).
        let newer_ms = chrono::Utc::now().timestamp_millis();
        c.execute(
            "INSERT INTO agent_sessions (id, title, space_id, message_count, updated_at)
             VALUES ('agent1', 'Build', 'ws1', 5, ?1)",
            rusqlite::params![newer_ms],
        )
        .unwrap();

        let threads = DbConversation.list_recent_threads(&c).unwrap();
        assert_eq!(threads.len(), 2);
        // Agent (newer) sorts first.
        assert_eq!(threads[0].kind, "agent");
        assert_eq!(threads[0].id, "agent1");
        assert_eq!(threads[0].message_count, 5);
        assert_eq!(threads[0].workspace_name, "Project");
        // Chat second, with metadata-derived emoji + pending flag + COUNT.
        assert_eq!(threads[1].kind, "chat");
        assert_eq!(threads[1].title, "Hello");
        assert_eq!(threads[1].title_emoji.as_deref(), Some("🎨"));
        assert_eq!(threads[1].title_pending, Some(true));
        assert_eq!(threads[1].message_count, 2);
    }

    #[test]
    fn get_messages_reprojects_content_blocks_and_flat_text() {
        let c = conn();
        // Persisted as JSON Vec<ContentBlock> with a Text block.
        let blocks_json = r#"[{"type":"text","text":"hello world"}]"#;
        c.execute(
            "INSERT INTO messages (id, conversation_id, role, content, reasoning, tool_activities_json, model, created_at)
             VALUES ('m1', 'conv1', 'assistant', ?1, 'because', NULL, 'deepseek-v4', 't1')",
            rusqlite::params![blocks_json],
        )
        .unwrap();
        // A plain-text (pre-V5) row falls through to the raw_content branch.
        c.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at)
             VALUES ('m2', 'conv1', 'user', 'just text', 't2')",
            [],
        )
        .unwrap();

        let msgs = DbConversation.get_messages(&c, "conv1").unwrap();
        assert_eq!(msgs.len(), 2);
        // First: blocks parsed, flat text projected from the Text block.
        assert_eq!(msgs[0].content, "hello world");
        assert_eq!(msgs[0].reasoning.as_deref(), Some("because"));
        assert_eq!(msgs[0].model.as_deref(), Some("deepseek-v4"));
        assert!(msgs[0].content_blocks.is_some());
        assert_eq!(msgs[0].conversation_id, "conv1");
        // Second: plain text falls through unchanged; no blocks.
        assert_eq!(msgs[1].content, "just text");
        assert!(msgs[1].content_blocks.is_none());
    }

    #[test]
    fn toggle_star_flips_and_persists() {
        let c = conn();
        c.execute(
            "INSERT INTO conversations (id, title, space_id, starred, updated_at)
             VALUES ('conv1', 'T', 'ws1', 0, 't')",
            [],
        )
        .unwrap();

        // 0 → true, persisted.
        assert!(DbConversation.toggle_star(&c, "conv1").unwrap());
        let stored: i32 = c
            .query_row("SELECT starred FROM conversations WHERE id = 'conv1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored, 1);

        // true → false.
        assert!(!DbConversation.toggle_star(&c, "conv1").unwrap());

        // Unknown id: COALESCE(starred,0) defaults to 0 → toggles to true (UPDATE
        // affects no rows but the returned new-state is computed from current=false).
        assert!(DbConversation.toggle_star(&c, "ghost").unwrap());
    }
}
