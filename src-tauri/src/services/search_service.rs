//! Search-domain SQL as a Tauri-independent service — the business logic behind
//! the `commands::search` thin commands.
//!
//! Operates on a borrowed `&Connection` (never `AppState`/`State`) so it
//! unit-tests without Tauri. Extracted from the legacy `tauri_commands.rs` god
//! file per the code-organization ADR (2026-05-31). Only the SQL-bearing part of
//! `search_conversations` lives here — the filesystem walks (`search_workspace`,
//! `search_all`'s file pass) stay in `commands::search` because they're
//! `tokio::fs` over `state.data_dir`, not DB logic.
//!
//! ## Shape: flat UNION-of-branches (preserved per CLAUDE.md)
//!
//! [`SearchService::search_conversations`] runs a fixed *sequence* of independent
//! query branches, each pushing rows into one shared `results: Vec<SearchResult>`:
//!   1. title `LIKE` over `conversations` (global, non-session only)
//!   2. chat-message FTS over `messages_fts`
//!   3. agent-turn FTS over `agent_turns_fts`
//!   4. agent-message FTS over `agent_messages_fts`
//!   5. substring `LIKE` fallback over `agent_messages` + `messages.content_text`
//!      (covers CJK ≤2-char queries the trigram tokenizer can't MATCH)
//! then truncates to 50. New result sources are added as **another branch in this
//! same file**, never via a generic dispatcher/abstraction layer.

use std::collections::HashSet;

use rusqlite::Connection;

use crate::error::Error;
use crate::ipc::SearchResult;

/// SQL-backed cross-domain search over the chat (`messages*`) and agent
/// (`agent_turns*` / `agent_messages*`) tables. The filesystem search lives in
/// `commands::search`; only the DB branches are here.
pub trait SearchService {
    /// The flat UNION-of-branches conversation/message search. `query` is the
    /// raw user text; `scope` is the optional `SearchInput.scope` field
    /// (`"session:<id>"` narrows the FTS branches to one session). Returns up to
    /// 50 hits, FTS branches first (already score-ordered within each batch),
    /// then LIKE-fallback hits, in branch order.
    fn search_conversations(
        &self,
        conn: &Connection,
        query: &str,
        scope: Option<&str>,
    ) -> Result<Vec<SearchResult>, Error>;
}

/// The SQLite-backed implementation (the only one in production).
pub struct DbSearch;

impl SearchService for DbSearch {
    fn search_conversations(
        &self,
        conn: &Connection,
        query: &str,
        scope: Option<&str>,
    ) -> Result<Vec<SearchResult>, Error> {
        let fts_query = build_fts_query(query);
        let session_filter = parse_scope(scope);

        let mut results: Vec<SearchResult> = Vec::new();

        // 1. Title hits — global only (titles aren't per-session).
        if session_filter.is_none() && !query.trim().is_empty() {
            let mut stmt = conn.prepare(
                "SELECT c.id, c.title, c.is_agent, c.updated_at, c.workspace_id
                 FROM conversations c
                 WHERE LOWER(c.title) LIKE LOWER(?1)
                 ORDER BY c.updated_at DESC
                 LIMIT 10",
            ).map_err(|e| Error::Internal(format!("prepare title query: {}", e)))?;
            let like_pattern = format!("%{}%", query.trim());
            let title_rows = stmt.query_map(rusqlite::params![like_pattern], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            }).map_err(|e| Error::Internal(format!("title query: {}", e)))?;
            for r in title_rows.flatten() {
                let (id, title, is_agent, updated_at, workspace_id) = r;
                let snippet = if is_agent != 0 { "Agent session" } else { "Chat" };
                results.push(SearchResult {
                    id: format!("title:{}", id),
                    title,
                    snippet: snippet.into(),
                    source: "conversation".into(),
                    source_id: id,
                    message_id: None,
                    workspace_id,
                    created_at: updated_at,
                });
            }
        }

        // 2. Chat message FTS — only if we have an FTS expression.
        if let Some(ref fq) = fts_query {
            let (sql, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match &session_filter {
                Some(sid) => (
                    "SELECT m.id, m.conversation_id, COALESCE(c.title, '') AS title,
                            snippet(messages_fts, 2, '<b>', '</b>', '...', 16) AS snip,
                            m.created_at, c.workspace_id, bm25(messages_fts) AS score
                     FROM messages_fts f
                     JOIN messages m ON m.rowid = f.rowid
                     LEFT JOIN conversations c ON c.id = m.conversation_id
                     WHERE messages_fts MATCH ?1 AND m.conversation_id = ?2
                     ORDER BY score LIMIT 30",
                    vec![Box::new(fq.clone()), Box::new(sid.clone())],
                ),
                None => (
                    "SELECT m.id, m.conversation_id, COALESCE(c.title, '') AS title,
                            snippet(messages_fts, 2, '<b>', '</b>', '...', 16) AS snip,
                            m.created_at, c.workspace_id, bm25(messages_fts) AS score
                     FROM messages_fts f
                     JOIN messages m ON m.rowid = f.rowid
                     LEFT JOIN conversations c ON c.id = m.conversation_id
                     WHERE messages_fts MATCH ?1
                     ORDER BY score LIMIT 30",
                    vec![Box::new(fq.clone())],
                ),
            };
            let mut stmt = conn.prepare(sql)
                .map_err(|e| Error::Internal(format!("prepare chat fts: {}", e)))?;
            let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| &**b as &dyn rusqlite::ToSql).collect();
            let chat_rows = stmt.query_map(rusqlite::params_from_iter(param_refs), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            }).map_err(|e| Error::Internal(format!("chat fts query: {}", e)))?;
            for r in chat_rows.flatten() {
                let (msg_id, conv_id, title, snip, created_at, workspace_id) = r;
                results.push(SearchResult {
                    id: format!("chat:{}", msg_id),
                    title,
                    snippet: snip,
                    source: "chat_message".into(),
                    source_id: conv_id,
                    message_id: Some(msg_id),
                    workspace_id,
                    created_at,
                });
            }
        }

        // 3. Agent turn FTS — same pattern.
        if let Some(ref fq) = fts_query {
            let (sql, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match &session_filter {
                Some(sid) => (
                    "SELECT at.id, at.session_id, COALESCE(s.title, '') AS title,
                            snippet(agent_turns_fts, 1, '<b>', '</b>', '...', 16) AS snip_content,
                            snippet(agent_turns_fts, 2, '<b>', '</b>', '...', 16) AS snip_tool,
                            snippet(agent_turns_fts, 3, '<b>', '</b>', '...', 16) AS snip_reasoning,
                            at.created_at, s.space_id, bm25(agent_turns_fts) AS score
                     FROM agent_turns_fts f
                     JOIN agent_turns at ON at.rowid = f.rowid
                     LEFT JOIN agent_sessions s ON s.id = at.session_id
                     WHERE agent_turns_fts MATCH ?1 AND at.session_id = ?2
                     ORDER BY score LIMIT 30",
                    vec![Box::new(fq.clone()), Box::new(sid.clone())],
                ),
                None => (
                    "SELECT at.id, at.session_id, COALESCE(s.title, '') AS title,
                            snippet(agent_turns_fts, 1, '<b>', '</b>', '...', 16) AS snip_content,
                            snippet(agent_turns_fts, 2, '<b>', '</b>', '...', 16) AS snip_tool,
                            snippet(agent_turns_fts, 3, '<b>', '</b>', '...', 16) AS snip_reasoning,
                            at.created_at, s.space_id, bm25(agent_turns_fts) AS score
                     FROM agent_turns_fts f
                     JOIN agent_turns at ON at.rowid = f.rowid
                     LEFT JOIN agent_sessions s ON s.id = at.session_id
                     WHERE agent_turns_fts MATCH ?1
                     ORDER BY score LIMIT 30",
                    vec![Box::new(fq.clone())],
                ),
            };
            let mut stmt = conn.prepare(sql)
                .map_err(|e| Error::Internal(format!("prepare agent fts: {}", e)))?;
            let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| &**b as &dyn rusqlite::ToSql).collect();
            let agent_rows = stmt.query_map(rusqlite::params_from_iter(param_refs), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            }).map_err(|e| Error::Internal(format!("agent fts query: {}", e)))?;
            for r in agent_rows.flatten() {
                let (turn_id, sess_id, title, snip_c, snip_t, snip_r, created_at, workspace_id) = r;
                let snippet = [&snip_c, &snip_t, &snip_r]
                    .iter()
                    .find(|s| !s.is_empty() && **s != "...")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "(no preview)".into());
                results.push(SearchResult {
                    id: format!("agent_turn:{}", turn_id),
                    title,
                    snippet,
                    source: "agent_turn".into(),
                    source_id: sess_id,
                    message_id: None,
                    workspace_id,
                    created_at: created_at.to_string(),
                });
            }
        }

        // 4. Agent message FTS hits (agent_messages_fts.{content, reasoning}).
        //    This is the user/assistant conversation in the agent domain — historically
        //    unindexed, which made user prompts and assistant replies invisible to
        //    search. agent_turns above only covers tool-call rows.
        let mut stmt = conn.prepare(
            "SELECT
                 am.id,
                 am.session_id,
                 COALESCE(s.title, '') AS title,
                 am.role,
                 snippet(agent_messages_fts, 2, '<b>', '</b>', '...', 16) AS snip_content,
                 snippet(agent_messages_fts, 3, '<b>', '</b>', '...', 16) AS snip_reasoning,
                 am.created_at,
                 s.space_id,
                 bm25(agent_messages_fts) AS score
             FROM agent_messages_fts f
             JOIN agent_messages am ON am.rowid = f.rowid
             LEFT JOIN agent_sessions s ON s.id = am.session_id
             WHERE agent_messages_fts MATCH ?1
             ORDER BY score
             LIMIT 30",
        ).map_err(|e| Error::Internal(format!("prepare agent_messages fts: {}", e)))?;
        let agent_msg_rows = stmt.query_map(rusqlite::params![&fts_query], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        }).map_err(|e| Error::Internal(format!("agent_messages fts query: {}", e)))?;
        for r in agent_msg_rows.flatten() {
            let (msg_id, sess_id, title, _role, snip_c, snip_r, created_at, workspace_id) = r;
            let snippet = [&snip_c, &snip_r]
                .iter()
                .find(|s| !s.is_empty() && **s != "...")
                .map(|s| s.to_string())
                .unwrap_or_else(|| "(no preview)".into());
            results.push(SearchResult {
                id: format!("agent_msg:{}", msg_id),
                title,
                snippet,
                source: "agent_message".into(),
                source_id: sess_id,
                message_id: Some(msg_id),
                workspace_id,
                created_at: created_at.to_string(),
            });
        }
        drop(stmt);

        // 5. Substring LIKE fallback over agent_messages.content + messages.content_text.
        //    Trigram FTS requires queries of ≥3 codepoints; CJK 2-char queries
        //    (e.g. "几点", "时间") return 0 from MATCH. LIKE handles those, plus
        //    English short prefixes. Bounded scan — fine for desktop SQLite at the
        //    sizes these tables reach.
        let q_trimmed = query.trim();
        if !q_trimmed.is_empty() {
            let like_pattern = format!("%{}%", q_trimmed);

            // Track what FTS already surfaced so we don't double-render the same
            // message id in the palette.
            let already_seen: HashSet<String> = results.iter()
                .filter_map(|r| r.message_id.as_ref().map(|m| format!("{}:{}", r.source, m)))
                .collect();

            // Agent messages
            let mut stmt = conn.prepare(
                "SELECT am.id, am.session_id, COALESCE(s.title, '') AS title,
                        am.content, am.created_at, s.space_id
                 FROM agent_messages am
                 LEFT JOIN agent_sessions s ON s.id = am.session_id
                 WHERE am.content LIKE ?1 COLLATE NOCASE
                 ORDER BY am.created_at DESC
                 LIMIT 20"
            ).map_err(|e| Error::Internal(format!("prepare agent_messages like: {}", e)))?;
            let rows = stmt.query_map(rusqlite::params![&like_pattern], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            }).map_err(|e| Error::Internal(format!("agent_messages like query: {}", e)))?;
            for r in rows.flatten() {
                let (msg_id, sess_id, title, content, created_at, workspace_id) = r;
                if already_seen.contains(&format!("agent_message:{}", msg_id)) { continue; }
                // Build a windowed snippet around the first hit, mimicking FTS snippet().
                let snippet = build_substring_snippet(&content, q_trimmed, 24);
                results.push(SearchResult {
                    id: format!("agent_msg:{}", msg_id),
                    title,
                    snippet,
                    source: "agent_message".into(),
                    source_id: sess_id,
                    message_id: Some(msg_id),
                    workspace_id,
                    created_at: created_at.to_string(),
                });
            }
            drop(stmt);

            // Chat messages — use content_text (V10 generated column).
            let mut stmt = conn.prepare(
                "SELECT m.id, m.conversation_id, COALESCE(c.title, '') AS title,
                        m.content_text, m.created_at, c.workspace_id
                 FROM messages m
                 LEFT JOIN conversations c ON c.id = m.conversation_id
                 WHERE m.content_text LIKE ?1 COLLATE NOCASE
                 ORDER BY m.created_at DESC
                 LIMIT 20"
            ).map_err(|e| Error::Internal(format!("prepare messages like: {}", e)))?;
            let rows = stmt.query_map(rusqlite::params![&like_pattern], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            }).map_err(|e| Error::Internal(format!("messages like query: {}", e)))?;
            for r in rows.flatten() {
                let (msg_id, conv_id, title, content_text, created_at, workspace_id) = r;
                if already_seen.contains(&format!("chat_message:{}", msg_id)) { continue; }
                let snippet = build_substring_snippet(&content_text, q_trimmed, 24);
                results.push(SearchResult {
                    id: format!("chat:{}", msg_id),
                    title,
                    snippet,
                    source: "chat_message".into(),
                    source_id: conv_id,
                    message_id: Some(msg_id),
                    workspace_id,
                    created_at,
                });
            }
            drop(stmt);
        }

        // Cap total results, prefer high-score hits already at the top of each batch
        results.truncate(50);
        Ok(results)
    }
}

/// Build an FTS5 MATCH expression from raw user input.
///
/// Splits on Unicode whitespace, escapes any double-quotes inside each
/// token, wraps each token as a phrase (`"…"`), and space-joins them so
/// FTS5 reads the result as implicit AND of substring matches (under the
/// trigram tokenizer added in V11).
///
/// Returns `None` for empty / whitespace-only input — the caller should
/// then skip the FTS branches and only do title LIKE.
fn build_fts_query(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parts: Vec<String> = trimmed
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(" "))
}

/// Parse the optional `scope` field on `SearchInput` into a typed value.
/// Format: `"session:<id>"` for a session-scoped search, anything else
/// (or `None`) → unscoped global search.
fn parse_scope(scope: Option<&str>) -> Option<String> {
    let raw = scope?;
    raw.strip_prefix("session:").map(|id| id.to_string())
}

/// Build a short snippet around the first case-insensitive occurrence of
/// `needle` in `text`, with `<b>` markers around the match. Mimics the
/// shape FTS5's snippet() returns so the frontend can render uniformly.
fn build_substring_snippet(text: &str, needle: &str, window: usize) -> String {
    let lower = text.to_lowercase();
    let lneedle = needle.to_lowercase();
    let Some(byte_idx) = lower.find(&lneedle) else {
        return text.chars().take(window * 2).collect::<String>();
    };
    // Convert byte_idx → char index for safe slicing on the original text.
    let char_idx = lower[..byte_idx].chars().count();
    let needle_chars = needle.chars().count();
    let start = char_idx.saturating_sub(window);
    let end = (char_idx + needle_chars + window).min(text.chars().count());
    let prefix = if start > 0 { "..." } else { "" };
    let suffix = if end < text.chars().count() { "..." } else { "" };
    let pre: String = text.chars().take(char_idx).skip(start).collect();
    let mid: String = text.chars().skip(char_idx).take(needle_chars).collect();
    let post: String = text.chars().skip(char_idx + needle_chars).take(end - char_idx - needle_chars).collect();
    format!("{}{}<b>{}</b>{}{}", prefix, pre, mid, post, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Pure-helper tests (ported verbatim from the legacy
    //     `tauri_commands::fts_query_tests`) ──────────────────────────────────

    #[test]
    fn empty_input_returns_none() {
        assert_eq!(build_fts_query(""), None);
        assert_eq!(build_fts_query("   "), None);
        assert_eq!(build_fts_query("\t\n"), None);
    }

    #[test]
    fn single_word() {
        assert_eq!(build_fts_query("gomoku").unwrap(), "\"gomoku\"");
    }

    #[test]
    fn multi_word_implicit_and() {
        assert_eq!(
            build_fts_query("gomoku rules").unwrap(),
            "\"gomoku\" \"rules\""
        );
    }

    #[test]
    fn cjk_token_preserved_as_phrase() {
        // Trigram tokenizer will further split this server-side;
        // build_fts_query just wraps the user's runs as phrases.
        assert_eq!(build_fts_query("五子棋").unwrap(), "\"五子棋\"");
    }

    #[test]
    fn mixed_cjk_and_ascii() {
        assert_eq!(
            build_fts_query("五子棋 rules").unwrap(),
            "\"五子棋\" \"rules\""
        );
    }

    #[test]
    fn embedded_double_quotes_are_doubled() {
        // FTS5 phrase escape: `"` → `""` inside a quoted phrase.
        assert_eq!(
            build_fts_query("a\"b c").unwrap(),
            "\"a\"\"b\" \"c\""
        );
    }

    #[test]
    fn whitespace_collapsed() {
        assert_eq!(
            build_fts_query("  foo   bar  ").unwrap(),
            "\"foo\" \"bar\""
        );
    }

    #[test]
    fn scope_session_parses() {
        assert_eq!(
            parse_scope(Some("session:abc-123")),
            Some("abc-123".to_string())
        );
    }

    #[test]
    fn scope_unknown_returns_none() {
        assert_eq!(parse_scope(Some("workspace:foo")), None);
        assert_eq!(parse_scope(Some("")), None);
        assert_eq!(parse_scope(None), None);
    }

    #[test]
    fn substring_snippet_marks_first_hit() {
        // ASCII: window clamps both sides, `<b>` wraps the needle.
        let snip = build_substring_snippet("the quick brown fox", "brown", 4);
        assert_eq!(snip, "...ick <b>brown</b> fox");
        // CJK is char-indexed, not byte-indexed (no panic on multibyte).
        let snip = build_substring_snippet("今天几点了", "几点", 1);
        assert_eq!(snip, "...天<b>几点</b>了");
        // Needle absent → leading window of chars, no markers.
        let snip = build_substring_snippet("hello world", "zzz", 3);
        assert_eq!(snip, "hello ");
    }

    // ─── End-to-end branch coverage with an in-memory DB ─────────────────────

    /// In-memory DB with the (real-schema) tables + FTS5 virtual tables the
    /// service touches. Trigram tokenizer mirrors V11 so `MATCH` behaves like prod.
    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE conversations (
                id TEXT PRIMARY KEY,
                title TEXT,
                is_agent INTEGER DEFAULT 0,
                workspace_id TEXT,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                content_text TEXT,
                created_at TEXT NOT NULL
            );
            CREATE TABLE agent_sessions (
                id TEXT PRIMARY KEY,
                title TEXT,
                space_id TEXT
            );
            CREATE TABLE agent_turns (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE agent_messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT,
                content TEXT,
                created_at INTEGER NOT NULL
            );
            CREATE VIRTUAL TABLE messages_fts USING fts5(
                id UNINDEXED, conversation_id UNINDEXED, content_text,
                tokenize='trigram'
            );
            CREATE VIRTUAL TABLE agent_turns_fts USING fts5(
                id UNINDEXED, content, tool, reasoning,
                tokenize='trigram'
            );
            CREATE VIRTUAL TABLE agent_messages_fts USING fts5(
                id UNINDEXED, role UNINDEXED, content, reasoning,
                tokenize='trigram'
            );",
        )
        .unwrap();
        c
    }

    #[test]
    fn search_title_and_chat_fts_branches() {
        let c = conn();
        // Title-hit branch (branch 1): LIKE over conversations.title.
        c.execute(
            "INSERT INTO conversations (id, title, is_agent, workspace_id, updated_at)
             VALUES ('conv1', 'Gomoku strategy', 0, 'ws1', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        // Chat-message FTS branch (branch 2): row in messages + its FTS shadow.
        c.execute(
            "INSERT INTO messages (id, conversation_id, content_text, created_at)
             VALUES ('m1', 'conv1', 'gomoku opening theory', 't1')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO messages_fts (rowid, id, conversation_id, content_text)
             VALUES ((SELECT rowid FROM messages WHERE id='m1'), 'm1', 'conv1', 'gomoku opening theory')",
            [],
        )
        .unwrap();

        let out = DbSearch.search_conversations(&c, "gomoku", None).unwrap();
        // Expect a title hit AND a chat_message hit (flat UNION pushes both).
        assert!(out.iter().any(|r| r.source == "conversation" && r.id == "title:conv1"));
        assert!(out.iter().any(|r| r.source == "chat_message" && r.message_id.as_deref() == Some("m1")));
    }

    #[test]
    fn search_agent_turn_and_message_fts_branches() {
        let c = conn();
        c.execute(
            "INSERT INTO agent_sessions (id, title, space_id) VALUES ('s1', 'Build', 'ws1')",
            [],
        )
        .unwrap();
        // Agent-turn FTS branch (branch 3).
        c.execute(
            "INSERT INTO agent_turns (id, session_id, created_at) VALUES ('t1', 's1', 100)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO agent_turns_fts (rowid, id, content, tool, reasoning)
             VALUES ((SELECT rowid FROM agent_turns WHERE id='t1'), 't1', 'running cargo build', '', '')",
            [],
        )
        .unwrap();
        // Agent-message FTS branch (branch 4).
        c.execute(
            "INSERT INTO agent_messages (id, session_id, role, content, created_at)
             VALUES ('am1', 's1', 'assistant', 'cargo finished compiling', 200)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO agent_messages_fts (rowid, id, role, content, reasoning)
             VALUES ((SELECT rowid FROM agent_messages WHERE id='am1'), 'am1', 'assistant', 'cargo finished compiling', '')",
            [],
        )
        .unwrap();

        let out = DbSearch.search_conversations(&c, "cargo", None).unwrap();
        assert!(out.iter().any(|r| r.source == "agent_turn" && r.source_id == "s1"));
        assert!(out.iter().any(|r| r.source == "agent_message" && r.message_id.as_deref() == Some("am1")));
    }

    #[test]
    fn substring_like_fallback_for_short_cjk_query() {
        let c = conn();
        c.execute(
            "INSERT INTO agent_sessions (id, title, space_id) VALUES ('s1', 'Chat', 'ws1')",
            [],
        )
        .unwrap();
        // 2-char CJK query: build_fts_query produces a phrase but the trigram
        // tokenizer can't MATCH <3 codepoints, so only the LIKE fallback (branch 5)
        // surfaces this row.
        c.execute(
            "INSERT INTO agent_messages (id, session_id, role, content, created_at)
             VALUES ('am1', 's1', 'user', '现在几点了', 100)",
            [],
        )
        .unwrap();

        let out = DbSearch.search_conversations(&c, "几点", None).unwrap();
        assert!(
            out.iter().any(|r| r.source == "agent_message" && r.message_id.as_deref() == Some("am1")),
            "LIKE fallback should surface the CJK row, got: {:?}",
            out.iter().map(|r| (&r.source, &r.id)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn session_scope_suppresses_global_title_branch() {
        let c = conn();
        // A title that would match globally, but session scope must skip branch 1.
        c.execute(
            "INSERT INTO conversations (id, title, is_agent, workspace_id, updated_at)
             VALUES ('conv1', 'cargo notes', 0, 'ws1', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let out = DbSearch
            .search_conversations(&c, "cargo", Some("session:other"))
            .unwrap();
        assert!(
            !out.iter().any(|r| r.source == "conversation"),
            "session-scoped search must not emit global title hits"
        );
    }
}
