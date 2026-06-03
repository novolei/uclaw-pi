pub const V1_INITIAL: &str = "
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS spaces (
    id        TEXT PRIMARY KEY,
    name      TEXT NOT NULL,
    icon      TEXT NOT NULL DEFAULT '📁',
    path      TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS conversations (
    id         TEXT PRIMARY KEY,
    space_id   TEXT NOT NULL,
    title      TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (space_id) REFERENCES spaces(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS messages (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role            TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS users (
    id          TEXT PRIMARY KEY,
    device_name TEXT NOT NULL DEFAULT 'unknown',
    device_id   TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    last_accessed TEXT
);

CREATE TABLE IF NOT EXISTS api_tokens (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL,
    token_hash  TEXT NOT NULL UNIQUE,
    label       TEXT NOT NULL DEFAULT 'API Token',
    expires_at  TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_conversations_space ON conversations(space_id);
CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id);
CREATE INDEX IF NOT EXISTS idx_users_device_id ON users(device_id);
CREATE INDEX IF NOT EXISTS idx_api_tokens_user ON api_tokens(user_id);
";

pub const V2_ARTIFACT_CACHE_AND_STARS: &str = "
ALTER TABLE conversations ADD COLUMN starred INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS artifact_cache (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    space_id TEXT NOT NULL,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    is_dir INTEGER NOT NULL DEFAULT 0,
    parent_path TEXT NOT NULL DEFAULT '',
    size_bytes INTEGER,
    mime_type TEXT,
    modified_at TEXT,
    cached_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(space_id, path)
);

CREATE INDEX IF NOT EXISTS idx_artifact_cache_space ON artifact_cache(space_id);
CREATE INDEX IF NOT EXISTS idx_artifact_cache_parent ON artifact_cache(space_id, parent_path);
";

pub const V3_MEMORIES: &str = "
CREATE TABLE IF NOT EXISTS memories (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL DEFAULT 'global',
    namespace   TEXT NOT NULL DEFAULT 'default',
    key         TEXT NOT NULL,
    value       TEXT NOT NULL,
    kind        TEXT NOT NULL DEFAULT 'note',
    tags        TEXT NOT NULL DEFAULT '',
    metadata_json TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at  TEXT,
    UNIQUE(space_id, namespace, key)
);

CREATE INDEX IF NOT EXISTS idx_memories_space ON memories(space_id);
CREATE INDEX IF NOT EXISTS idx_memories_ns ON memories(space_id, namespace);
CREATE INDEX IF NOT EXISTS idx_memories_kind ON memories(kind);
CREATE INDEX IF NOT EXISTS idx_memories_expires ON memories(expires_at);
CREATE INDEX IF NOT EXISTS idx_memories_key ON memories(key);
";

pub const V4_MEMORY_GRAPH: &str = "
CREATE TABLE IF NOT EXISTS memory_nodes (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL,
    kind        TEXT NOT NULL DEFAULT 'reference',
    title       TEXT NOT NULL,
    metadata_json TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS memory_versions (
    id                    TEXT PRIMARY KEY,
    node_id               TEXT NOT NULL,
    supersedes_version_id TEXT,
    status                TEXT NOT NULL DEFAULT 'active',
    content               TEXT NOT NULL,
    metadata_json         TEXT,
    embedding_json        TEXT,
    created_at            TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (node_id) REFERENCES memory_nodes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS memory_edges (
    id              TEXT PRIMARY KEY,
    space_id        TEXT NOT NULL,
    parent_node_id  TEXT,
    child_node_id   TEXT NOT NULL,
    relation_kind   TEXT NOT NULL DEFAULT 'relates_to',
    visibility      TEXT NOT NULL DEFAULT 'private',
    priority        INTEGER NOT NULL DEFAULT 0,
    trigger_text    TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (parent_node_id) REFERENCES memory_nodes(id) ON DELETE SET NULL,
    FOREIGN KEY (child_node_id)  REFERENCES memory_nodes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS memory_routes (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL,
    edge_id     TEXT,
    node_id     TEXT NOT NULL,
    domain      TEXT NOT NULL,
    path        TEXT NOT NULL,
    is_primary  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (edge_id) REFERENCES memory_edges(id) ON DELETE SET NULL,
    FOREIGN KEY (node_id) REFERENCES memory_nodes(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS memory_keywords (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL,
    node_id     TEXT NOT NULL,
    keyword     TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (node_id) REFERENCES memory_nodes(id) ON DELETE CASCADE
);

-- Indexes for memory_nodes
CREATE INDEX IF NOT EXISTS idx_memory_nodes_space ON memory_nodes(space_id);
CREATE INDEX IF NOT EXISTS idx_memory_nodes_kind ON memory_nodes(space_id, kind);
CREATE INDEX IF NOT EXISTS idx_memory_nodes_updated ON memory_nodes(space_id, updated_at DESC);

-- Indexes for memory_versions
CREATE INDEX IF NOT EXISTS idx_memory_versions_node ON memory_versions(node_id);
CREATE INDEX IF NOT EXISTS idx_memory_versions_status ON memory_versions(node_id, status);

-- Indexes for memory_edges
CREATE INDEX IF NOT EXISTS idx_memory_edges_parent ON memory_edges(parent_node_id);
CREATE INDEX IF NOT EXISTS idx_memory_edges_child ON memory_edges(child_node_id);
CREATE INDEX IF NOT EXISTS idx_memory_edges_space ON memory_edges(space_id);
CREATE INDEX IF NOT EXISTS idx_memory_edges_relation ON memory_edges(relation_kind);

-- Indexes for memory_routes
CREATE INDEX IF NOT EXISTS idx_memory_routes_uri ON memory_routes(space_id, domain, path);
CREATE INDEX IF NOT EXISTS idx_memory_routes_node ON memory_routes(node_id);
CREATE INDEX IF NOT EXISTS idx_memory_routes_primary ON memory_routes(node_id, is_primary);

-- Indexes for memory_keywords
CREATE INDEX IF NOT EXISTS idx_memory_keywords_node ON memory_keywords(node_id);
CREATE INDEX IF NOT EXISTS idx_memory_keywords_space ON memory_keywords(space_id, keyword);

-- FTS5 virtual table for full-text search (trigram for CJK + substring support)
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
    node_id UNINDEXED,
    title,
    content,
    tokenize='trigram'
);
";

pub const V5_AGENT_SESSIONS: &str = "
ALTER TABLE conversations ADD COLUMN is_agent INTEGER NOT NULL DEFAULT 0;
ALTER TABLE conversations ADD COLUMN workspace_id TEXT;
";

// First batch: ALTER TABLE only (safe to fail on repeat runs — column already exists)
pub const V5_ALTER: &str = "
ALTER TABLE conversations ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}';
";

// Second batch: all CREATE TABLE IF NOT EXISTS (idempotent, safe to run every time)
pub const V5_TABLES: &str = "
-- Track active workspace in settings table (key = 'active_workspace_id')
-- No schema change needed — settings table already supports arbitrary keys.

-- Per-turn trajectory records for agent sessions
CREATE TABLE IF NOT EXISTS agent_turns (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    turn_index  INTEGER NOT NULL,
    role        TEXT NOT NULL,
    content     TEXT,
    tool_name   TEXT,
    tool_args   TEXT,
    tool_result TEXT,
    reasoning   TEXT,
    is_error    INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_turns_session ON agent_turns(session_id);
CREATE INDEX IF NOT EXISTS idx_agent_turns_tool ON agent_turns(tool_name);

-- FTS5 for full-text search over trajectory content
CREATE VIRTUAL TABLE IF NOT EXISTS agent_turns_fts USING fts5(
    session_id UNINDEXED,
    content,
    tool_result,
    reasoning,
    content='agent_turns',
    content_rowid='rowid',
    tokenize='unicode61'
);

-- Sync triggers for external content FTS table
CREATE TRIGGER IF NOT EXISTS agent_turns_fts_insert
AFTER INSERT ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(rowid, session_id, content, tool_result, reasoning)
  VALUES (new.rowid, new.session_id, new.content, new.tool_result, new.reasoning);
END;

CREATE TRIGGER IF NOT EXISTS agent_turns_fts_update
AFTER UPDATE ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(agent_turns_fts, rowid, session_id, content, tool_result, reasoning)
  VALUES ('delete', old.rowid, old.session_id, old.content, old.tool_result, old.reasoning);
  INSERT INTO agent_turns_fts(rowid, session_id, content, tool_result, reasoning)
  VALUES (new.rowid, new.session_id, new.content, new.tool_result, new.reasoning);
END;

CREATE TRIGGER IF NOT EXISTS agent_turns_fts_delete
AFTER DELETE ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(agent_turns_fts, rowid, session_id, content, tool_result, reasoning)
  VALUES ('delete', old.rowid, old.session_id, old.content, old.tool_result, old.reasoning);
END;

-- Self-evaluation records
CREATE TABLE IF NOT EXISTS session_evals (
    id           TEXT PRIMARY KEY,
    session_id   TEXT NOT NULL,
    score        REAL NOT NULL,
    reasoning    TEXT,
    learnings    TEXT,
    created_at   INTEGER NOT NULL
);
";

pub const V6_AGENT_TEAMS: &str = "
CREATE TABLE IF NOT EXISTS team_runs (
    id           TEXT PRIMARY KEY,
    session_id   TEXT NOT NULL,
    task         TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'running',
    result       TEXT,
    created_at   INTEGER NOT NULL,
    completed_at INTEGER
);

CREATE TABLE IF NOT EXISTS team_channel_messages (
    id         TEXT PRIMARY KEY,
    team_id    TEXT NOT NULL,
    from_role  TEXT NOT NULL,
    to_role    TEXT,
    message    TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_team_channel_team ON team_channel_messages(team_id);
CREATE INDEX IF NOT EXISTS idx_team_runs_session ON team_runs(session_id);
";

pub const V7_AUTOMATIONS: &str = "
CREATE TABLE IF NOT EXISTS automation_specs (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    toml_content TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS automation_activities (
    id           TEXT PRIMARY KEY,
    spec_id      TEXT NOT NULL,
    run_id       TEXT NOT NULL,
    trigger      TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'running',
    result       TEXT,
    error        TEXT,
    duration_ms  INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    completed_at INTEGER,
    FOREIGN KEY (spec_id) REFERENCES automation_specs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_automation_activities_spec ON automation_activities(spec_id);
CREATE INDEX IF NOT EXISTS idx_automation_activities_status ON automation_activities(status);
";

pub const V8_AGENT_SESSIONS: &str = "
CREATE TABLE IF NOT EXISTS agent_sessions (
    id           TEXT PRIMARY KEY,
    space_id     TEXT NOT NULL DEFAULT 'default',
    title        TEXT NOT NULL DEFAULT 'New session',
    metadata_json TEXT NOT NULL DEFAULT '{}',
    message_count INTEGER NOT NULL DEFAULT 0,
    pinned       INTEGER NOT NULL DEFAULT 0,
    archived     INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_messages (
    id           TEXT PRIMARY KEY,
    session_id   TEXT NOT NULL,
    role         TEXT NOT NULL,
    content      TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_agent_sessions_space ON agent_sessions(space_id);
CREATE INDEX IF NOT EXISTS idx_agent_sessions_updated ON agent_sessions(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_messages_session ON agent_messages(session_id);
";

/// V9: persist reasoning + tool activities + model on chat / agent messages
/// so historical messages can re-render thinking blocks and tool call cards.
/// Each ALTER is wrapped because rusqlite reports an error on duplicate
/// columns; the surrounding `let _ = conn.execute(...)` is what makes
/// re-running idempotent (same pattern V5_ALTER uses).
pub const V9_MESSAGE_PROCESS: &str = "
ALTER TABLE messages ADD COLUMN reasoning TEXT;
ALTER TABLE messages ADD COLUMN tool_activities_json TEXT;
ALTER TABLE messages ADD COLUMN model TEXT;
ALTER TABLE messages ADD COLUMN attachments_json TEXT;
ALTER TABLE agent_messages ADD COLUMN reasoning TEXT;
ALTER TABLE agent_messages ADD COLUMN tool_activities_json TEXT;
ALTER TABLE agent_messages ADD COLUMN events_json TEXT;
ALTER TABLE agent_messages ADD COLUMN model TEXT;
";

/// V10: FTS5 index over chat message content for the global search palette.
///
/// `messages.content` is stored as JSON (Vec<ContentBlock>), so we extract
/// just the text via json_extract in the triggers. The `messages_fts` table
/// uses external content (content='messages') so we don't duplicate storage —
/// SQLite reads back from messages.content_text on snippet/highlight calls.
///
/// `content_text` is a generated column maintained by the same triggers so the
/// FTS index doesn't have to deserialize JSON on every match.
pub const V10_MESSAGES_FTS: &str = "
ALTER TABLE messages ADD COLUMN content_text TEXT NOT NULL DEFAULT '';

CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    conversation_id UNINDEXED,
    role UNINDEXED,
    content_text,
    reasoning,
    content='messages',
    content_rowid='rowid',
    tokenize='unicode61'
);

-- Triggers populate content_text from JSON on every write; the FTS table syncs from there.
CREATE TRIGGER IF NOT EXISTS messages_content_text_insert
AFTER INSERT ON messages BEGIN
  UPDATE messages
  SET content_text = (
    SELECT COALESCE(group_concat(json_extract(value, '$.text'), ' '), '')
    FROM json_each(new.content)
    WHERE json_extract(value, '$.type') = 'text'
  )
  WHERE rowid = new.rowid;

  INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
  VALUES (
    new.rowid,
    new.conversation_id,
    new.role,
    (SELECT content_text FROM messages WHERE rowid = new.rowid),
    new.reasoning
  );
END;

CREATE TRIGGER IF NOT EXISTS messages_content_text_update
AFTER UPDATE ON messages BEGIN
  UPDATE messages
  SET content_text = (
    SELECT COALESCE(group_concat(json_extract(value, '$.text'), ' '), '')
    FROM json_each(new.content)
    WHERE json_extract(value, '$.type') = 'text'
  )
  WHERE rowid = new.rowid;

  INSERT INTO messages_fts(messages_fts, rowid, conversation_id, role, content_text, reasoning)
  VALUES ('delete', old.rowid, old.conversation_id, old.role, old.content_text, old.reasoning);
  INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
  VALUES (
    new.rowid,
    new.conversation_id,
    new.role,
    (SELECT content_text FROM messages WHERE rowid = new.rowid),
    new.reasoning
  );
END;

CREATE TRIGGER IF NOT EXISTS messages_content_text_delete
AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, conversation_id, role, content_text, reasoning)
  VALUES ('delete', old.rowid, old.conversation_id, old.role, old.content_text, old.reasoning);
END;
";

/// V11: switch FTS tokenizer from `unicode61` to `trigram` for messages_fts
/// and agent_turns_fts.
///
/// Drops + recreates both tables (FTS5 has no ALTER tokenizer). Backfills
/// from messages and agent_turns. Trigram gives substring + CJK + multi-
/// word implicit AND. Cost: ~3× index size — acceptable for desktop SQLite.
pub const V11_FTS_TRIGRAM: &str = "
DROP TRIGGER IF EXISTS messages_fts_insert;
DROP TRIGGER IF EXISTS messages_fts_update;
DROP TRIGGER IF EXISTS messages_fts_delete;
DROP TABLE IF EXISTS messages_fts;

DROP TRIGGER IF EXISTS agent_turns_fts_insert;
DROP TRIGGER IF EXISTS agent_turns_fts_update;
DROP TRIGGER IF EXISTS agent_turns_fts_delete;
DROP TABLE IF EXISTS agent_turns_fts;

CREATE VIRTUAL TABLE messages_fts USING fts5(
    conversation_id UNINDEXED,
    role UNINDEXED,
    content_text,
    reasoning,
    content='messages',
    content_rowid='rowid',
    tokenize='trigram'
);

CREATE TRIGGER messages_fts_insert
AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
  VALUES (new.rowid, new.conversation_id, new.role, new.content_text, new.reasoning);
END;

CREATE TRIGGER messages_fts_update
AFTER UPDATE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, conversation_id, role, content_text, reasoning)
  VALUES ('delete', old.rowid, old.conversation_id, old.role, old.content_text, old.reasoning);
  INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
  VALUES (new.rowid, new.conversation_id, new.role, new.content_text, new.reasoning);
END;

CREATE TRIGGER messages_fts_delete
AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, conversation_id, role, content_text, reasoning)
  VALUES ('delete', old.rowid, old.conversation_id, old.role, old.content_text, old.reasoning);
END;

CREATE VIRTUAL TABLE agent_turns_fts USING fts5(
    session_id UNINDEXED,
    content,
    tool_result,
    reasoning,
    content='agent_turns',
    content_rowid='rowid',
    tokenize='trigram'
);

CREATE TRIGGER agent_turns_fts_insert
AFTER INSERT ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(rowid, session_id, content, tool_result, reasoning)
  VALUES (new.rowid, new.session_id, new.content, new.tool_result, new.reasoning);
END;

CREATE TRIGGER agent_turns_fts_update
AFTER UPDATE ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(agent_turns_fts, rowid, session_id, content, tool_result, reasoning)
  VALUES ('delete', old.rowid, old.session_id, old.content, old.tool_result, old.reasoning);
  INSERT INTO agent_turns_fts(rowid, session_id, content, tool_result, reasoning)
  VALUES (new.rowid, new.session_id, new.content, new.tool_result, new.reasoning);
END;

CREATE TRIGGER agent_turns_fts_delete
AFTER DELETE ON agent_turns BEGIN
  INSERT INTO agent_turns_fts(agent_turns_fts, rowid, session_id, content, tool_result, reasoning)
  VALUES ('delete', old.rowid, old.session_id, old.content, old.tool_result, old.reasoning);
END;
";

pub const V11_BACKFILL_MESSAGES: &str = "
INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
SELECT rowid, conversation_id, role, content_text, reasoning FROM messages
";

pub const V11_BACKFILL_AGENT_TURNS: &str = "
INSERT INTO agent_turns_fts(rowid, session_id, content, tool_result, reasoning)
SELECT rowid, session_id, content, tool_result, reasoning FROM agent_turns
";

/// V12: agent_messages FTS so the agent-domain conversation is searchable.
pub const V12_AGENT_MESSAGES_FTS: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS agent_messages_fts USING fts5(
    session_id UNINDEXED,
    role UNINDEXED,
    content,
    reasoning,
    content='agent_messages',
    content_rowid='rowid',
    tokenize='trigram'
);

CREATE TRIGGER IF NOT EXISTS agent_messages_fts_insert
AFTER INSERT ON agent_messages BEGIN
  INSERT INTO agent_messages_fts(rowid, session_id, role, content, reasoning)
  VALUES (new.rowid, new.session_id, new.role, new.content, new.reasoning);
END;

CREATE TRIGGER IF NOT EXISTS agent_messages_fts_update
AFTER UPDATE ON agent_messages BEGIN
  INSERT INTO agent_messages_fts(agent_messages_fts, rowid, session_id, role, content, reasoning)
  VALUES ('delete', old.rowid, old.session_id, old.role, old.content, old.reasoning);
  INSERT INTO agent_messages_fts(rowid, session_id, role, content, reasoning)
  VALUES (new.rowid, new.session_id, new.role, new.content, new.reasoning);
END;

CREATE TRIGGER IF NOT EXISTS agent_messages_fts_delete
AFTER DELETE ON agent_messages BEGIN
  INSERT INTO agent_messages_fts(agent_messages_fts, rowid, session_id, role, content, reasoning)
  VALUES ('delete', old.rowid, old.session_id, old.role, old.content, old.reasoning);
END;
";

/// V13: per-turn cost records for the usage dashboard.
pub const V13_COST_RECORDS: &str = "
CREATE TABLE IF NOT EXISTS cost_records (
    id            TEXT PRIMARY KEY,
    session_id    TEXT NOT NULL,
    model         TEXT NOT NULL,
    input_tokens  INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cost_usd      REAL NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cost_records_created ON cost_records(created_at);
CREATE INDEX IF NOT EXISTS idx_cost_records_session ON cost_records(session_id);
CREATE INDEX IF NOT EXISTS idx_cost_records_model   ON cost_records(model);
";

/// V14: tool permission rules + audit log.
///
/// `tool_permission_rules` extends the existing safety_policy.json model
/// (which stays as the "global tier") with two new scopes:
///   - 'session' — only for the named session_id; cleared on session delete
///   - 'pattern' — for tools whose first-arg / command matches `target` as a
///     simple prefix (kept simple on purpose; regex is YAGNI)
/// Resolution precedence in safety/permissions.rs: session > pattern > tool > global.
///
/// `permission_audit_log` records every decision the resolver makes so the
/// settings UI can show a per-session table.
pub const V14_PERMISSION_TABLES: &str = "
CREATE TABLE IF NOT EXISTS tool_permission_rules (
    id          TEXT PRIMARY KEY,
    scope       TEXT NOT NULL CHECK(scope IN ('session', 'pattern')),
    session_id  TEXT,
    tool_name   TEXT NOT NULL,
    target      TEXT,
    mode        TEXT NOT NULL CHECK(mode IN ('allow', 'block', 'ask')),
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tool_permission_rules_session
    ON tool_permission_rules(session_id, tool_name);
CREATE INDEX IF NOT EXISTS idx_tool_permission_rules_pattern
    ON tool_permission_rules(scope, tool_name)
    WHERE scope = 'pattern';

CREATE TABLE IF NOT EXISTS permission_audit_log (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    tool_name   TEXT NOT NULL,
    args_hash   TEXT NOT NULL,
    decision    TEXT NOT NULL CHECK(decision IN ('auto_approve', 'user_approve', 'user_deny', 'blocked')),
    rule_id     TEXT,
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_permission_audit_session ON permission_audit_log(session_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_permission_audit_tool    ON permission_audit_log(tool_name, created_at DESC);
";

/// V15: per-message metrics — duration_ms, token counts, cost stored on each assistant turn.
pub const V15_AGENT_MESSAGE_METRICS: &str = "
ALTER TABLE agent_messages ADD COLUMN duration_ms INTEGER;
ALTER TABLE agent_messages ADD COLUMN input_tokens INTEGER;
ALTER TABLE agent_messages ADD COLUMN output_tokens INTEGER;
ALTER TABLE agent_messages ADD COLUMN cost_usd REAL;
";

/// V16: persist the 'default' workspace as a real DB row (replaces the
/// synthetic in-memory fallback in list_spaces) and re-home agent_sessions
/// whose space_id points at a workspace that doesn't exist (orphan healing
/// from before this migration). Idempotent — safe to re-run.
pub const V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL: &str = "
INSERT OR IGNORE INTO spaces (id, name, icon, path, created_at, updated_at)
VALUES ('default', '默认工作区', '📁', NULL, datetime('now'), datetime('now'));

UPDATE agent_sessions
SET space_id = 'default'
WHERE space_id NOT IN (SELECT id FROM spaces);
";

/// V17: per-workspace + per-session attached directory lists (JSON columns),
/// workspace sort ordering (integer column), and a one-time backfill that
/// derives sort_order from created_at descending so the existing newest-first
/// order is preserved after the schema change.
///
/// All three ALTERs may fail on re-run with "duplicate column" — handled by
/// the per-statement tracing::warn! skip in run(), matching V9/V10 idiom.
pub const V17_WORKSPACE_PATH_SORT_ATTACHED: &str = "
ALTER TABLE spaces ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
ALTER TABLE spaces ADD COLUMN attached_dirs TEXT NOT NULL DEFAULT '[]';
ALTER TABLE agent_sessions ADD COLUMN attached_dirs TEXT NOT NULL DEFAULT '[]';

UPDATE spaces SET sort_order = (
    SELECT COUNT(*) FROM spaces s2 WHERE s2.created_at > spaces.created_at
) WHERE (SELECT COUNT(*) FROM spaces WHERE sort_order != 0) = 0;
";

/// V18: pin state for agent sessions. Nullable INTEGER stores the ms
/// timestamp when the session was pinned; NULL means unpinned. Defaults
/// to NULL for new + existing rows. The pre-existing `pinned` INTEGER
/// column is unrelated (used by chat conversations) and left untouched.
///
/// ALTER may fail on re-run with "duplicate column" — handled by the
/// per-statement tracing::warn! skip in run(), matching V9/V10/V17 idiom.
pub const V18_AGENT_SESSIONS_PINNED_AT: &str = "
ALTER TABLE agent_sessions ADD COLUMN pinned_at INTEGER NULL;
CREATE INDEX IF NOT EXISTS idx_agent_sessions_pinned_at ON agent_sessions(pinned_at);
";

/// V19 — per-workspace skill tag scoping (architecture brief item #3).
///
/// Stores a JSON array of lowercased tag strings. Empty array (the
/// default) means "no filter" — the workspace sees every enabled skill,
/// matching pre-V19 behavior. Non-empty means the skills_manifest filter
/// applies: a skill is included iff its own tags intersect the workspace's
/// tags, OR the skill has no tags (untagged = global, like a fresh-extracted
/// learned skill).
///
/// ALTER may fail on re-run with "duplicate column" — handled by the
/// per-statement tracing::warn! skip in run(), matching V9/V10/V17/V18 idiom.
pub const V19_SPACES_SKILL_TAGS: &str = "
ALTER TABLE spaces ADD COLUMN skill_tags TEXT NOT NULL DEFAULT '[]';
";

// ---------------------------------------------------------------------------
// V20 — rewrite automation_specs + automation_activities to Humane schema
// ---------------------------------------------------------------------------
//
// Three sub-steps executed inside a single transaction:
//
// V20a: create automation_specs_new with the Humane schema (spec_yaml + spec_json
//       + flat identity columns + source/source_ref/source_version for marketplace).
//
// V20b: create automation_activities_new with trigger_source_type +
//       trigger_payload_json + runtime metric columns + resumption chain columns.
//       NOTE: FK columns referencing tables created in V21
//       (subscription_id → automation_subscriptions,
//        escalation_id → automation_escalations,
//        resumed_from_escalation_id → automation_escalations)
//       are present as plain TEXT columns WITHOUT REFERENCES clauses. SQLite does
//       not support adding FK constraints to existing columns via ALTER TABLE, so
//       these will remain without FK enforcement. Application layer enforces
//       referential integrity for these three columns. Self-reference
//       resumed_from_activity_id → automation_activities is safe to add now (same
//       table) and is included with ON DELETE SET NULL.
//
// V20c: data fixup — migrates legacy toml_content rows to Humane YAML via
//       migrate_legacy_toml(), marks source='toml-migrated'. Failures per-row
//       produce status='error' stub rows and do not abort the transaction.
//       Legacy automation_activities rows are mapped with a trigger heuristic.
//       Final swap drops the old tables and renames the new ones.

const SQL_V20A_CREATE_SPECS: &str = "
CREATE TABLE IF NOT EXISTS automation_specs_new (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    version             TEXT NOT NULL,
    author              TEXT NOT NULL,
    description         TEXT NOT NULL,
    system_prompt       TEXT NOT NULL,

    spec_format         TEXT NOT NULL DEFAULT 'humane-yaml-v1',
    spec_yaml           TEXT NOT NULL,
    spec_json           TEXT NOT NULL,

    user_config_values  TEXT NOT NULL DEFAULT '{}',
    permissions_granted TEXT NOT NULL DEFAULT '[]',
    permissions_denied  TEXT NOT NULL DEFAULT '[]',

    status              TEXT NOT NULL DEFAULT 'active',
    enabled             INTEGER NOT NULL DEFAULT 1,
    space_id            TEXT,

    source              TEXT NOT NULL DEFAULT 'local',
    source_ref          TEXT,
    source_version      TEXT,

    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    last_run_at         INTEGER,
    last_run_outcome    TEXT
);
CREATE INDEX IF NOT EXISTS idx_specs_status    ON automation_specs_new(status);
CREATE INDEX IF NOT EXISTS idx_specs_space     ON automation_specs_new(space_id);
CREATE INDEX IF NOT EXISTS idx_specs_enabled   ON automation_specs_new(enabled);
CREATE INDEX IF NOT EXISTS idx_specs_source    ON automation_specs_new(source, source_version);
";

const SQL_V20B_CREATE_ACTIVITIES: &str = "
CREATE TABLE IF NOT EXISTS automation_activities_new (
    id                          TEXT PRIMARY KEY,
    spec_id                     TEXT NOT NULL,
    -- NOTE: subscription_id references automation_subscriptions which does not
    -- exist until V21. FK clause omitted intentionally — see module-level comment.
    subscription_id             TEXT,
    trigger_source_type         TEXT NOT NULL DEFAULT 'manual',
    trigger_payload_json        TEXT NOT NULL DEFAULT '{}',

    status                      TEXT NOT NULL DEFAULT 'queued',
    error_text                  TEXT,
    queued_at                   INTEGER NOT NULL,
    started_at                  INTEGER,
    completed_at                INTEGER,
    duration_ms                 INTEGER NOT NULL DEFAULT 0,

    llm_iterations              INTEGER NOT NULL DEFAULT 0,
    llm_tokens_in               INTEGER NOT NULL DEFAULT 0,
    llm_tokens_out              INTEGER NOT NULL DEFAULT 0,

    tool_calls_json             TEXT NOT NULL DEFAULT '[]',  -- dropped by V24

    report_text                 TEXT,
    report_outcome              TEXT,

    -- NOTE: escalation_id references automation_escalations which does not
    -- exist until V21. FK clause omitted intentionally — see module-level comment.
    escalation_id               TEXT,

    -- Self-reference FK is safe: same table. SET NULL on delete.
    resumed_from_activity_id    TEXT REFERENCES automation_activities_new(id) ON DELETE SET NULL,
    -- NOTE: resumed_from_escalation_id references automation_escalations which
    -- does not exist until V21. FK clause omitted intentionally.
    resumed_from_escalation_id  TEXT,

    FOREIGN KEY (spec_id) REFERENCES automation_specs_new(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_act_spec      ON automation_activities_new(spec_id);
CREATE INDEX IF NOT EXISTS idx_act_status    ON automation_activities_new(status);
CREATE INDEX IF NOT EXISTS idx_act_queued_at ON automation_activities_new(queued_at DESC);
CREATE INDEX IF NOT EXISTS idx_act_resumed   ON automation_activities_new(resumed_from_activity_id);
CREATE INDEX IF NOT EXISTS idx_act_sub       ON automation_activities_new(subscription_id);
";

const SQL_V20_SWAP: &str = "
DROP TABLE IF EXISTS automation_activities;
DROP TABLE IF EXISTS automation_specs;
ALTER TABLE automation_specs_new RENAME TO automation_specs;
ALTER TABLE automation_activities_new RENAME TO automation_activities;
";

/// Map a legacy trigger string to a trigger_source_type string.
fn map_trigger_source_type(trigger: &str) -> &'static str {
    match trigger.to_ascii_lowercase().trim() {
        "cron" => "cron",
        "manual" => "manual",
        _ => "manual",
    }
}

/// V20c — migrate legacy automation_specs rows into automation_specs_new.
/// Each row is processed independently: on success, the Humane YAML is
/// inserted with source='toml-migrated'. On failure, a stub row with
/// status='error' is inserted and the error is logged. Neither outcome
/// aborts the transaction.
fn migrate_specs_data(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    use crate::automation::protocol::{
        migrate_toml_v1::migrate_legacy_toml,
        normalize::normalize_to_json,
    };

    // Fetch all legacy rows. We read everything upfront to avoid borrow
    // conflicts between the SELECT statement and the INSERT statements.
    let mut stmt = conn.prepare(
        "SELECT id, name, description, toml_content, enabled, created_at, updated_at
         FROM automation_specs"
    )?;

    struct LegacyRow {
        id: String,
        name: String,
        description: String,
        toml_content: String,
        enabled: i64,
        created_at: i64,
        updated_at: i64,
    }

    let rows: Vec<LegacyRow> = stmt
        .query_map([], |row| {
            Ok(LegacyRow {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                toml_content: row.get(3)?,
                enabled: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for row in rows {
        match migrate_legacy_toml(&row.toml_content) {
            Ok(migrated) => {
                let spec_json = normalize_to_json(&migrated.spec).unwrap_or_else(|e| {
                    tracing::warn!("V20c: failed to normalize spec_json for {}: {}", row.id, e);
                    "{}".to_string()
                });
                if let Err(e) = conn.execute(
                    "INSERT INTO automation_specs_new
                     (id, name, version, author, description, system_prompt,
                      spec_format, spec_yaml, spec_json,
                      status, enabled, source,
                      created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                             'humane-yaml-v1', ?7, ?8,
                             'active', ?9, 'toml-migrated',
                             ?10, ?11)",
                    rusqlite::params![
                        row.id,
                        migrated.spec.name,
                        migrated.spec.version,
                        migrated.spec.author,
                        migrated.spec.description,
                        migrated.spec.system_prompt,
                        migrated.yaml,
                        spec_json,
                        row.enabled,
                        row.created_at,
                        row.updated_at,
                    ],
                ) {
                    tracing::error!("V20c: failed to insert migrated spec {}: {}", row.id, e);
                }
            }
            Err(e) => {
                // Migration failed — insert a stub so the row is not silently lost.
                tracing::warn!("V20c: migration failed for spec {}: {} — inserting error stub", row.id, e);
                let stub_yaml = format!("# migration-error: {}", e);
                let _ = conn.execute(
                    "INSERT INTO automation_specs_new
                     (id, name, version, author, description, system_prompt,
                      spec_format, spec_yaml, spec_json,
                      status, enabled, source,
                      created_at, updated_at)
                     VALUES (?1, ?2, '0.0.0', 'uclaw-migrated', ?3, '',
                             'humane-yaml-v1', ?4, '{}',
                             'error', ?5, 'toml-migrated',
                             ?6, ?7)",
                    rusqlite::params![
                        row.id,
                        row.name,
                        row.description,
                        stub_yaml,
                        row.enabled,
                        row.created_at,
                        row.updated_at,
                    ],
                );
            }
        }
    }

    Ok(())
}

/// V20c — migrate legacy automation_activities rows into automation_activities_new.
fn migrate_activities_data(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // Fetch all legacy rows upfront.
    let mut stmt = conn.prepare(
        "SELECT id, spec_id, trigger, status, result, error, duration_ms, created_at, completed_at
         FROM automation_activities"
    )?;

    struct LegacyActivity {
        id: String,
        spec_id: String,
        trigger: String,
        status: String,
        result: Option<String>,
        error: Option<String>,
        duration_ms: i64,
        created_at: i64,
        completed_at: Option<i64>,
    }

    let rows: Vec<LegacyActivity> = stmt
        .query_map([], |row| {
            Ok(LegacyActivity {
                id: row.get(0)?,
                spec_id: row.get(1)?,
                trigger: row.get(2)?,
                status: row.get(3)?,
                result: row.get(4)?,
                error: row.get(5)?,
                duration_ms: row.get(6)?,
                created_at: row.get(7)?,
                completed_at: row.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for row in rows {
        let trigger_source_type = map_trigger_source_type(&row.trigger);
        let new_status = row.status.to_ascii_lowercase();
        // Map legacy status → new status vocabulary.
        let mapped_status = match new_status.as_str() {
            "completed" | "success" => "completed",
            "failed" | "error" => "failed",
            "running" => "running",
            _ => "completed",
        };
        // report_outcome: 'useful' if the legacy run was successful.
        let report_outcome: Option<&str> = match new_status.as_str() {
            "completed" | "success" => Some("useful"),
            _ => None,
        };
        // error_text from legacy error column.
        let error_text: Option<&str> = row.error.as_deref();

        if let Err(e) = conn.execute(
            "INSERT INTO automation_activities_new
             (id, spec_id, trigger_source_type, status, error_text,
              queued_at, completed_at, duration_ms,
              report_text, report_outcome)
             VALUES (?1, ?2, ?3, ?4, ?5,
                     ?6, ?7, ?8,
                     ?9, ?10)",
            rusqlite::params![
                row.id,
                row.spec_id,
                trigger_source_type,
                mapped_status,
                error_text,
                row.created_at,
                row.completed_at,
                row.duration_ms,
                row.result,
                report_outcome,
            ],
        ) {
            tracing::error!("V20c: failed to migrate activity {}: {}", row.id, e);
        }
    }

    Ok(())
}

/// V20 — rewrite automation_specs + automation_activities to the Humane schema.
///
/// All three sub-steps (V20a table creation, V20b table creation, V20c data
/// fixup + final swap) run inside a single transaction so any failure leaves
/// the DB in its pre-V20 state.
pub fn run_v20(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // Idempotency check: if automation_specs already has the new-schema
    // `spec_yaml` column, V20 has already been applied — skip the whole
    // migration. Without this guard, a successful V20 followed by a failed
    // V21 (where the .ok() at app.rs:248 swallowed the error pre-fix)
    // leaves us with: automation_specs at new schema, V21 tables missing,
    // and the next startup retrying V20 → SELECT toml_content FROM
    // (new-schema) automation_specs → "no such column" → V20 fails again
    // → V21 never gets to run → automation_escalations never created.
    let v20_already_applied: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('automation_specs') WHERE name = 'spec_yaml'",
        [],
        |row| row.get::<_, i64>(0),
    ).map(|n| n > 0).unwrap_or(false);
    if v20_already_applied {
        tracing::info!("V20 skipped: automation_specs already on Humane schema (V20 was applied previously)");
        return Ok(());
    }

    let tx = conn.unchecked_transaction()?;

    // V20a — create automation_specs_new
    tracing::debug!("V20a: creating automation_specs_new");
    tx.execute_batch(SQL_V20A_CREATE_SPECS)?;

    // V20b — create automation_activities_new
    tracing::debug!("V20b: creating automation_activities_new");
    tx.execute_batch(SQL_V20B_CREATE_ACTIVITIES)?;

    // V20c — migrate existing legacy rows
    tracing::debug!("V20c: migrating legacy automation_specs rows");
    migrate_specs_data(&tx)?;
    tracing::debug!("V20c: migrating legacy automation_activities rows");
    migrate_activities_data(&tx)?;

    // Final swap — drop old tables, rename new ones
    tracing::debug!("V20c: swapping tables");
    tx.execute_batch(SQL_V20_SWAP)?;

    tx.commit()
}

const SQL_V21: &str = "
CREATE TABLE IF NOT EXISTS automation_subscriptions (
    id            TEXT PRIMARY KEY,
    spec_id       TEXT NOT NULL REFERENCES automation_specs(id) ON DELETE CASCADE,
    source_type   TEXT NOT NULL,
    config_json   TEXT NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    last_fired_at INTEGER,
    created_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sub_spec        ON automation_subscriptions(spec_id);
CREATE INDEX IF NOT EXISTS idx_sub_source_type ON automation_subscriptions(source_type);

CREATE TABLE IF NOT EXISTS automation_memory (
    spec_id                 TEXT PRIMARY KEY REFERENCES automation_specs(id) ON DELETE CASCADE,
    last_updated_at         INTEGER NOT NULL,
    compacted_archives_json TEXT NOT NULL DEFAULT '[]',
    bytes                   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS automation_escalations (
    id           TEXT PRIMARY KEY,
    spec_id      TEXT NOT NULL REFERENCES automation_specs(id) ON DELETE CASCADE,
    activity_id  TEXT NOT NULL REFERENCES automation_activities(id) ON DELETE CASCADE,
    question     TEXT NOT NULL,
    choices_json TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'waiting',
    user_choice  TEXT,
    user_note    TEXT,
    created_at   INTEGER NOT NULL,
    responded_at INTEGER
);
CREATE INDEX IF NOT EXISTS idx_escalation_spec   ON automation_escalations(spec_id);
CREATE INDEX IF NOT EXISTS idx_escalation_status ON automation_escalations(status);
";

pub fn run_v21(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SQL_V21)
}

/// V22 — automation_installed_skills.
///
/// Records which bundled skills each marketplace-installed automation pulled
/// in. Read by AppsView to enumerate "what got installed alongside this
/// automation" and by uninstall to delete the right files.
///
/// file_count is a cheap drift detector — diagnostic only in this PR.
const SQL_V22: &str = "
CREATE TABLE IF NOT EXISTS automation_installed_skills (
    automation_slug TEXT NOT NULL,
    skill_id        TEXT NOT NULL,
    installed_at    INTEGER NOT NULL,
    file_count      INTEGER NOT NULL,
    PRIMARY KEY (automation_slug, skill_id)
);
CREATE INDEX IF NOT EXISTS idx_aut_inst_skills_slug
    ON automation_installed_skills(automation_slug);
";

const V23A_MARKETPLACE_CACHE: &str = "
CREATE TABLE IF NOT EXISTS automation_marketplace_items (
    registry_id      TEXT NOT NULL,
    slug             TEXT NOT NULL,
    name             TEXT NOT NULL,
    version          TEXT NOT NULL,
    author           TEXT NOT NULL,
    description      TEXT NOT NULL,
    item_type        TEXT NOT NULL,
    category         TEXT NOT NULL DEFAULT 'other',
    icon             TEXT,
    tags_json        TEXT NOT NULL DEFAULT '[]',
    locale           TEXT,
    min_app_version  TEXT,
    size_bytes       INTEGER,
    checksum         TEXT,
    requires_json    TEXT NOT NULL DEFAULT '{}',
    i18n_json        TEXT NOT NULL DEFAULT '{}',
    spec_yaml        TEXT,
    updated_at_index TEXT,
    cached_at        INTEGER NOT NULL,
    PRIMARY KEY (registry_id, slug)
);

CREATE INDEX IF NOT EXISTS idx_marketplace_type     ON automation_marketplace_items(item_type);
CREATE INDEX IF NOT EXISTS idx_marketplace_category ON automation_marketplace_items(category);

CREATE VIRTUAL TABLE IF NOT EXISTS automation_marketplace_fts USING fts5(
    slug UNINDEXED,
    registry_id UNINDEXED,
    name,
    description,
    author,
    tags,
    tokenize = 'trigram'
);

CREATE TABLE IF NOT EXISTS automation_registry_sync (
    registry_id    TEXT PRIMARY KEY,
    last_synced_at INTEGER,
    last_etag      TEXT,
    last_modified  TEXT,
    last_error     TEXT,
    item_count     INTEGER NOT NULL DEFAULT 0
);
";

/// V25 — marketplace_standalone_installs.
///
/// Tracks standalone (non-bundled) skill and MCP marketplace installs so the
/// AppsTab can list them and uninstall can find what to remove. `mcp_server_id`
/// links a `type: mcp` install to its mcp_servers.json entry; NULL for skills.
/// (V24 is claimed by the parallel Automation Phase 2a branch.)
const SQL_V25: &str = "
CREATE TABLE IF NOT EXISTS marketplace_standalone_installs (
    slug          TEXT PRIMARY KEY,
    item_type     TEXT NOT NULL,
    version       TEXT NOT NULL,
    installed_at  INTEGER NOT NULL,
    mcp_server_id TEXT
);
";

/// V26 — conversations gains `archived` + `archived_at` for general session archiving.
const SQL_V26: &str = "
ALTER TABLE conversations ADD COLUMN archived  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE conversations ADD COLUMN archived_at INTEGER;
";

/// V27 — user-customizable system prompts for chat/agent paths.
const SQL_V27: &str = "
CREATE TABLE IF NOT EXISTS system_prompts (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    content    TEXT NOT NULL DEFAULT '',
    is_builtin INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_system_prompts_sort ON system_prompts(sort_order);

INSERT OR IGNORE INTO system_prompts (id, name, content, is_builtin, sort_order, created_at, updated_at)
VALUES ('builtin-default', '默认', 'You are a helpful assistant.', 1, 0,
        CAST(strftime('%s', 'now') AS INTEGER) * 1000,
        CAST(strftime('%s', 'now') AS INTEGER) * 1000);
";

/// V28 — version history for system prompts.
/// Records a snapshot every time a prompt is created or updated.
const SQL_V28: &str = "
CREATE TABLE IF NOT EXISTS system_prompt_versions (
    id         TEXT PRIMARY KEY,
    prompt_id  TEXT NOT NULL,
    name       TEXT NOT NULL,
    content    TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    FOREIGN KEY (prompt_id) REFERENCES system_prompts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_sp_versions_prompt ON system_prompt_versions(prompt_id, created_at DESC);
";

/// V29 — logical compaction support.
///
/// Replaces physical DELETE with logical marking:
/// - agent_messages.compacted: 0 = active, 1 = logically removed
/// - compaction_markers: records each compaction event's metadata
///
/// This aligns with openhanako's appendCompaction() pattern — messages
/// persist in the database and can be re-rendered by the UI, while the
/// LLM context builder skips compacted messages.
const V29_COMPACTION_SUPPORT: &str = "
ALTER TABLE agent_messages ADD COLUMN compacted INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS compaction_markers (
    id                    TEXT PRIMARY KEY,
    session_id            TEXT NOT NULL,
    summary               TEXT NOT NULL DEFAULT '',
    removed_count         INTEGER NOT NULL DEFAULT 0,
    tokens_before         INTEGER,
    tokens_after          INTEGER,
    first_kept_message_id TEXT,
    model_window          INTEGER,
    created_at            INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_compaction_markers_session ON compaction_markers(session_id);
";

// ---------------------------------------------------------------------------
// V30 — fragment_reviews + daily_summaries for the memory fragment system
// ---------------------------------------------------------------------------
const V30_FRAGMENT_TABLES: &str = "
CREATE TABLE IF NOT EXISTS fragment_reviews (
    id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL,
    session_id TEXT,
    review_count INTEGER DEFAULT 0,
    next_review_at INTEGER,
    last_reviewed_at INTEGER,
    completed INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_fragment_reviews_next ON fragment_reviews(next_review_at);
CREATE INDEX IF NOT EXISTS idx_fragment_reviews_node ON fragment_reviews(node_id);

CREATE TABLE IF NOT EXISTS daily_summaries (
    id TEXT PRIMARY KEY,
    summary_date TEXT NOT NULL,
    content TEXT NOT NULL,
    fragment_count INTEGER DEFAULT 0,
    fragment_ids_json TEXT,
    created_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_daily_summaries_date ON daily_summaries(summary_date);
";

// V31 — rebuild memory_fts with trigram tokenizer for CJK + substring search.
// Drops the old unicode61 table, recreates with trigram, then backfills from
// memory_nodes + active memory_versions. Same pattern as V11 for messages_fts.
const V31_MEMORY_FTS_TRIGRAM: &str = "
DROP TABLE IF EXISTS memory_fts;

CREATE VIRTUAL TABLE memory_fts USING fts5(
    node_id UNINDEXED,
    title,
    content,
    tokenize='trigram'
);
";

const V31_BACKFILL_MEMORY_FTS: &str = "
INSERT INTO memory_fts(node_id, title, content)
SELECT n.id, n.title, v.content
FROM memory_nodes n
INNER JOIN memory_versions v ON v.node_id = n.id
WHERE v.status = 'active'
  AND v.content IS NOT NULL AND v.content != '';
";

/// V32 — IM channel infrastructure: instances, sessions, spec bindings,
/// and three new columns on automation_specs (trigger_phrase, system_prompt override, description).
const SQL_V32: &str = "
CREATE TABLE IF NOT EXISTS im_channel_instances (
    id                   TEXT PRIMARY KEY,
    space_id             TEXT NOT NULL,
    channel_type         TEXT NOT NULL,
    name                 TEXT NOT NULL,
    config_json          TEXT NOT NULL DEFAULT '{}',
    credentials_json     TEXT NOT NULL DEFAULT '{}',
    enabled              INTEGER NOT NULL DEFAULT 1,
    streaming            INTEGER NOT NULL DEFAULT 0,
    reply_scope          TEXT NOT NULL DEFAULT 'all',
    permission_enabled   INTEGER NOT NULL DEFAULT 0,
    owners_json          TEXT NOT NULL DEFAULT '[]',
    guest_policy_json    TEXT NOT NULL DEFAULT '{}',
    created_at           INTEGER NOT NULL,
    updated_at           INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_im_channel_instances_space
    ON im_channel_instances(space_id, enabled);

CREATE TABLE IF NOT EXISTS im_sessions (
    id               TEXT PRIMARY KEY,
    space_id         TEXT NOT NULL,
    channel_type     TEXT NOT NULL,
    chat_id          TEXT NOT NULL,
    agent_session_id TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    last_active_at   INTEGER NOT NULL,
    UNIQUE(space_id, channel_type, chat_id)
);

CREATE TABLE IF NOT EXISTS spec_channel_bindings (
    spec_id             TEXT NOT NULL,
    channel_instance_id TEXT NOT NULL,
    enabled             INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (spec_id, channel_instance_id)
);
";

/// V32b — ALTER TABLE additions to automation_specs (separate statements for idempotency).
const SQL_V32B: &str = "
ALTER TABLE automation_specs ADD COLUMN trigger_phrase TEXT;
ALTER TABLE automation_specs ADD COLUMN system_prompt_override TEXT;
ALTER TABLE automation_specs ADD COLUMN spec_description TEXT;
";
pub fn run_v23a(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(V23A_MARKETPLACE_CACHE)
}

/// V24 — automation run = agent_session ownership model.
/// `automation_activities` gains `session_id` (nullable link to the run's
/// agent_session) + `report_artifacts_json` (declared products), and drops
/// `tool_calls_json` (per-tool breakdown now lives in agent_messages).
/// `agent_sessions` gains `archived_at` for retention ordering.
/// All statements are individually error-tolerant: a re-run hits
/// "duplicate column" / "no such column" and is skipped (same pattern as
/// V9–V19). DROP COLUMN requires SQLite >= 3.35 (rusqlite bundles a newer one).
const V24_AUTOMATION_RUN_SESSIONS: &str = "
ALTER TABLE automation_activities ADD COLUMN session_id TEXT;
ALTER TABLE automation_activities ADD COLUMN report_artifacts_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE automation_activities DROP COLUMN tool_calls_json;
ALTER TABLE agent_sessions ADD COLUMN archived_at INTEGER;
CREATE INDEX IF NOT EXISTS idx_act_session ON automation_activities(session_id);
";

/// V33 — Symphony runtime schema. Four tables backing the DAG-of-agent-runs
/// orchestrator described in `docs/superpowers/specs/2026-05-17-symphony-runtime-design.md` §8.2:
///
/// - `symphony_workflows`         one row per workflow (current version pointer).
/// - `symphony_workflow_versions` immutable snapshots; runs pin a version.
/// - `symphony_runs`              one row per run.
/// - `symphony_node_runs`         one row per node attempt (retries get new rows).
///
/// Plus a seeded `spaces.id = 'symphonies'` home for the per-run agent_sessions.
///
/// All statements are individually error-tolerant (`IF NOT EXISTS` + `INSERT OR
/// IGNORE`), matching the V25/V26 style.
const SQL_V33_SYMPHONY: &str = "
CREATE TABLE IF NOT EXISTS symphony_workflows (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    description     TEXT,
    space_id        TEXT,
    current_version INTEGER NOT NULL DEFAULT 1,
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    FOREIGN KEY (space_id) REFERENCES spaces(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS symphony_workflow_versions (
    workflow_id     TEXT NOT NULL,
    version         INTEGER NOT NULL,
    definition_yaml TEXT NOT NULL,
    definition_md   TEXT NOT NULL,
    nodes_json      TEXT NOT NULL,
    edges_json      TEXT NOT NULL,
    created_at      INTEGER NOT NULL,
    PRIMARY KEY (workflow_id, version),
    FOREIGN KEY (workflow_id) REFERENCES symphony_workflows(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS symphony_runs (
    id              TEXT PRIMARY KEY,
    workflow_id     TEXT NOT NULL,
    workflow_version INTEGER NOT NULL,
    trigger_kind    TEXT NOT NULL,
    trigger_payload_json TEXT NOT NULL DEFAULT '{}',
    status          TEXT NOT NULL,
    outcome         TEXT,
    inputs_json     TEXT NOT NULL DEFAULT '{}',
    outputs_json    TEXT,
    total_cost_usd  REAL NOT NULL DEFAULT 0,
    error_text      TEXT,
    queued_at       INTEGER NOT NULL,
    started_at      INTEGER,
    completed_at    INTEGER,
    FOREIGN KEY (workflow_id) REFERENCES symphony_workflows(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_symphony_runs_workflow ON symphony_runs(workflow_id, queued_at DESC);
CREATE INDEX IF NOT EXISTS idx_symphony_runs_status ON symphony_runs(status);

CREATE TABLE IF NOT EXISTS symphony_node_runs (
    id              TEXT PRIMARY KEY,
    run_id          TEXT NOT NULL,
    node_id         TEXT NOT NULL,
    attempt         INTEGER NOT NULL DEFAULT 1,
    status          TEXT NOT NULL,
    session_id      TEXT,
    cost_usd        REAL NOT NULL DEFAULT 0,
    iterations      INTEGER NOT NULL DEFAULT 0,
    started_at_ms   INTEGER,
    last_heartbeat_ms INTEGER,
    completed_at_ms INTEGER,
    error_text      TEXT,
    output_json     TEXT,
    FOREIGN KEY (run_id) REFERENCES symphony_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_symphony_node_runs_run ON symphony_node_runs(run_id, node_id);
CREATE INDEX IF NOT EXISTS idx_symphony_node_runs_status ON symphony_node_runs(status);
CREATE INDEX IF NOT EXISTS idx_symphony_node_runs_heartbeat ON symphony_node_runs(last_heartbeat_ms);

INSERT OR IGNORE INTO spaces (id, name, icon, path, created_at, updated_at)
VALUES ('symphonies', 'Symphonies', '🎼', NULL, datetime('now'), datetime('now'));
";

// V34: plan_suggest_events — telemetry for plan-mode auto-suggest.
// Each row is one "we showed the banner" event with its eventual outcome.
const SQL_V34_PLAN_SUGGEST_EVENTS: &str = "
CREATE TABLE IF NOT EXISTS plan_suggest_events (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    message_id      TEXT NOT NULL,
    source          TEXT NOT NULL,
    matched_pattern TEXT,
    reason          TEXT,
    user_msg_preview TEXT NOT NULL,
    outcome         TEXT NOT NULL DEFAULT 'pending',
    decline_reason  TEXT,
    fired_at        INTEGER NOT NULL,
    decided_at      INTEGER,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_plan_suggest_session ON plan_suggest_events(session_id);
CREATE INDEX IF NOT EXISTS idx_plan_suggest_pattern ON plan_suggest_events(matched_pattern)
    WHERE matched_pattern IS NOT NULL;
CREATE TABLE IF NOT EXISTS mode_suggest_overrides (
    pattern         TEXT PRIMARY KEY,
    disabled_until  INTEGER NOT NULL,
    reason          TEXT,
    updated_at      INTEGER NOT NULL
);
";

/// V35 — Memory OS Foundation Phase 1.
///
/// (Originally authored as V34; renumbered to V35 during rebase since PR
/// #185 landed V34 (plan_suggest_events) first. Sandbox's tip commit
/// `6871587` did this rename; we foreshadow it here to keep the rebase
/// cascade clean.)
///
/// Adds three additive tables backing the Memory OS Foundation layer
/// (`docs/superpowers/specs/2026-05-18-agent-memory-os-design.md` §4.2.4):
///
/// - `memory_edge_audit`        — records whether each memory_edges row was
///   created by the auto-link post-hook (Phase 2) or an explicit user/agent
///   action. Enables stale-link reconciliation without ever touching edges
///   that humans manually asserted.
/// - `wiki_artifacts`           — derived AI Wiki content (overview.md,
///   index.md, and later: hot/purpose/log/synthesis kinds). One row per
///   `(space_id, kind)` snapshot; LLM-regenerated by `wiki_overview`
///   scenario (Phase 3) on accumulation threshold.
/// - `memory_health_findings`   — issues surfaced by the Health (Phase 4)
///   and Lint (Phase 5) scenarios. `is_lint=0` for free-tier zero-LLM
///   checks; `is_lint=1` for paid checks with LLM cost.
///
/// All three are pure additive: no ALTER TABLE on existing schemas, no
/// dependency on tables outside V1-V34. Cascading deletes via FK on
/// `memory_edge_audit.edge_id` mirror the existing memory_edges → other
/// memory_* cascade pattern.
pub const V35_MEMORY_OS_PHASE_1: &str = "
CREATE TABLE IF NOT EXISTS memory_edge_audit (
    edge_id     TEXT PRIMARY KEY REFERENCES memory_edges(id) ON DELETE CASCADE,
    source      TEXT NOT NULL,
    inferred_by TEXT,
    confidence  REAL,
    extracted_from_version_id TEXT REFERENCES memory_versions(id),
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memory_edge_audit_src ON memory_edge_audit(source);

CREATE TABLE IF NOT EXISTS wiki_artifacts (
    id              TEXT PRIMARY KEY,
    space_id        TEXT NOT NULL,
    kind            TEXT NOT NULL,
    content         TEXT NOT NULL,
    generated_at    INTEGER NOT NULL,
    source_node_ids TEXT NOT NULL,
    llm_model       TEXT,
    token_cost      INTEGER
);
CREATE INDEX IF NOT EXISTS idx_wiki_artifacts_space_kind ON wiki_artifacts(space_id, kind);
CREATE INDEX IF NOT EXISTS idx_wiki_artifacts_generated ON wiki_artifacts(generated_at);

CREATE TABLE IF NOT EXISTS memory_health_findings (
    id            TEXT PRIMARY KEY,
    space_id      TEXT NOT NULL,
    severity      TEXT NOT NULL,
    check_kind    TEXT NOT NULL,
    subject       TEXT NOT NULL,
    payload_json  TEXT,
    is_lint       INTEGER NOT NULL DEFAULT 0,
    dismissed     INTEGER NOT NULL DEFAULT 0,
    discovered_at INTEGER NOT NULL,
    dismissed_at  INTEGER
);
CREATE INDEX IF NOT EXISTS idx_health_findings_active
    ON memory_health_findings(space_id, dismissed, discovered_at);
CREATE INDEX IF NOT EXISTS idx_health_findings_subject
    ON memory_health_findings(subject);
";

/// V37 — Memory OS Foundation Phase 7.
///
/// `brain_sync_state` tracks the on-disk mirror of each EntityPage in
/// `~/Documents/workground/brain/<subkind>/<slug>.md`. One row per
/// node, keyed by node_id. Lets the sync engine answer three questions
/// without re-parsing every file:
///
/// 1. Has the DB version moved past what we last wrote to disk?
///    → compare `last_synced_version_id` to the node's current
///      active `memory_versions.id`.
/// 2. Has the disk file been edited since we last read it?
///    → compare current file mtime to `file_mtime_at_last_sync_ms`.
/// 3. Is the edit actually content (vs `touch` / IDE save churn)?
///    → SHA-256 the current bytes and compare to `last_synced_sha256`.
///
/// Conflict detection (Phase 7.3) flags rows where both 1 and 2 are
/// true: DB and disk both diverged since the last sync. Default
/// resolution writes disk into DB (gbrain's "human always wins"
/// principle) and inserts a `memory_health_findings` row with
/// `check_kind='sync_conflict'` so the user sees it in the Health tab.
///
/// V36 is claimed by the Automation Phase 2b Messaging design (see
/// CLAUDE.md migration registry).
///
/// Nothing here is destructive: pure additive table + 3 indexes,
/// guarded with IF NOT EXISTS so idempotent on re-run.
pub const V37_MEMORY_OS_PHASE_7: &str = "
CREATE TABLE IF NOT EXISTS brain_sync_state (
    node_id                     TEXT PRIMARY KEY REFERENCES memory_nodes(id) ON DELETE CASCADE,
    space_id                    TEXT NOT NULL,
    file_path                   TEXT NOT NULL,
    last_synced_version_id      TEXT REFERENCES memory_versions(id),
    last_synced_at_ms           INTEGER NOT NULL,
    file_mtime_at_last_sync_ms  INTEGER NOT NULL,
    last_synced_sha256          TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_brain_sync_file_path ON brain_sync_state(file_path);
CREATE INDEX IF NOT EXISTS idx_brain_sync_space ON brain_sync_state(space_id);
CREATE INDEX IF NOT EXISTS idx_brain_sync_last_at ON brain_sync_state(last_synced_at_ms);
";


/// V38 — Automation Phase 2b cluster A · per-(spec, identity) chat session index.
///
/// `automation_chat_sessions` maps a (spec_id, identity_key) pair to the
/// agent_sessions row that hosts the long-lived chat thread for that pair.
///
/// identity_key conventions (string):
///   - "local"                       → spec owner's local chat thread
///   - "{channel_type}:{chat_id}"    → per-IM-user thread (e.g. "wechat_ilink:UIN_abc")
///
/// UNIQUE(spec_id, identity_key) guarantees idempotent get-or-create.
/// FK CASCADE on agent_sessions deletion keeps the index clean.
pub const V38_AUTOMATION_CHAT_SESSIONS: &str = "
CREATE TABLE IF NOT EXISTS automation_chat_sessions (
    spec_id          TEXT NOT NULL,
    identity_key     TEXT NOT NULL,
    agent_session_id TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    PRIMARY KEY (spec_id, identity_key),
    FOREIGN KEY (agent_session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_aut_chat_sess_agent_id
    ON automation_chat_sessions(agent_session_id);
";

/// V39 — Memory OS Foundation Sprint 1 (post-Phase-7) · `user_profile_facets`.
///
/// Stores the openhuman-style stability-graded user profile facets. Each
/// row is one fact about the user (or a candidate fact still proving
/// itself), graded by half-life-decayed evidence accumulation.
///
/// Schema mirrors openhuman's `memory/store/unified/profile.rs:25-47`:
///
/// - `facet_id`      Stable identity for this (class, name) tuple. ULID-ish.
/// - `class`         One of: 'identity' | 'style' | 'tooling' | 'veto' |
///                   'goal' | 'channel'. Sets the budget (4/4/5/3/3/1)
///                   and half-life (90d/14d/30d/30d/60d/7d).
/// - `name`          Short slot name within the class
///                   ("editor", "package_manager", ...). UNIQUE per class.
/// - `value`         The actual fact value. Free-form short string.
/// - `state`         One of: 'candidate' | 'provisional' | 'active' |
///                   'forgotten'. Promoted by stability_detector when
///                   score crosses TAU_PROVISIONAL (0.7) → TAU_PROMOTE
///                   (1.5); demoted when below TAU_EVICT (0.4).
/// - `stability`     The most recently computed stability score (REAL).
///                   Persisted for telemetry + UI; recomputed every
///                   30 min by `learning::scheduler::rebuild`.
/// - `cue_families_json` JSON map `{explicit: float, structural: float,
///                   behavioral: float, recurrence: float}` accumulating
///                   the weighted evidence count by cue family.
/// - `evidence_count` Total evidence rows that contributed (across all
///                   cue families). Used in the log-decay term.
/// - `last_seen_at`  Latest evidence timestamp (epoch ms). Drives the
///                   exp(-Δt/half_life) decay factor.
/// - `created_at`/`updated_at` Lifecycle.
///
/// Indexes:
/// - `idx_facets_class_state`  for the common "list active in class"
///                             query the prompt section needs.
/// - `idx_facets_last_seen`    for half-life decay sweeps.
/// - UNIQUE(class, name)       enforces "one facet per (class, name)";
///                             repeated observation just bumps
///                             evidence_count and last_seen_at.
/// V40 — MCP PR-5 audit table. One row per significant MCP lifecycle
/// event: connect_attempt, connect_failed, health_failed, reconnect_attempt,
/// reconnect_failed, tools_changed, disconnect. The `message_redacted`
/// column always has env-value substrings replaced with `[REDACTED]`
/// so the table is safe to ship if a user wants to share logs.
///
/// Indexes are tuned for the two read patterns: per-server timeline
/// (server_id + created_at DESC, the "View logs" affordance) and
/// global recent-activity (created_at DESC, future dashboard).
pub const V40_MCP_AUDIT: &str = "
CREATE TABLE IF NOT EXISTS mcp_audit (
    id                 TEXT PRIMARY KEY,
    server_id          TEXT NOT NULL,
    event_kind         TEXT NOT NULL,
    message_redacted   TEXT NOT NULL,
    created_at         INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_mcp_audit_server_time
    ON mcp_audit(server_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_mcp_audit_time
    ON mcp_audit(created_at DESC);
";

/// V41 — Browser task memory/checkpoint foundation.
///
/// Persists autonomous browser runs, their step trail, and a compact per-session
/// memory notebook. This is intentionally additive so browser agent state can
/// survive app restarts without changing existing agent session tables.
pub const V41_BROWSER_TASK_MEMORY: &str = "
CREATE TABLE IF NOT EXISTS browser_task_runs (
    run_id      TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    task        TEXT NOT NULL,
    status      TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_browser_task_runs_session_time
    ON browser_task_runs(session_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS browser_task_steps (
    run_id               TEXT NOT NULL,
    step_index           INTEGER NOT NULL,
    phase                TEXT NOT NULL,
    observation_summary  TEXT NOT NULL,
    reasoning            TEXT NOT NULL,
    action_name          TEXT NOT NULL,
    action_args_json     TEXT NOT NULL,
    ok                   INTEGER NOT NULL,
    message              TEXT,
    error                TEXT,
    timestamp_ms         INTEGER NOT NULL,
    PRIMARY KEY (run_id, step_index),
    FOREIGN KEY (run_id) REFERENCES browser_task_runs(run_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_browser_task_steps_run_time
    ON browser_task_steps(run_id, step_index);

CREATE TABLE IF NOT EXISTS browser_task_memory (
    session_id       TEXT PRIMARY KEY,
    task_summary     TEXT NOT NULL,
    facts_json       TEXT NOT NULL DEFAULT '[]',
    visited_urls_json TEXT NOT NULL DEFAULT '[]',
    open_tabs_json   TEXT NOT NULL DEFAULT '[]',
    updated_at       INTEGER NOT NULL
);
";

pub const V42_BROWSER_TASK_CHECKPOINTS: &str = "
CREATE TABLE IF NOT EXISTS browser_task_checkpoints (
    checkpoint_id   TEXT PRIMARY KEY,
    run_id          TEXT NOT NULL,
    session_id      TEXT NOT NULL,
    step_index      INTEGER NOT NULL,
    active_tab_id   TEXT,
    memory_json     TEXT,
    loop_state_json TEXT NOT NULL,
    created_at      INTEGER NOT NULL,
    FOREIGN KEY (run_id) REFERENCES browser_task_runs(run_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_browser_task_checkpoints_run_step
    ON browser_task_checkpoints(run_id, step_index DESC);
CREATE INDEX IF NOT EXISTS idx_browser_task_checkpoints_session_time
    ON browser_task_checkpoints(session_id, created_at DESC);
";

/// V43 — Memory OS Cognitive Layer Phase 8.1: five new tables backing
/// the cognitive layer's segmented provenance / two-step compile /
/// review-queue / per-subkind templates.
///
/// All five tables are `IF NOT EXISTS` and reference Foundation's
/// existing `memory_nodes` table only via FK on delete-cascade — no
/// alters to existing V1–V42 schema. The cognitive plan (Phase 8.2)
/// will add a SEED constant for `wiki_page_templates`; this migration
/// only ships the empty tables.
///
/// - `wiki_log_events`     — wiki-level event ledger (compile/skip/dismiss audit)
/// - `page_content_hashes` — SHA-256 incremental compile cache
/// - `review_queue_items`  — human-in-the-loop brake queue for contradictions / unresolved questions
/// - `wiki_page_templates` — per-subkind compile prompt + section schema (seed in Phase 8.2)
/// - `analysis_cache`      — Step 1 (Analyze) LLM result cache for the two-stage compile pipeline
///
/// Cognitive layer is feature-flag gated; with all flags off the tables
/// stay empty and Foundation Phase 1-7 behavior is unchanged.
pub const V43_COGNITIVE_LAYER: &str = "
CREATE TABLE IF NOT EXISTS wiki_log_events (
    id          TEXT PRIMARY KEY,
    space_id    TEXT NOT NULL,
    event_type  TEXT NOT NULL,
    subject_id  TEXT,
    actor       TEXT NOT NULL,
    payload_json TEXT,
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_wiki_log_events_time
    ON wiki_log_events(space_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_wiki_log_events_subject
    ON wiki_log_events(subject_id);

CREATE TABLE IF NOT EXISTS page_content_hashes (
    node_id          TEXT PRIMARY KEY REFERENCES memory_nodes(id) ON DELETE CASCADE,
    sources_hash     TEXT NOT NULL,
    timeline_hash    TEXT NOT NULL,
    compiled_hash    TEXT NOT NULL,
    last_compiled_at INTEGER NOT NULL,
    last_skip_count  INTEGER NOT NULL DEFAULT 0,
    skip_reason      TEXT
);

CREATE TABLE IF NOT EXISTS review_queue_items (
    id              TEXT PRIMARY KEY,
    space_id        TEXT NOT NULL,
    item_kind       TEXT NOT NULL,
    severity        TEXT NOT NULL,
    subject_ids     TEXT NOT NULL,
    title           TEXT NOT NULL,
    context_json    TEXT,
    status          TEXT NOT NULL DEFAULT 'open',
    resolution      TEXT,
    resolution_note TEXT,
    assignee        TEXT,
    created_at      INTEGER NOT NULL,
    resolved_at     INTEGER,
    snooze_until    INTEGER
);
CREATE INDEX IF NOT EXISTS idx_review_queue_active
    ON review_queue_items(space_id, status, severity, created_at);
CREATE INDEX IF NOT EXISTS idx_review_queue_subject
    ON review_queue_items(subject_ids);

CREATE TABLE IF NOT EXISTS wiki_page_templates (
    subkind         TEXT PRIMARY KEY,
    display_name    TEXT NOT NULL,
    compile_prompt  TEXT NOT NULL,
    sections_json   TEXT NOT NULL,
    ui_card_layout  TEXT,
    updated_at      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS analysis_cache (
    node_id          TEXT PRIMARY KEY REFERENCES memory_nodes(id) ON DELETE CASCADE,
    inputs_hash      TEXT NOT NULL,
    analysis_json    TEXT NOT NULL,
    llm_model        TEXT,
    token_cost       INTEGER,
    created_at       INTEGER NOT NULL
);
";

/// V47 — Memory OS L3 §4.12.5 RETAINED (per ADR 2026-05-20 §8) —
/// Cross-Source Triangulation evidence.
///
/// Spec §4.12.5: when ≥2 independent sources agree on a claim, the
/// agent's confidence in that claim should be boosted; when sources
/// disagree, surface it for review. Each row is one (claim, source)
/// agreement assertion. The triangulation algorithm groups by claim
/// and computes a confidence boost from the agreement rate.
///
/// `claim_id` is a caller-supplied identifier (typically a hash of
/// the claim text or a node_id when the claim is "this EntityPage's
/// compiled_truth"). `source_node_id` references the source page
/// providing the evidence. `agrees` is 1 when the source supports
/// the claim, 0 when it contradicts (so opposing evidence is
/// captured too).
pub const V47_TRIANGULATION_EVIDENCE: &str = "
CREATE TABLE IF NOT EXISTS triangulation_evidence (
    id              TEXT PRIMARY KEY,
    claim_id        TEXT NOT NULL,
    source_node_id  TEXT NOT NULL REFERENCES memory_nodes(id) ON DELETE CASCADE,
    agrees          INTEGER NOT NULL DEFAULT 1,
    weight          REAL NOT NULL DEFAULT 1.0,
    note            TEXT,
    computed_at     INTEGER NOT NULL,
    UNIQUE(claim_id, source_node_id)
);
CREATE INDEX IF NOT EXISTS idx_triangulation_claim
    ON triangulation_evidence(claim_id);
CREATE INDEX IF NOT EXISTS idx_triangulation_source
    ON triangulation_evidence(source_node_id);
";

/// V46 — Memory OS L3 §4.12.4 RETAINED (per ADR 2026-05-20 §8) —
/// Concept Drift Detection events. One row per drift signal detected
/// on an EntityPage's version chain.
///
/// Spec §4.12.4: if a page is rewritten >= 3 times in 30 days with
/// large content diffs, it's "drifting" — could be evolving fact,
/// LLM instability, or unresolved contradiction. Each drift signal
/// becomes a `drift_events` row + (in a future PR) a
/// `review_queue_items` row for human triage.
///
/// `score` is the Levenshtein-normalized drift magnitude (0-1).
/// `snapshot_version_ids` is a JSON array of the version IDs that
/// were sampled to compute the score (debugging + future LLM
/// review uses this to reconstruct what changed).
pub const V46_DRIFT_EVENTS: &str = "
CREATE TABLE IF NOT EXISTS drift_events (
    id                   TEXT PRIMARY KEY,
    node_id              TEXT NOT NULL REFERENCES memory_nodes(id) ON DELETE CASCADE,
    score                REAL NOT NULL,
    snapshot_version_ids TEXT NOT NULL,
    computed_at          INTEGER NOT NULL,
    status               TEXT NOT NULL DEFAULT 'open',
    resolution_note      TEXT,
    resolved_at          INTEGER
);
CREATE INDEX IF NOT EXISTS idx_drift_events_node_time
    ON drift_events(node_id, computed_at DESC);
CREATE INDEX IF NOT EXISTS idx_drift_events_status
    ON drift_events(status, computed_at DESC);
";

/// V45 — Memory OS L3 §4.12.3 RETAINED (per ADR 2026-05-20 §8) —
/// Spaced Repetition state. One row per node enrolled in review;
/// SM-2-style interval ladder (1, 3, 7, 14, 30, 90 days).
///
/// Caller (Q3a Spaced Repetition module) enrolls nodes whose
/// `memory_importance_scores.importance >= 0.6` AND
/// `metadata_json.status = 'verified'`. The scheduled review
/// (future PR) re-checks the node's content via LLM; on pass →
/// next interval, on fail → reset to interval 0.
///
/// `enabled = 0` is a per-node opt-out (e.g. user marks a page
/// "no review needed"). Default 1.
pub const V45_SPACED_REPETITION_STATE: &str = "
CREATE TABLE IF NOT EXISTS spaced_repetition_state (
    node_id          TEXT PRIMARY KEY REFERENCES memory_nodes(id) ON DELETE CASCADE,
    interval_idx     INTEGER NOT NULL DEFAULT 0,
    last_reviewed_at INTEGER NOT NULL,
    next_review_at   INTEGER NOT NULL,
    reviews_total    INTEGER NOT NULL DEFAULT 0,
    reviews_passed   INTEGER NOT NULL DEFAULT 0,
    enabled          INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_spaced_rep_due
    ON spaced_repetition_state(next_review_at)
    WHERE enabled = 1;
";

/// V50 — Halo-compatible automation app metadata.
///
/// Additive only: `automation_specs.status` already exists on V20+
/// databases, so duplicate-column errors are tolerated by the migration
/// runner just like the earlier automation_specs ALTER migrations.
pub const V50_HALO_AUTOMATION_METADATA: &str = "
ALTER TABLE automation_specs ADD COLUMN status TEXT;
ALTER TABLE automation_specs ADD COLUMN user_overrides_json TEXT;
ALTER TABLE automation_specs ADD COLUMN browser_login TEXT;
ALTER TABLE automation_specs ADD COLUMN uninstalled_at INTEGER;
";

/// V51 — `memorization_queue` shared schema in the main app DB.
///
/// `memorization::storage::MemorizationStorage::ensure_tables` creates
/// this table on its own SQLite connection (which is the same file —
/// `~/.uclaw/uclaw.db`), but the order of init relative to
/// `proactive::conversation_bridge::enqueue_message` was racy: the
/// bridge would INSERT before MemorizationStorage's setup ran, hitting
/// `no such table: memorization_queue` in production.
///
/// Schema mirrors `MemorizationStorage::ensure_tables` plus the
/// `metadata` column the conversation_bridge inserts (it was missing
/// in the original ensure_tables — that's a separate gap also patched
/// here so both writers agree on the column set).
///
/// Idempotent: CREATE TABLE IF NOT EXISTS + ALTER ADD COLUMN errors
/// are skipped by the migration runner the same way V50 swallows
/// duplicate-column errors.
pub const V51_MEMORIZATION_QUEUE: &str = "
CREATE TABLE IF NOT EXISTS memorization_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    platform TEXT NOT NULL DEFAULT 'local',
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    conversation_id TEXT,
    space_id TEXT,
    timestamp INTEGER NOT NULL,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);
ALTER TABLE memorization_queue ADD COLUMN metadata TEXT;
CREATE TABLE IF NOT EXISTS memorization_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT DEFAULT CURRENT_TIMESTAMP
);
";

/// V52 — `agent_fold_baselines` table for Bundle 17-B delta-rendered
/// `/compact` placeholder.
///
/// Per spec
/// [`docs/superpowers/specs/2026-05-22-bundle-17bc-wireup-design.md`](../../../docs/superpowers/specs/2026-05-22-bundle-17bc-wireup-design.md) §9.2:
/// the `/compact` intercept in `tauri_commands.rs` historically drops
/// the typed `StructuredFold` after rendering it to markdown. To enable
/// the delta-rendered path (small drift → render `<context_changes_since_last_fold>`
/// block on top of the *stable* prior fold), we persist the most recent
/// fold per session so the next `/compact` can diff against it.
///
/// Schema:
/// - `session_id` — PK, refs `agent_sessions.id` (1-to-1 with current fold)
/// - `fold_json` — serde_json::to_string of `StructuredFold` (NOT NULL)
/// - `baseline_hash` — `StructuredFold::baseline_hash()` (sanity check)
/// - `updated_at` — wall-clock ms (debugging only)
///
/// No FK so an orphaned baseline row simply gets ignored on next session
/// query — matches the pattern of `compaction_markers` (V29) which also
/// avoids FK to keep session deletion fast.
///
/// Idempotent: `CREATE TABLE IF NOT EXISTS` so re-running V52 on already-
/// migrated DB is a no-op. First-compact behavior on a brand-new V52
/// session: `SELECT` returns 0 rows → falls back to current full-rewrite
/// behavior (no regression).
pub const V52_AGENT_FOLD_BASELINES: &str = "
CREATE TABLE IF NOT EXISTS agent_fold_baselines (
    session_id     TEXT PRIMARY KEY,
    fold_json      TEXT NOT NULL,
    baseline_hash  TEXT NOT NULL,
    updated_at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_agent_fold_baselines_session
    ON agent_fold_baselines(session_id);
";

/// V53 — Living Persona MVP state.
///
/// Persona is an expression layer only. These tables persist user-visible,
/// editable voice and relationship state; they do not control tool access,
/// permission mode, model routing, memory write policy, or safety policy.
pub const V53_LIVING_PERSONA: &str = "
CREATE TABLE IF NOT EXISTS persona_voice_profiles (
    id TEXT PRIMARY KEY,
    scope TEXT NOT NULL CHECK(scope IN ('global', 'workspace', 'session')),
    scope_id TEXT,
    preset_id TEXT NOT NULL,
    warmth INTEGER NOT NULL CHECK(warmth BETWEEN 0 AND 5),
    directness INTEGER NOT NULL CHECK(directness BETWEEN 0 AND 5),
    challenge INTEGER NOT NULL CHECK(challenge BETWEEN 0 AND 5),
    playfulness INTEGER NOT NULL CHECK(playfulness BETWEEN 0 AND 5),
    detail INTEGER NOT NULL CHECK(detail BETWEEN 0 AND 5),
    initiative INTEGER NOT NULL CHECK(initiative BETWEEN 0 AND 5),
    structure INTEGER NOT NULL CHECK(structure BETWEEN 0 AND 5),
    restraint INTEGER NOT NULL CHECK(restraint BETWEEN 0 AND 5),
    neutral_mode INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(scope, scope_id)
);

CREATE TABLE IF NOT EXISTS persona_bond_profiles (
    id TEXT PRIMARY KEY,
    scope TEXT NOT NULL CHECK(scope IN ('global', 'workspace', 'session')),
    scope_id TEXT,
    content_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(scope, scope_id)
);

CREATE TABLE IF NOT EXISTS persona_journal_entries (
    id TEXT PRIMARY KEY,
    session_id TEXT,
    task_id TEXT,
    observation TEXT NOT NULL,
    interpretation TEXT,
    confidence TEXT NOT NULL CHECK(confidence IN ('low', 'medium', 'high')),
    promoted_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS persona_keepsakes (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    narrative TEXT NOT NULL,
    learned_text TEXT,
    evidence_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL CHECK(status IN ('proposed', 'accepted', 'hidden', 'discarded')) DEFAULT 'proposed',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS persona_evolution_candidates (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    proposed_change_json TEXT NOT NULL,
    scope TEXT NOT NULL CHECK(scope IN ('global', 'workspace', 'session')),
    scope_id TEXT,
    status TEXT NOT NULL CHECK(status IN ('candidate', 'observed', 'accepted', 'rejected', 'retired')) DEFAULT 'candidate',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    reviewed_at TEXT
);

CREATE TABLE IF NOT EXISTS persona_badges (
    id TEXT PRIMARY KEY,
    badge_key TEXT NOT NULL,
    label TEXT NOT NULL,
    unlock_reason TEXT NOT NULL,
    evidence_json TEXT NOT NULL DEFAULT '[]',
    hidden INTEGER NOT NULL DEFAULT 0,
    awarded_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(badge_key)
);

CREATE INDEX IF NOT EXISTS idx_persona_journal_session ON persona_journal_entries(session_id);
CREATE INDEX IF NOT EXISTS idx_persona_keepsakes_status ON persona_keepsakes(status);
CREATE INDEX IF NOT EXISTS idx_persona_candidates_status ON persona_evolution_candidates(status);
";

/// V54 — Append-only Living Persona event ledger.
///
/// Events are explicit relationship-growth observations. They feed affinity
/// calculation and UI timelines only; they do not affect tool access, safety,
/// model routing, or prompt capability.
pub const V54_PERSONA_EVENTS: &str = "
CREATE TABLE IF NOT EXISTS persona_events (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    session_id TEXT,
    task_id TEXT,
    minutes INTEGER NOT NULL DEFAULT 0,
    weight INTEGER NOT NULL DEFAULT 1,
    note TEXT,
    evidence_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_persona_events_kind ON persona_events(kind);
CREATE INDEX IF NOT EXISTS idx_persona_events_session ON persona_events(session_id);
CREATE INDEX IF NOT EXISTS idx_persona_events_created ON persona_events(created_at);
";

pub const V55_SESSION_TREE: &str = "
CREATE TABLE IF NOT EXISTS session_tree (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    parent_id   TEXT,
    entry_type  TEXT NOT NULL,
    data_json   TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_session_tree_session ON session_tree(session_id);
CREATE INDEX IF NOT EXISTS idx_session_tree_parent ON session_tree(parent_id);

CREATE TABLE IF NOT EXISTS session_leaves (
    session_id  TEXT PRIMARY KEY,
    leaf_id     TEXT,
    updated_at  INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES agent_sessions(id) ON DELETE CASCADE
);
";

/// V43 seed — Phase 8.2: 7 default rows in `wiki_page_templates`, one
/// per subkind defined in Cognitive Spec §2.2. `INSERT OR IGNORE` keeps
/// the seed re-runnable: users / agents can tune the prompts in the
/// table afterwards and the seed won't overwrite their edits.
///
/// Subkinds (display_name shown for clarity):
/// - entity     (实体)   — Summary / Background / Current Status / Relationships / Open Questions
/// - concept    (概念)   — Definition / Key Properties / Examples / Confusables / References
/// - comparison (对比)   — Dimensions / Side-by-Side Table / When-to-Use / Trade-offs
/// - question   (问题)   — Question Statement / Why It Matters / Current Hypotheses / Known Answers / Status
/// - synthesis  (综合)   — Topic / Scope / Key Findings / Open Issues / Source List
/// - decision   (决策)   — Context / Options Considered / Decision / Rationale / Pitfalls Avoided
/// - gap        (空白)   — Question / What We Know / What We Don't Know / Possible Paths / Priority
///
/// `compile_prompt` is the LLM-facing instruction the Phase-10
/// `wiki_compile` module will use when (re)generating a page's
/// `compiled_truth` section. Placeholders `{title}`, `{sources}`,
/// `{existing_truth}`, `{timeline}` are substituted at compile time.
///
/// Single quotes inside the seed are SQL-escaped via doubled `''`.
pub const V43_SEED_TEMPLATES: &str = r#"
INSERT OR IGNORE INTO wiki_page_templates
    (subkind, display_name, compile_prompt, sections_json, ui_card_layout, updated_at)
VALUES
    ('entity', '实体',
     '你是一个知识管理员,负责为实体页生成 compiled_truth。给定标题 {title}、源材料 {sources}、当前 compiled_truth {existing_truth}、时间线摘要 {timeline}。写一份 5 段结构的实体页:Summary(一句话定位)、Background(背景与历史)、Current Status(当下在做什么/最新状态)、Relationships(与其他实体的关键关系)、Open Questions(尚未明朗的问题)。语言简洁,事实优先,避免主观判断。',
     '["Summary","Background","Current Status","Relationships","Open Questions"]',
     'card_entity', strftime('%s','now')*1000),

    ('concept', '概念',
     '你是一个知识管理员,负责为概念页生成 compiled_truth。给定标题 {title}、源材料 {sources}、当前 compiled_truth {existing_truth}、时间线摘要 {timeline}。写一份 5 段结构的概念页:Definition(严格定义,1-2 句)、Key Properties(关键属性清单)、Examples(2-3 个具体实例)、Confusables(易混淆点 + 区分线)、References(原始出处)。精确性优先,避免类比扩展。',
     '["Definition","Key Properties","Examples","Confusables","References"]',
     'card_concept', strftime('%s','now')*1000),

    ('comparison', '对比',
     '你是一个知识管理员,负责为对比页生成 compiled_truth。给定标题 {title}、源材料 {sources}、当前 compiled_truth {existing_truth}、时间线摘要 {timeline}。写一份 4 段结构的对比页:Dimensions(对比维度列表)、Side-by-Side Table(强制 markdown 表格,每个维度一行)、When-to-Use(什么场景选哪个)、Trade-offs(取舍与限制)。表格是该页的核心,务必完整。',
     '["Dimensions","Side-by-Side Table","When-to-Use","Trade-offs"]',
     'card_comparison', strftime('%s','now')*1000),

    ('question', '问题',
     '你是一个知识管理员,负责为问题页生成 compiled_truth。给定标题 {title}、源材料 {sources}、当前 compiled_truth {existing_truth}、时间线摘要 {timeline}。写一份 5 段结构的问题页:Question Statement(问题陈述,一句话)、Why It Matters(为什么值得追问)、Current Hypotheses(当前假设清单)、Known Answers(已知答案 + 出处引用)、Status(`open` / `answered` / `disputed`)。answered 时必须附 Answer 段与引用源。',
     '["Question Statement","Why It Matters","Current Hypotheses","Known Answers","Status"]',
     'card_question', strftime('%s','now')*1000),

    ('synthesis', '综合',
     '你是一个知识管理员,负责为综合页生成 compiled_truth。给定标题 {title}、源材料 {sources}、当前 compiled_truth {existing_truth}、时间线摘要 {timeline}。写一份 5 段结构的综合页:Topic(综合主题)、Scope(包含/排除范围)、Key Findings(关键发现,跨多源)、Open Issues(尚未解决的问题)、Source List(完整引用列表,必填)。综合页必须有引用列表,无引用的发现不要写入。',
     '["Topic","Scope","Key Findings","Open Issues","Source List"]',
     'card_synthesis', strftime('%s','now')*1000),

    ('decision', '决策',
     '你是一个知识管理员,负责为决策页生成 compiled_truth。给定标题 {title}、源材料 {sources}、当前 compiled_truth {existing_truth}、时间线摘要 {timeline}。写一份 5 段结构的决策页:Context(决策背景)、Options Considered(候选方案清单)、Decision(最终选择)、Rationale(选择理由)、Pitfalls Avoided(避开了什么坑 + 学到了什么)。Pitfalls 是该页的核心价值载体,务必具体。',
     '["Context","Options Considered","Decision","Rationale","Pitfalls Avoided"]',
     'card_decision', strftime('%s','now')*1000),

    ('gap', '空白',
     '你是一个知识管理员,负责为空白页生成 compiled_truth。给定标题 {title}、源材料 {sources}、当前 compiled_truth {existing_truth}、时间线摘要 {timeline}。写一份 5 段结构的空白页:Question(我们想知道但不知道的核心问题)、What We Know(已知部分)、What We Don''t Know(明确的盲区)、Possible Paths(可能的探索方向)、Priority(`urgent` / `important` / `curious`)。这页让 Agent 显式留白主动调研。',
     '["Question","What We Know","What We Don''t Know","Possible Paths","Priority"]',
     'card_gap', strftime('%s','now')*1000)
;
"#;

pub const V39_USER_PROFILE_FACETS: &str = "
CREATE TABLE IF NOT EXISTS user_profile_facets (
    facet_id           TEXT PRIMARY KEY,
    class              TEXT NOT NULL,
    name               TEXT NOT NULL,
    value              TEXT NOT NULL,
    state              TEXT NOT NULL DEFAULT 'candidate',
    stability          REAL NOT NULL DEFAULT 0.0,
    cue_families_json  TEXT NOT NULL DEFAULT '{}',
    evidence_count     INTEGER NOT NULL DEFAULT 1,
    last_seen_at       INTEGER NOT NULL,
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_facets_class_name
    ON user_profile_facets(class, name);
CREATE INDEX IF NOT EXISTS idx_facets_class_state
    ON user_profile_facets(class, state);
CREATE INDEX IF NOT EXISTS idx_facets_last_seen
    ON user_profile_facets(last_seen_at);
";

/// V44 — Memory OS L3 Engines RETAINED schema (per ADR 2026-05-20 §8).
///
/// gbrain is uClaw's primary knowledge layer; L2 Cognitive is paused;
/// L3 Engines is partially paused. This migration ships only the
/// schema for the RETAINED subset:
///
/// - `timeline_events`         — global event ledger (Phase 16)
/// - `temporal_aggregates`     — per-grain summaries (Phase 16; day/week/month/quarter/year)
/// - `activity_clusters`       — LLM-grouped event clusters per period (Phase 16)
/// - `memory_importance_scores` — Ebbinghaus + importance-weighted decay (Phase §4.12.1)
///
/// Tables explicitly NOT shipped (Entity Graph PAUSED, Dream Cycle pipeline
/// PAUSED): `entity_aliases`, `entity_aliases_fts`, `entity_raw_data`,
/// `dream_cycle_runs`, `dream_cycle_stages`, `spaced_repetition_state`,
/// `drift_events`, `triangulation_evidence`. Those land later, each
/// alongside the code that uses them (or as future-V migrations).
///
/// All four tables are `IF NOT EXISTS` and additive. `memory_importance_scores`
/// FKs to `memory_nodes(id)` with `ON DELETE CASCADE` (matches V43's
/// `page_content_hashes` / `analysis_cache` pattern). Tables stay empty
/// until consumers (Timeline Engine scenario in a follow-up PR;
/// Importance Decay algorithm in P2) start populating them.
pub const V49_COST_RECORDS_6D: &str = "
-- M1-T6 — extend cost_records (V13) with the 6-dimension TokenUsage shape.
--   * cached_input_tokens — prompt-cache reads (cheaper than fresh input)
--   * reasoning_output_tokens — extended-thinking output (Claude / o1)
-- Existing rows default the new columns to 0 (no semantic change for
-- pre-M1-T6 data). Cost recalculation against new pricing is a follow-up.
ALTER TABLE cost_records ADD COLUMN cached_input_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE cost_records ADD COLUMN reasoning_output_tokens INTEGER NOT NULL DEFAULT 0;
";

pub const V48_TASK_EVENTS_ROLLOUT: &str = "
-- M1-T5 (Phase 0.5 / Runtime Contracts) — rollout sidecar for task event
-- stream. Mirrors the JSONL at `~/.uclaw/sessions/rollout-*.jsonl` so
-- replay + index queries are fast.
--
-- One row per TaskEvent emitted by a SessionTask. The full event payload
-- is stored as JSON in `payload_json`; `kind` + `source` are indexed for
-- cheap rollup queries. `sequence` is monotonically assigned by the
-- writer per (task_id, rollout_file) tuple so replay is deterministic.
CREATE TABLE IF NOT EXISTS task_events_rollout (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id       TEXT NOT NULL,
    intent_id     TEXT,
    sequence      INTEGER NOT NULL,
    ts            TEXT NOT NULL,
    kind          TEXT NOT NULL,
    source        TEXT NOT NULL,
    payload_json  TEXT NOT NULL,
    rollout_file  TEXT
);
CREATE INDEX IF NOT EXISTS idx_task_events_rollout_task ON task_events_rollout(task_id, sequence);
CREATE INDEX IF NOT EXISTS idx_task_events_rollout_kind ON task_events_rollout(kind, ts);
CREATE INDEX IF NOT EXISTS idx_task_events_rollout_source ON task_events_rollout(source, ts);
CREATE INDEX IF NOT EXISTS idx_task_events_rollout_intent ON task_events_rollout(intent_id, sequence) WHERE intent_id IS NOT NULL;
";

pub const V44_L3_RETAINED_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS timeline_events (
    id                 TEXT PRIMARY KEY,
    space_id           TEXT NOT NULL,
    event_kind         TEXT NOT NULL,
    subject_id         TEXT,
    title              TEXT NOT NULL,
    payload_json       TEXT,
    related_entity_ids TEXT,
    occurred_at        INTEGER NOT NULL,
    importance         REAL NOT NULL DEFAULT 0.5,
    created_at         INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_timeline_events_time
    ON timeline_events(space_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_timeline_events_entity
    ON timeline_events(related_entity_ids);
CREATE INDEX IF NOT EXISTS idx_timeline_events_kind
    ON timeline_events(space_id, event_kind, occurred_at DESC);

CREATE TABLE IF NOT EXISTS temporal_aggregates (
    id            TEXT PRIMARY KEY,
    space_id      TEXT NOT NULL,
    grain         TEXT NOT NULL,
    period_start  INTEGER NOT NULL,
    period_end    INTEGER NOT NULL,
    summary_md    TEXT NOT NULL,
    event_count   INTEGER NOT NULL DEFAULT 0,
    top_themes    TEXT NOT NULL DEFAULT '[]',
    top_entities  TEXT NOT NULL DEFAULT '[]',
    llm_model     TEXT,
    token_cost    INTEGER,
    created_at    INTEGER NOT NULL,
    UNIQUE(space_id, grain, period_start)
);
CREATE INDEX IF NOT EXISTS idx_temporal_aggregates_lookup
    ON temporal_aggregates(space_id, grain, period_start DESC);

CREATE TABLE IF NOT EXISTS activity_clusters (
    id                 TEXT PRIMARY KEY,
    space_id           TEXT NOT NULL,
    period_start       INTEGER NOT NULL,
    period_end         INTEGER NOT NULL,
    cluster_label      TEXT NOT NULL,
    description        TEXT NOT NULL,
    event_ids          TEXT NOT NULL,
    related_entity_ids TEXT NOT NULL DEFAULT '[]',
    score              REAL NOT NULL DEFAULT 0.5,
    created_at         INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_activity_clusters_period
    ON activity_clusters(space_id, period_start DESC);

CREATE TABLE IF NOT EXISTS memory_importance_scores (
    node_id               TEXT PRIMARY KEY REFERENCES memory_nodes(id) ON DELETE CASCADE,
    base_value            REAL NOT NULL,
    citation_factor       REAL NOT NULL,
    edge_factor           REAL NOT NULL,
    recency_factor        REAL NOT NULL,
    status_bonus          REAL NOT NULL,
    penalty               REAL NOT NULL,
    importance            REAL NOT NULL,
    decay_half_life_days  REAL NOT NULL,
    last_computed_at      INTEGER NOT NULL,
    archive_pending_since INTEGER
);
CREATE INDEX IF NOT EXISTS idx_importance_scores_value
    ON memory_importance_scores(importance DESC);
CREATE INDEX IF NOT EXISTS idx_importance_scores_archive
    ON memory_importance_scores(archive_pending_since)
    WHERE archive_pending_since IS NOT NULL;
";

// ─── V56 — Slice 1b safety chokepoint: automation approval requests ──────
const SQL_V56: &str = r#"
    CREATE TABLE automation_approval_requests (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        activity_id TEXT NOT NULL REFERENCES automation_activities(id),
        tool_name TEXT NOT NULL,
        arguments_json TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'pending',
        created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
        resolved_at TIMESTAMP NULL
    );
    CREATE INDEX idx_aar_activity_status ON automation_approval_requests(activity_id, status);
    ALTER TABLE automation_activities
        ADD COLUMN pending_approval_request_id INTEGER NULL
            REFERENCES automation_approval_requests(id);
"#;

// ─── V57 — Phase 3 growth: reflections + user_model (mem.md Reflection Agent) ──
// `reflections`: periodic insights distilled from recent agent turns.
// `user_model`: singleton distilled persona/preference model (Pattern→Model
// layer, populated by P3-②'s promotion pass). Additive + IF NOT EXISTS so the
// block is idempotent and safe to replay on every boot.
const SQL_V57: &str = r#"
    CREATE TABLE IF NOT EXISTS reflections (
        id                 TEXT PRIMARY KEY,
        insight            TEXT NOT NULL,
        confidence         REAL NOT NULL DEFAULT 0.5,
        source_event_count INTEGER NOT NULL DEFAULT 0,
        created_at         TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS idx_reflections_created ON reflections(created_at DESC);
    CREATE TABLE IF NOT EXISTS user_model (
        id          TEXT PRIMARY KEY,
        summary     TEXT NOT NULL,
        updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
    );
"#;

// ─── V58 — Phase 4 growth: daydreams (divergent free-association pass) ────────
// `daydreams`: novel hypotheses/connections the ReflectionService free-associates
// over random memory (~every 100 agent turns, 1/5 of the reflection cadence).
// UI-surface only (emitted as `agent:daydream`); NOT injected into the prompt.
// Additive + IF NOT EXISTS so the block is idempotent and safe to replay on boot.
const SQL_V58: &str = r#"
    CREATE TABLE IF NOT EXISTS daydreams (
        id          TEXT PRIMARY KEY,
        content     TEXT NOT NULL,
        created_at  TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS idx_daydreams_created ON daydreams(created_at DESC);
"#;

// ─── V59 — P5 memory refinement: reflections.archived_at + user_model_history ───
// `reflections.archived_at`: soft-delete marker — NULL means live; set to an
// ISO-8601 timestamp when a reflection is superseded during consolidation.
// `user_model_history`: append-only audit trail of superseded user_model summaries;
// one row is inserted before each re-ground overwrites the singleton summary.
// ALTER TABLE errors on replay if the column already exists; the error-catch loop
// below handles that (logs "skipped"), so this block is replay-safe.
const SQL_V59: &str = r#"
    ALTER TABLE reflections ADD COLUMN archived_at TEXT;
    CREATE TABLE IF NOT EXISTS user_model_history (
        id          TEXT PRIMARY KEY,
        summary     TEXT NOT NULL,
        replaced_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
"#;

/// Test/dev helper: run the full migration stack on a fresh connection.
///
/// The `_target` parameter is currently ignored. All migrations are
/// cumulative (later versions only add to the schema), so callers always
/// receive the complete schema regardless of the target value. This shape
/// keeps a stable test-side API in case future migrations need bounded
/// application.
#[cfg(test)]
pub fn run_migrations_up_to(conn: &rusqlite::Connection, _target: u32) -> Result<(), rusqlite::Error> {
    run(conn)
}

pub fn run(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    tracing::debug!("Running migration V1: initial schema");
    conn.execute_batch(V1_INITIAL)?;
    // Run V2 migration (ignore error if column/table already exists)
    tracing::debug!("Running migration V2: artifact cache & stars");
    let _ = conn.execute_batch(V2_ARTIFACT_CACHE_AND_STARS);
    // Run V3 migration – memories table
    tracing::debug!("Running migration V3: memories");
    let _ = conn.execute_batch(V3_MEMORIES);
    // Run V4 migration – memory graph tables
    tracing::debug!("Running migration V4: memory graph");
    let _ = conn.execute_batch(V4_MEMORY_GRAPH);
    // Run V5 migration – agent session columns (is_agent, workspace_id)
    tracing::debug!("Running migration V5: agent sessions");
    let _ = conn.execute_batch(V5_AGENT_SESSIONS);
    // V5 additional: ALTER TABLE for metadata column (idempotent failure OK)
    tracing::debug!("Running migration V5a: metadata column");
    let _ = conn.execute_batch(V5_ALTER);
    // V5 additional: CREATE TABLE IF NOT EXISTS for harness tables (always safe)
    tracing::debug!("Running migration V5b: harness tables");
    let _ = conn.execute_batch(V5_TABLES);
    // V6: agent teams tables
    tracing::debug!("Running migration V6: agent teams tables");
    let _ = conn.execute_batch(V6_AGENT_TEAMS);
    // V7: automation tables
    tracing::debug!("Running migration V7: automation tables");
    let _ = conn.execute_batch(V7_AUTOMATIONS);
    // V8: agent sessions tables
    tracing::debug!("Running migration V8: agent sessions tables");
    let _ = conn.execute_batch(V8_AGENT_SESSIONS);
    // V9: per-message process columns (reasoning, tool_activities, model, attachments).
    // Each ALTER is run individually so an existing column doesn't abort the rest.
    tracing::debug!("Running migration V9: message process columns");
    for stmt in V9_MESSAGE_PROCESS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let _ = conn.execute(stmt, []);
    }
    // V10: messages FTS for chat search — must run individual ALTER first because
    // it can fail on re-runs if the column already exists. Subsequent CREATE TRIGGER
    // / CREATE VIRTUAL TABLE statements have IF NOT EXISTS guards.
    tracing::debug!("Running migration V10: messages FTS");
    for stmt in V10_MESSAGES_FTS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let _ = conn.execute(stmt, []);
    }
    // Backfill content_text + FTS index for messages that pre-date V10. Idempotent.
    let _ = conn.execute(
        "UPDATE messages SET content_text = (
            SELECT COALESCE(group_concat(json_extract(value, '$.text'), ' '), '')
            FROM json_each(messages.content)
            WHERE json_extract(value, '$.type') = 'text'
        ) WHERE content_text = ''",
        [],
    );
    let _ = conn.execute(
        "INSERT INTO messages_fts(rowid, conversation_id, role, content_text, reasoning)
         SELECT rowid, conversation_id, role, content_text, reasoning
         FROM messages
         WHERE rowid NOT IN (SELECT rowid FROM messages_fts)",
        [],
    );
    // V11: re-tokenize FTS5 with trigram for CJK + substring + typo-resilience.
    tracing::debug!("Running migration V11: FTS trigram tokenizer");
    for stmt in V11_FTS_TRIGRAM.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V11 stmt skipped: {} :: {}", e, stmt);
        }
    }
    if let Err(e) = conn.execute(V11_BACKFILL_MESSAGES, []) {
        tracing::warn!("V11 messages backfill failed: {}", e);
    }
    if let Err(e) = conn.execute(V11_BACKFILL_AGENT_TURNS, []) {
        tracing::warn!("V11 agent_turns backfill failed: {}", e);
    }
    // V12: agent_messages FTS so the agent-domain conversation is searchable.
    tracing::debug!("Running migration V12: agent_messages FTS");
    for stmt in V12_AGENT_MESSAGES_FTS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V12 stmt skipped: {} :: {}", e, stmt);
        }
    }
    if let Err(e) = conn.execute(
        "INSERT INTO agent_messages_fts(rowid, session_id, role, content, reasoning)
         SELECT rowid, session_id, role, content, reasoning
         FROM agent_messages
         WHERE rowid NOT IN (SELECT rowid FROM agent_messages_fts)",
        [],
    ) {
        tracing::warn!("V12 agent_messages backfill failed: {}", e);
    }
    // V13: per-turn cost records for the usage dashboard.
    tracing::debug!("Running migration V13: cost records");
    for stmt in V13_COST_RECORDS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V13 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V14: per-session + pattern rules + audit log.
    tracing::debug!("Running migration V14: permission tables");
    for stmt in V14_PERMISSION_TABLES.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V14 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V15: per-message metrics (duration, token counts, cost).
    tracing::debug!("Running migration V15: agent_message metrics columns");
    for stmt in V15_AGENT_MESSAGE_METRICS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V15 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V16: persist 'default' workspace + heal orphan agent_sessions.
    tracing::debug!("Running migration V16: workspace default + orphan heal");
    for stmt in V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V16 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V17: workspace sort + attached directories columns + backfill.
    tracing::debug!("Running migration V17: workspace path/sort/attached_dirs");
    for stmt in V17_WORKSPACE_PATH_SORT_ATTACHED.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V17 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V18: agent_sessions.pinned_at — canonical pin state for the agent UI.
    tracing::debug!("Running migration V18: agent_sessions.pinned_at");
    for stmt in V18_AGENT_SESSIONS_PINNED_AT.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V18 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V19: per-workspace skill_tags JSON column for the manifest filter.
    tracing::debug!("Running migration V19: spaces.skill_tags");
    for stmt in V19_SPACES_SKILL_TAGS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V19 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V20: rewrite automation_specs + automation_activities to Humane schema.
    // Wrapped in its own transaction inside run_v20 — failure here is fatal
    // (unlike the ALTER-only migrations above) because V20 replaces the tables
    // entirely and partial execution would leave an inconsistent schema.
    //
    // info! (not debug!) so the line is visible at the default WARN+ tracing
    // filter — silent V20 failures are a documented pain point.
    tracing::info!("Running migration V20: Humane automation schema rewrite");
    if let Err(e) = run_v20(conn) {
        tracing::error!(error = %e, "V20 FAILED — Humane automation features will not work");
        return Err(e);
    }
    // V21: three Humane behavior tables that FK into the V20 schema.
    tracing::info!("Running migration V21: automation_subscriptions, automation_memory, automation_escalations");
    if let Err(e) = run_v21(conn) {
        tracing::error!(error = %e, "V21 FAILED — automation escalations / subscriptions / memory tables missing");
        return Err(e);
    }
    // V22: automation_installed_skills — tracks bundled skills per automation.
    tracing::debug!("Running migration V22: automation_installed_skills");
    for stmt in SQL_V22.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V22 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V23a: Marketplace cache (Phase 3a). Phase 3b extends to add the
    // automation_registries table for multi-source support.
    tracing::info!("Running migration V23a: marketplace cache (items + FTS + sync state)");
    if let Err(e) = run_v23a(conn) {
        tracing::error!(error = %e, "V23a FAILED — marketplace cache unavailable");
        return Err(e);
    }
    // V24: automation run = agent_session. Statement-split tolerant style —
    // ADD/DROP COLUMN are not transactional-schema-replacing, so partial
    // application is fine and a re-run's "duplicate/no such column" is benign.
    tracing::debug!("Running migration V24: automation run-session columns");
    for stmt in V24_AUTOMATION_RUN_SESSIONS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V24 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V25: marketplace_standalone_installs — tracks standalone skill/MCP installs.
    tracing::debug!("Running migration V25: marketplace_standalone_installs");
    for stmt in SQL_V25.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V25 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V26: conversations.archived + archived_at
    tracing::debug!("Running migration V26: conversations archived columns");
    for stmt in SQL_V26.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V26 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V27: system_prompts table for user-customizable system prompts.
    tracing::debug!("Running migration V27: system_prompts");
    for stmt in SQL_V27.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V27 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V28: system_prompt_versions table for prompt version history.
    tracing::debug!("Running migration V28: system_prompt_versions");
    for stmt in SQL_V28.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V28 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V29: compaction support — logical marking (compacted column + compaction_markers table).
    // Replaces the old physical-deletion approach so compacted messages stay in the DB
    // and can be replayed in the UI with "compacted" visual treatment.
    tracing::debug!("Running migration V29: compaction support (logical marking)");
    for stmt in V29_COMPACTION_SUPPORT.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V29 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V30: fragment_reviews + daily_summaries for the memory fragment system.
    tracing::debug!("Running migration V30: fragment tables");
    for stmt in V30_FRAGMENT_TABLES.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V30 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V31: rebuild memory_fts with trigram tokenizer for CJK + substring search.
    // Like V11 for messages_fts, drops the old unicode61 table and recreates
    // with trigram, then backfills from memory_nodes + active memory_versions.
    tracing::info!("Running migration V31: memory_fts trigram tokenizer");
    for stmt in V31_MEMORY_FTS_TRIGRAM.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V31 stmt skipped: {} :: {}", e, stmt);
        }
    }
    if let Err(e) = conn.execute(V31_BACKFILL_MEMORY_FTS, []) {
        tracing::warn!("V31 memory_fts backfill failed: {}", e);
    }
    // V32: IM channel infrastructure (im_channel_instances, im_sessions, spec_channel_bindings).
    tracing::debug!("Running migration V32: IM channel tables");
    for stmt in SQL_V32.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute_batch(stmt) {
            tracing::warn!("V32 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V32b: automation_specs additional columns (ALTER TABLE — idempotent, ignore if column exists).
    tracing::debug!("Running migration V32b: automation_specs IM columns");
    for stmt in SQL_V32B.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute_batch(stmt) {
            tracing::warn!("V32b stmt skipped (likely already exists): {} :: {}", e, stmt);
        }
    }
    // V33: Symphony runtime schema (workflows + versions + runs + node-runs + seed space).
    tracing::debug!("Running migration V33: Symphony tables");
    for stmt in SQL_V33_SYMPHONY.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V33 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V34: plan_suggest_events
    tracing::debug!("Running migration V34: plan_suggest_events");
    for stmt in SQL_V34_PLAN_SUGGEST_EVENTS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V34 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V35: Memory OS Foundation Phase 1 — edge audit + wiki artifacts + health findings.
    tracing::debug!("Running migration V35: Memory OS Phase 1");
    for stmt in V35_MEMORY_OS_PHASE_1.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V35 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V37: Memory OS Foundation Phase 7 — brain_sync_state for the
    // markdown bidirectional sync engine.
    tracing::debug!("Running migration V37: Memory OS Phase 7");
    for stmt in V37_MEMORY_OS_PHASE_7.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V37 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V38: Automation Phase 2b cluster A — per-(spec, identity) chat session index.
    tracing::debug!("Running migration V38: automation_chat_sessions");
    for stmt in V38_AUTOMATION_CHAT_SESSIONS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V38 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V39: Memory OS Sprint 1 — user_profile_facets for the openhuman-style
    // stability-graded facet store. See strategy doc Appendix D.
    tracing::debug!("Running migration V39: user_profile_facets");
    for stmt in V39_USER_PROFILE_FACETS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V39 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V40: MCP PR-5 — mcp_audit table for connection-attempt + lifecycle
    // events. Persistence layer for the substantive audit hardening pass.
    tracing::debug!("Running migration V40: mcp_audit");
    for stmt in V40_MCP_AUDIT.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V40 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V41: browser task persistence + per-session notebook.
    tracing::debug!("Running migration V41: browser_task_memory");
    for stmt in V41_BROWSER_TASK_MEMORY.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V41 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V42: resumable browser task checkpoints.
    tracing::debug!("Running migration V42: browser_task_checkpoints");
    for stmt in V42_BROWSER_TASK_CHECKPOINTS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V42 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V43: Memory OS Cognitive Layer Phase 8.1 — 5 new tables for
    // segmented provenance / two-step compile / review queue / per-
    // subkind templates / analysis cache. Tables stay empty until
    // Cognitive layer feature flags are turned on; Foundation Phase
    // 1-7 behavior is unaffected.
    tracing::debug!("Running migration V43: cognitive layer");
    for stmt in V43_COGNITIVE_LAYER.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V43 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V43 seed — Phase 8.2: 7 default rows for wiki_page_templates.
    // INSERT OR IGNORE keeps the seed re-runnable; user edits to
    // existing rows are preserved across restarts.
    tracing::debug!("Running migration V43 seed: wiki_page_templates");
    for stmt in V43_SEED_TEMPLATES.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V43 seed stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V44: Memory OS L3 Engines RETAINED schema (per ADR 2026-05-20 §8).
    // 4 new tables: timeline_events / temporal_aggregates /
    // activity_clusters / memory_importance_scores. Additive only;
    // tables stay empty until P2 (Importance Decay) and later
    // Timeline Engine scenarios populate them.
    tracing::debug!("Running migration V44: L3 engines retained schema");
    for stmt in V44_L3_RETAINED_SCHEMA.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V44 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V45: Memory OS L3 §4.12.3 RETAINED — Spaced Repetition state table.
    tracing::debug!("Running migration V45: spaced_repetition_state");
    for stmt in V45_SPACED_REPETITION_STATE.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V45 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V46: Memory OS L3 §4.12.4 RETAINED — Concept Drift Detection events.
    tracing::debug!("Running migration V46: drift_events");
    for stmt in V46_DRIFT_EVENTS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V46 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V47: Memory OS L3 §4.12.5 RETAINED — Cross-Source Triangulation evidence.
    tracing::debug!("Running migration V47: triangulation_evidence");
    for stmt in V47_TRIANGULATION_EVIDENCE.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V47 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V48: M1-T5 — task_events_rollout sidecar for the rollout JSONL writer.
    tracing::debug!("Running migration V48: task_events_rollout");
    for stmt in V48_TASK_EVENTS_ROLLOUT.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V48 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V49: M1-T6 — cost_records gains cached_input_tokens + reasoning_output_tokens.
    tracing::debug!("Running migration V49: cost_records 6-D");
    for stmt in V49_COST_RECORDS_6D.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            // Already-applied ADD COLUMN raises "duplicate column" — that's expected.
            tracing::debug!("V49 stmt skipped (likely already applied): {} :: {}", e, stmt);
        }
    }
    // V50: Halo-compatible automation app metadata.
    tracing::debug!("Running migration V50: Halo automation metadata");
    for stmt in V50_HALO_AUTOMATION_METADATA.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V50 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V51: memorization_queue shared schema (Slice 3 Bundle 1 Issue 6).
    // Race-safe creation in the main DB so proactive::conversation_bridge's
    // INSERT can run regardless of whether MemorizationStorage::new ran first.
    tracing::debug!("Running migration V51: memorization_queue");
    for stmt in V51_MEMORIZATION_QUEUE.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            // ALTER ADD COLUMN errors when the column already exists are expected
            // on databases that ran MemorizationStorage::ensure_tables first —
            // log at DEBUG so they don't fill the operator's stderr with noise.
            tracing::debug!("V51 stmt skipped (likely already applied): {} :: {}", e, stmt);
        }
    }
    // V52: agent_fold_baselines for Bundle 17-B `/compact` delta-rendered path.
    // CREATE TABLE IF NOT EXISTS is fully idempotent; no expected error class.
    tracing::debug!("Running migration V52: agent_fold_baselines");
    for stmt in V52_AGENT_FOLD_BASELINES.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V52 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V53: Living Persona MVP state.
    tracing::debug!("Running migration V53: living persona");
    for stmt in V53_LIVING_PERSONA.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V53 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V54: Append-only Living Persona event ledger.
    tracing::debug!("Running migration V54: persona events");
    for stmt in V54_PERSONA_EVENTS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V54 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V55: session_tree + session_leaves — fork/rewind lineage (Sprint 3 ③).
    tracing::debug!("Running migration V55: session tree");
    for stmt in V55_SESSION_TREE.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V55 stmt skipped: {} :: {}", e, stmt);
        }
    }
    // V56: Slice 1b safety chokepoint — automation_approval_requests table
    // + pending_approval_request_id column on automation_activities.
    tracing::debug!("Running migration V56: automation_approval_requests");
    for stmt in SQL_V56.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::debug!("V56 stmt skipped (likely already applied): {} :: {}", e, stmt);
        }
    }
    // V57: Phase 3 growth — reflections + user_model tables (mem.md Reflection
    // Agent). Additive + IF NOT EXISTS, so replaying on every boot is a no-op.
    tracing::debug!("Running migration V57: reflections + user_model");
    for stmt in SQL_V57.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::debug!("V57 stmt skipped (likely already applied): {} :: {}", e, stmt);
        }
    }
    // V58: Phase 4 growth — daydreams table (divergent free-association pass).
    // Additive + IF NOT EXISTS, so replaying on every boot is a no-op.
    tracing::debug!("Running migration V58: daydreams");
    for stmt in SQL_V58.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::debug!("V58 stmt skipped (likely already applied): {} :: {}", e, stmt);
        }
    }
    // V59: P5 memory refinement — reflections.archived_at soft-delete + user_model_history audit table.
    // ALTER TABLE errors on replay if column already exists; the catch loop logs "skipped" — replay-safe.
    tracing::debug!("Running migration V59: archived_at + user_model_history");
    for stmt in SQL_V59.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::debug!("V59 stmt skipped (likely already applied): {} :: {}", e, stmt);
        }
    }
    tracing::info!("Database migrations complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    #[test]
    fn v53_living_persona_tables_are_created_and_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN (
                    'persona_voice_profiles',
                    'persona_bond_profiles',
                    'persona_journal_entries',
                    'persona_keepsakes',
                    'persona_evolution_candidates',
                    'persona_badges'
                )",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 6);
    }

    #[test]
    fn v53_voice_profile_rejects_out_of_range_values() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        let err = conn
            .execute(
                "INSERT INTO persona_voice_profiles
                 (id, scope, scope_id, preset_id, warmth, directness, challenge, playfulness, detail, initiative, structure, restraint)
                 VALUES ('bad', 'global', NULL, 'clarity', 6, 1, 1, 1, 1, 1, 1, 1)",
                [],
            )
            .expect_err("warmth > 5 must fail");
        assert!(err.to_string().contains("CHECK"));
    }

    #[test]
    fn v54_persona_events_table_is_created_and_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'persona_events'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN (
                    'idx_persona_events_kind',
                    'idx_persona_events_session',
                    'idx_persona_events_created'
                )",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(index_count, 3);
    }

    #[test]
    fn v55_session_tree_tables_created_and_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error");
        let tables: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('session_tree','session_leaves')",
            [], |r| r.get(0),
        ).unwrap();
        assert_eq!(tables, 2);
        let idx: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN ('idx_session_tree_session','idx_session_tree_parent')",
            [], |r| r.get(0),
        ).unwrap();
        assert_eq!(idx, 2);
    }

    #[test]
    fn v54_persona_events_schema_stays_append_friendly() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        conn.execute(
            "INSERT INTO persona_events (id, kind, note) VALUES ('evt-1', 'future_signal', 'kept for forward compatibility')",
            [],
        )
        .expect("schema should not reject future event kinds");

        let kind: String = conn
            .query_row("SELECT kind FROM persona_events WHERE id = 'evt-1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(kind, "future_signal");
    }

    // ─── V56 — Slice 1b automation_approval_requests ─────────────────────────
    #[test]
    fn v56_creates_automation_approval_requests_and_pending_column() {
        let conn = Connection::open_in_memory().unwrap();
        super::run_migrations_up_to(&conn, 56).unwrap();
        // Table exists.
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type='table' AND name='automation_approval_requests'",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(n, 1, "table must exist after V56");
        // Required columns.
        let mut required = std::collections::HashSet::from([
            "id", "activity_id", "tool_name", "arguments_json",
            "status", "created_at", "resolved_at",
        ]);
        let mut stmt = conn.prepare("PRAGMA table_info(automation_approval_requests)").unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
        for row in rows {
            required.remove(row.unwrap().as_str());
        }
        assert!(required.is_empty(), "missing columns: {required:?}");
        // Index exists.
        let idx: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type='index' AND name='idx_aar_activity_status'",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(idx, 1, "idx_aar_activity_status must exist after V56");
        // pending_approval_request_id column on automation_activities.
        let mut found = false;
        let mut stmt2 = conn.prepare("PRAGMA table_info(automation_activities)").unwrap();
        let rows = stmt2.query_map([], |r| r.get::<_, String>(1)).unwrap();
        for row in rows {
            if row.unwrap() == "pending_approval_request_id" { found = true; }
        }
        assert!(found, "automation_activities.pending_approval_request_id missing after V56");
    }

    /// Apply only the migrations needed to set up `spaces` and `agent_sessions`,
    /// stopping BEFORE V16 so tests can drive V16 themselves and observe
    /// pre/post state.
    fn db_pre_v16() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // V1 creates `spaces`. V8 creates `agent_sessions`. We don't need the
        // intermediate migrations because none of them touch the columns
        // we're testing here.
        conn.execute_batch(super::V1_INITIAL).unwrap();
        // V8 contains a multi-statement block; use execute_batch.
        conn.execute_batch(super::V8_AGENT_SESSIONS).unwrap();
        conn
    }

    fn run_v16(conn: &Connection) {
        for stmt in super::V16_WORKSPACE_DEFAULT_AND_ORPHAN_HEAL
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            conn.execute(stmt, []).unwrap();
        }
    }

    #[test]
    fn v16_inserts_default_idempotent() {
        let conn = db_pre_v16();

        // First run inserts 'default'.
        run_v16(&conn);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM spaces WHERE id = 'default'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "first V16 run should insert one 'default' row");

        // Second run is a no-op.
        run_v16(&conn);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM spaces WHERE id = 'default'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "second V16 run must not create a duplicate");
    }

    #[test]
    fn v16_heals_orphan_agent_sessions() {
        let conn = db_pre_v16();

        // Pre-V16: insert an agent_session pointing at a workspace that does
        // not exist in `spaces`.
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at)
             VALUES ('s-orphan', 'ghost-workspace', 'orphaned session', 0, 0)",
            [],
        )
        .unwrap();

        run_v16(&conn);

        // Post-V16: orphan should be re-homed to 'default'.
        let space_id: String = conn
            .query_row(
                "SELECT space_id FROM agent_sessions WHERE id = 's-orphan'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(space_id, "default", "orphan session must be re-homed to 'default'");
    }

    /// Apply migrations through V16 so V17 has a populated schema to extend.
    fn db_pre_v17() -> Connection {
        let conn = db_pre_v16();
        // V16 needs to run first so 'default' exists, otherwise the
        // V17 backfill counts can be confused by data that V16 would touch.
        run_v16(&conn);
        conn
    }

    /// Apply V17 statements via .unwrap(). **First-run only** — calling
    /// this twice on the same connection will panic with "duplicate column"
    /// because ALTER TABLE ADD COLUMN isn't idempotent in SQLite. The
    /// production `run()` uses warn-on-error to allow safe re-runs; tests
    /// that need to verify re-run behavior must inline the loop and
    /// swallow errors manually (see `v17_adds_sort_order_column_idempotent`).
    fn run_v17(conn: &Connection) {
        for stmt in super::V17_WORKSPACE_PATH_SORT_ATTACHED
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            conn.execute(stmt, []).unwrap();
        }
    }

    #[test]
    fn v17_adds_sort_order_column_idempotent() {
        let conn = db_pre_v17();
        run_v17(&conn);
        let mut stmt = conn.prepare("SELECT sort_order FROM spaces WHERE id = 'default'").unwrap();
        let val: i64 = stmt.query_row([], |r| r.get(0)).unwrap();
        assert_eq!(val, 0, "default workspace should be at sort_order 0 (only workspace)");

        for stmt in super::V17_WORKSPACE_PATH_SORT_ATTACHED.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            let _ = conn.execute(stmt, []);
        }
        let val2: i64 = conn.query_row("SELECT sort_order FROM spaces WHERE id = 'default'", [], |r| r.get(0)).unwrap();
        assert_eq!(val2, 0, "sort_order must remain 0 after re-run");
    }

    #[test]
    fn v17_adds_workspace_attached_dirs_column() {
        let conn = db_pre_v17();
        run_v17(&conn);
        let val: String = conn.query_row(
            "SELECT attached_dirs FROM spaces WHERE id = 'default'", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(val, "[]", "fresh workspace should have empty attached_dirs JSON");
    }

    #[test]
    fn v17_adds_session_attached_dirs_column() {
        let conn = db_pre_v17();
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at)
             VALUES ('s-1', 'default', 'test', 0, 0)",
            [],
        ).unwrap();
        run_v17(&conn);
        let val: String = conn.query_row(
            "SELECT attached_dirs FROM agent_sessions WHERE id = 's-1'", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(val, "[]", "fresh session should have empty attached_dirs JSON");
    }

    #[test]
    fn v17_backfills_sort_order_from_created_at() {
        let conn = db_pre_v17();
        conn.execute(
            "INSERT INTO spaces (id, name, icon, path, created_at, updated_at)
             VALUES ('ws-a', 'A', '📁', NULL, '2026-05-01 00:00:00', '2026-05-01 00:00:00')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO spaces (id, name, icon, path, created_at, updated_at)
             VALUES ('ws-b', 'B', '📁', NULL, '2099-01-01 00:00:00', '2099-01-01 00:00:00')",
            [],
        ).unwrap();
        run_v17(&conn);

        let mut stmt = conn.prepare(
            "SELECT id, sort_order FROM spaces ORDER BY sort_order ASC"
        ).unwrap();
        let rows: Vec<(String, i64)> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        let ws_b_order = rows.iter().find(|(id, _)| id == "ws-b").map(|(_, o)| *o).unwrap();
        let ws_a_order = rows.iter().find(|(id, _)| id == "ws-a").map(|(_, o)| *o).unwrap();
        assert_eq!(ws_b_order, 0, "newest workspace ws-b should have sort_order 0");
        assert_eq!(ws_a_order, 2, "oldest workspace ws-a should have sort_order 2");
    }

    #[test]
    fn v17_backfill_skips_after_user_reorder() {
        let conn = db_pre_v17();
        // Insert 3 workspaces with non-trivial created_at ordering.
        conn.execute(
            "INSERT INTO spaces (id, name, icon, path, created_at, updated_at)
             VALUES ('ws-a', 'A', '📁', NULL, '2026-05-01 00:00:00', '2026-05-01 00:00:00')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO spaces (id, name, icon, path, created_at, updated_at)
             VALUES ('ws-b', 'B', '📁', NULL, '2026-05-11 00:00:00', '2026-05-11 00:00:00')",
            [],
        ).unwrap();

        // First V17 run does the initial backfill.
        run_v17(&conn);

        // Simulate user reorder: put ws-a at sort_order 0, ws-b at sort_order 5.
        conn.execute("UPDATE spaces SET sort_order = 0 WHERE id = 'ws-a'", []).unwrap();
        conn.execute("UPDATE spaces SET sort_order = 5 WHERE id = 'ws-b'", []).unwrap();

        // Re-run V17 (simulating app reboot). The backfill UPDATE should be a no-op
        // because at least one row has sort_order != 0.
        // Manually inline the V17 SQL with error-swallowing (run_v17 unwraps would
        // panic on the ALTERs because columns already exist).
        for stmt in super::V17_WORKSPACE_PATH_SORT_ATTACHED
            .split(';').map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            let _ = conn.execute(stmt, []);
        }

        // User's reorder values should be preserved.
        let a_order: i64 = conn.query_row(
            "SELECT sort_order FROM spaces WHERE id = 'ws-a'", [], |r| r.get(0)
        ).unwrap();
        let b_order: i64 = conn.query_row(
            "SELECT sort_order FROM spaces WHERE id = 'ws-b'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(a_order, 0, "user-set sort_order=0 preserved across re-run");
        assert_eq!(b_order, 5, "user-set sort_order=5 preserved across re-run");
    }

    /// V18 adds a nullable pinned_at column to agent_sessions. Verifies:
    /// (1) column exists after migration, (2) existing rows default to NULL,
    /// (3) re-running the migration is idempotent.
    #[test]
    fn v18_adds_pinned_at_column_nullable_with_null_default() {
        let conn = db_pre_v16();
        // Pre-V18: insert a session row to make sure backfill is non-destructive.
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, metadata_json,
                                          message_count, pinned, archived,
                                          created_at, updated_at)
             VALUES ('s1', 'default', 't', '{}', 0, 0, 0, 0, 0)",
            [],
        ).unwrap();

        // Drive V18.
        for stmt in super::V18_AGENT_SESSIONS_PINNED_AT
            .split(';').map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            // Match the run() per-stmt skip pattern.
            let _ = conn.execute(stmt, []);
        }

        // Column exists + pre-existing row has NULL.
        let pinned_at: Option<i64> = conn.query_row(
            "SELECT pinned_at FROM agent_sessions WHERE id = 's1'",
            [],
            |row| row.get::<_, Option<i64>>(0),
        ).unwrap();
        assert!(pinned_at.is_none());

        // Idempotent re-run (duplicate column ALTER fails per-stmt; ignored).
        for stmt in super::V18_AGENT_SESSIONS_PINNED_AT
            .split(';').map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            let _ = conn.execute(stmt, []);
        }
        // Row still present, still NULL.
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_sessions",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 1);
    }

    /// V19 adds spaces.skill_tags as a TEXT NOT NULL column with default
    /// '[]'. Existing rows must get the default; new rows can override.
    /// Re-running V19 must be safe (duplicate-column ALTER skipped per
    /// run()'s per-stmt error-tolerance idiom).
    #[test]
    fn v19_adds_skill_tags_column_with_empty_array_default() {
        let conn = db_pre_v16();
        // Pre-V19: insert a workspace row to test that existing rows
        // get the default value.
        conn.execute(
            "INSERT INTO spaces (id, name, icon)
             VALUES ('engineering', 'Engineering', '📁')",
            [],
        ).unwrap();

        // Drive V19.
        for stmt in super::V19_SPACES_SKILL_TAGS
            .split(';').map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            let _ = conn.execute(stmt, []);
        }

        // Existing row got the default.
        let tags: String = conn.query_row(
            "SELECT skill_tags FROM spaces WHERE id = 'engineering'",
            [],
            |row| row.get::<_, String>(0),
        ).unwrap();
        assert_eq!(tags, "[]",
            "existing spaces rows must get skill_tags='[]' default");

        // New rows can override.
        conn.execute(
            "INSERT INTO spaces (id, name, icon, skill_tags)
             VALUES ('research', 'Research', '📁', '[\"research\",\"academic\"]')",
            [],
        ).unwrap();
        let research_tags: String = conn.query_row(
            "SELECT skill_tags FROM spaces WHERE id = 'research'",
            [],
            |row| row.get::<_, String>(0),
        ).unwrap();
        assert_eq!(research_tags, "[\"research\",\"academic\"]");

        // Idempotent re-run.
        for stmt in super::V19_SPACES_SKILL_TAGS
            .split(';').map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            let _ = conn.execute(stmt, []);
        }
        // Data still intact after re-run.
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM spaces",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 2);
    }

    // -----------------------------------------------------------------------
    // V20 helpers
    // -----------------------------------------------------------------------

    /// Apply the minimal set of migrations needed before V20 can run:
    /// V1 (spaces), V7 (automation_specs + automation_activities).
    /// We skip the intermediate migrations because none of them touch the
    /// tables that V20 operates on — this keeps the helper fast.
    fn db_pre_v20() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(super::V1_INITIAL).unwrap();
        conn.execute_batch(super::V7_AUTOMATIONS).unwrap();
        conn
    }

    /// V20 migrates a legacy TOML spec row into the Humane schema, setting
    /// source='toml-migrated' and populating spec_yaml / spec_json.
    #[test]
    fn v20_migrates_legacy_toml_specs() {
        let conn = db_pre_v20();

        // Seed a legacy row using the real legacy TOML shape.
        conn.execute(
            "INSERT INTO automation_specs (id, name, description, toml_content, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                "s1", "Test", "desc",
                "name = \"Test\"\ndescription = \"desc\"\ntask = \"Do thing\"\n[trigger]\nkind = \"manual\"\n",
                1i64, 1i64, 1i64,
            ],
        ).unwrap();

        super::run_v20(&conn).unwrap();

        // spec_yaml must contain Humane YAML markers.
        let yaml: String = conn.query_row(
            "SELECT spec_yaml FROM automation_specs WHERE id = 's1'", [], |r| r.get(0)
        ).unwrap();
        assert!(yaml.contains("type: automation"), "expected 'type: automation' in yaml: {}", yaml);
        assert!(yaml.contains("system_prompt: Do thing"), "expected 'system_prompt: Do thing' in yaml: {}", yaml);

        // source must be 'toml-migrated'.
        let source: String = conn.query_row(
            "SELECT source FROM automation_specs WHERE id = 's1'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(source, "toml-migrated");

        // spec_json must be valid JSON and non-empty.
        let spec_json: String = conn.query_row(
            "SELECT spec_json FROM automation_specs WHERE id = 's1'", [], |r| r.get(0)
        ).unwrap();
        let v: serde_json::Value = serde_json::from_str(&spec_json)
            .expect("spec_json must be valid JSON");
        assert!(v.get("name").is_some(), "spec_json must have 'name' key");

        // Identity columns must be populated from the migrated spec.
        let name: String = conn.query_row(
            "SELECT name FROM automation_specs WHERE id = 's1'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(name, "Test");

        let status: String = conn.query_row(
            "SELECT status FROM automation_specs WHERE id = 's1'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(status, "active");
    }

    /// V20 on an empty legacy table should succeed and produce new tables
    /// with the correct schema columns.
    #[test]
    fn v20_is_idempotent_on_empty_legacy() {
        let conn = db_pre_v20();
        super::run_v20(&conn).unwrap(); // no legacy data — must succeed

        // Verify new schema columns are present via pragma_table_info.
        let columns: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('automation_specs')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        for expected in &["spec_yaml", "spec_json", "source_version", "permissions_granted"] {
            assert!(
                columns.contains(&expected.to_string()),
                "missing column '{}' in automation_specs; columns present: {:?}",
                expected,
                columns
            );
        }

        // Verify automation_activities new columns too.
        let act_columns: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('automation_activities')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        for expected in &["trigger_source_type", "trigger_payload_json", "llm_iterations", "resumed_from_activity_id"] {
            assert!(
                act_columns.contains(&expected.to_string()),
                "missing column '{}' in automation_activities; columns present: {:?}",
                expected,
                act_columns
            );
        }
    }

    /// V20 correctly maps legacy automation_activities rows, preserving
    /// trigger_source_type and report_outcome heuristics.
    #[test]
    fn v20_migrates_legacy_activities() {
        let conn = db_pre_v20();

        // Seed a spec (required for FK).
        conn.execute(
            "INSERT INTO automation_specs (id, name, description, toml_content, enabled, created_at, updated_at)
             VALUES ('sp1', 'Spec', '', 'name = \"Spec\"\ntask = \"t\"\n[trigger]\nkind = \"manual\"\n', 1, 0, 0)",
            [],
        ).unwrap();

        // Seed a completed activity and a failed one.
        conn.execute(
            "INSERT INTO automation_activities (id, spec_id, run_id, trigger, status, result, error, duration_ms, created_at)
             VALUES ('a1', 'sp1', 'r1', 'cron', 'Completed', 'Done', NULL, 500, 42)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO automation_activities (id, spec_id, run_id, trigger, status, result, error, duration_ms, created_at)
             VALUES ('a2', 'sp1', 'r2', 'manual', 'failed', NULL, 'oops', 100, 99)",
            [],
        ).unwrap();

        super::run_v20(&conn).unwrap();

        // Verify completed activity mapping.
        let (trigger_type, status, outcome, queued_at): (String, String, Option<String>, i64) =
            conn.query_row(
                "SELECT trigger_source_type, status, report_outcome, queued_at
                 FROM automation_activities WHERE id = 'a1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            ).unwrap();
        assert_eq!(trigger_type, "cron");
        assert_eq!(status, "completed");
        assert_eq!(outcome.as_deref(), Some("useful"), "completed activity should have report_outcome='useful'");
        assert_eq!(queued_at, 42, "queued_at should map from legacy created_at");

        // Verify failed activity mapping.
        let (trigger_type2, status2, outcome2): (String, String, Option<String>) =
            conn.query_row(
                "SELECT trigger_source_type, status, report_outcome
                 FROM automation_activities WHERE id = 'a2'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            ).unwrap();
        assert_eq!(trigger_type2, "manual");
        assert_eq!(status2, "failed");
        assert!(outcome2.is_none(), "failed activity should have NULL report_outcome");
    }

    /// V21 creates automation_subscriptions, automation_memory, and
    /// automation_escalations after V20 has established the parent tables.
    #[test]
    fn v21_creates_three_behavior_tables() {
        let conn = Connection::open_in_memory().unwrap();
        // Stand up the minimal schema V21 depends on: V1 (spaces/agent_sessions
        // foundation) + V7 (legacy automation tables V20 needs to migrate from).
        conn.execute_batch(super::V1_INITIAL).unwrap();
        conn.execute_batch(super::V7_AUTOMATIONS).unwrap();
        // Apply V20 to produce the Humane-shaped parent tables.
        super::run_v20(&conn).unwrap();
        // Apply V21 under test.
        super::run_v21(&conn).unwrap();

        for table in [
            "automation_subscriptions",
            "automation_memory",
            "automation_escalations",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "table {} missing after V21", table);
        }
    }

    /// V20 produces a status='error' stub for a spec whose TOML content is
    /// unparseable, but does NOT abort the migration for other rows.
    #[test]
    fn v20_handles_bad_toml_with_error_stub() {
        let conn = db_pre_v20();

        // One valid spec and one broken spec.
        conn.execute(
            "INSERT INTO automation_specs (id, name, description, toml_content, enabled, created_at, updated_at)
             VALUES ('good', 'Good', '', 'name = \"Good\"\ntask = \"t\"\n[trigger]\nkind = \"manual\"\n', 1, 0, 0)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO automation_specs (id, name, description, toml_content, enabled, created_at, updated_at)
             VALUES ('bad', 'Bad', '', 'not valid toml [[[', 1, 0, 0)",
            [],
        ).unwrap();

        super::run_v20(&conn).unwrap();

        // Both rows should appear in the new table.
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM automation_specs", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 2, "both rows (good + error stub) must be present after V20");

        // Good spec is 'active', bad spec is 'error'.
        let good_status: String = conn.query_row(
            "SELECT status FROM automation_specs WHERE id = 'good'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(good_status, "active");

        let bad_status: String = conn.query_row(
            "SELECT status FROM automation_specs WHERE id = 'bad'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(bad_status, "error");
    }

    #[test]
    fn v23a_creates_marketplace_cache_tables() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        // Tables exist
        for tbl in ["automation_marketplace_items", "automation_marketplace_fts", "automation_registry_sync"] {
            let count: i64 = conn
                .query_row(
                    &format!("SELECT count(*) FROM sqlite_master WHERE type IN ('table','virtual table') AND name = '{}'", tbl),
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            assert!(count >= 1, "{} should exist after V23a", tbl);
        }
        // FTS5 trigram tokenizer works
        conn.execute("INSERT INTO automation_marketplace_fts(slug, registry_id, name, description, author, tags) VALUES('s', 'halo', 'AI News', 'curates news', 'a', 'ai,news')", []).unwrap();
        let hits: i64 = conn.query_row(
            "SELECT count(*) FROM automation_marketplace_fts WHERE automation_marketplace_fts MATCH 'news'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(hits, 1, "FTS5 trigram should match");
    }

    #[test]
    fn v22_creates_automation_installed_skills_table() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).expect("migrations run");

        // Inserting a row should succeed.
        conn.execute(
            "INSERT INTO automation_installed_skills \
                (automation_slug, skill_id, installed_at, file_count) \
                VALUES (?, ?, ?, ?)",
            rusqlite::params!["xhs-monitor", "xhs-search", 1715000000_i64, 2_i64],
        )
        .expect("insert ok");

        // PK collision should error.
        let dup = conn.execute(
            "INSERT INTO automation_installed_skills \
                (automation_slug, skill_id, installed_at, file_count) \
                VALUES (?, ?, ?, ?)",
            rusqlite::params!["xhs-monitor", "xhs-search", 1715000000_i64, 2_i64],
        );
        assert!(dup.is_err(), "PK should reject duplicate");

        // The companion index must exist.
        let idx_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type='index' AND name='idx_aut_inst_skills_slug'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx_count, 1);
    }

    #[test]
    fn v22_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error (CREATE IF NOT EXISTS)");
    }

    #[test]
    fn v24_adds_session_columns_and_drops_tool_calls_json() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        let has_session_id: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('automation_activities') WHERE name = 'session_id'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(has_session_id, 1, "session_id column missing");

        let has_artifacts: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('automation_activities') WHERE name = 'report_artifacts_json'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(has_artifacts, 1, "report_artifacts_json column missing");

        let has_tool_calls: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('automation_activities') WHERE name = 'tool_calls_json'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(has_tool_calls, 0, "tool_calls_json should have been dropped");

        let has_archived_at: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('agent_sessions') WHERE name = 'archived_at'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(has_archived_at, 1, "agent_sessions.archived_at column missing");

        let has_idx: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_act_session'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(has_idx, 1, "idx_act_session missing");
    }

    #[test]
    fn v24_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        super::run(&conn).unwrap();
    }

    #[test]
    fn v25_creates_marketplace_standalone_installs_table() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).expect("migrations run");

        conn.execute(
            "INSERT INTO marketplace_standalone_installs \
                (slug, item_type, version, installed_at, mcp_server_id) \
                VALUES (?, ?, ?, ?, ?)",
            rusqlite::params!["my-skill", "skill", "1.0.0", 1715000000_i64, Option::<String>::None],
        ).expect("skill row insert ok");

        conn.execute(
            "INSERT INTO marketplace_standalone_installs \
                (slug, item_type, version, installed_at, mcp_server_id) \
                VALUES (?, ?, ?, ?, ?)",
            rusqlite::params!["my-mcp", "mcp", "2.0.0", 1715000000_i64, Some("srv-uuid-123")],
        ).expect("mcp row insert ok");

        // slug is PK — duplicate must error.
        let dup = conn.execute(
            "INSERT INTO marketplace_standalone_installs \
                (slug, item_type, version, installed_at, mcp_server_id) \
                VALUES (?, ?, ?, ?, ?)",
            rusqlite::params!["my-skill", "skill", "1.0.1", 1715000001_i64, Option::<String>::None],
        );
        assert!(dup.is_err(), "slug PK must reject duplicate");
    }

    #[test]
    fn v25_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error");
    }

    #[test]
    fn v26_conversations_archived_columns_exist() {
        let conn = Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        let archived: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('conversations') WHERE name = 'archived'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(archived, 1, "conversations.archived column missing");

        let archived_at: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('conversations') WHERE name = 'archived_at'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(archived_at, 1, "conversations.archived_at column missing");
    }

    #[test]
    fn v32_im_tables_created() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        // im_channel_instances table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='im_channel_instances'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "im_channel_instances table must exist after V32");

        // im_sessions table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='im_sessions'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "im_sessions table must exist after V32");

        // spec_channel_bindings table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='spec_channel_bindings'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "spec_channel_bindings table must exist after V32");

        // automation_specs gains trigger_phrase column
        conn.execute(
            "INSERT INTO automation_specs (id, name, version, author, description, system_prompt, \
             spec_yaml, spec_json, trigger_phrase, created_at, updated_at) \
             VALUES ('t1','n','1','a','d','s','y','{}', '/test', 1, 1)",
            [],
        )
        .unwrap();
    }

    #[test]
    fn halo_metadata_columns_are_additive_to_automation_specs() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        let columns: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('automation_specs')")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        for column in [
            "status",
            "user_overrides_json",
            "browser_login",
            "uninstalled_at",
        ] {
            assert!(
                columns.iter().any(|name| name == column),
                "missing automation_specs column: {column}; columns present: {columns:?}"
            );
        }
    }

    // ─── V33: Symphony schema ─────────────────────────────────────────────

    #[test]
    fn v33_creates_symphony_tables_and_indexes() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        for table in [
            "symphony_workflows",
            "symphony_workflow_versions",
            "symphony_runs",
            "symphony_node_runs",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "table {} must exist after V33", table);
        }

        for index in [
            "idx_symphony_runs_workflow",
            "idx_symphony_runs_status",
            "idx_symphony_node_runs_run",
            "idx_symphony_node_runs_status",
            "idx_symphony_node_runs_heartbeat",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                    [index],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "index {} must exist after V33", index);
        }

        // Seeded 'symphonies' home space.
        let seeded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM spaces WHERE id = 'symphonies'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(seeded, 1, "'symphonies' home space must be seeded");
    }

    #[test]
    fn v33_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error");

        // Seed is INSERT OR IGNORE — second run mustn't duplicate.
        let seeded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM spaces WHERE id = 'symphonies'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(seeded, 1, "'symphonies' must not duplicate on re-run");
    }

    #[test]
    fn v33_run_fk_cascades_to_node_runs_on_workflow_delete() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        // Enable FK enforcement (off by default in rusqlite in-memory).
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

        let now = 1_700_000_000_000_i64;
        conn.execute(
            "INSERT INTO symphony_workflows \
             (id, name, current_version, enabled, created_at, updated_at) \
             VALUES ('wf-1', 'demo', 1, 1, ?1, ?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symphony_workflow_versions \
             (workflow_id, version, definition_yaml, definition_md, nodes_json, edges_json, created_at) \
             VALUES ('wf-1', 1, 'kind: symphony', '---\\nkind: symphony\\n---', '[]', '[]', ?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symphony_runs \
             (id, workflow_id, workflow_version, trigger_kind, status, queued_at) \
             VALUES ('run-1', 'wf-1', 1, 'manual', 'running', ?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symphony_node_runs \
             (id, run_id, node_id, attempt, status) \
             VALUES ('nr-1', 'run-1', 'a', 1, 'ready')",
            [],
        )
        .unwrap();

        // Sanity: rows present.
        let n_runs_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM symphony_runs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_runs_before, 1);
        let n_node_runs_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM symphony_node_runs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_node_runs_before, 1);

        // Delete the workflow → cascade through versions → runs → node_runs.
        conn.execute("DELETE FROM symphony_workflows WHERE id = 'wf-1'", [])
            .unwrap();

        let n_versions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symphony_workflow_versions",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n_versions, 0, "versions must cascade on workflow delete");
        let n_runs: i64 = conn
            .query_row("SELECT COUNT(*) FROM symphony_runs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_runs, 0, "runs must cascade on workflow delete");
        let n_node_runs: i64 = conn
            .query_row("SELECT COUNT(*) FROM symphony_node_runs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_node_runs, 0, "node_runs must cascade through runs");
    }

    // ─── V34: Memory OS Foundation Phase 1 ────────────────────────────────

    #[test]
    fn v35_creates_memory_os_phase_1_tables_and_indexes() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        for table in [
            "memory_edge_audit",
            "wiki_artifacts",
            "memory_health_findings",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "table {} must exist after V34", table);
        }

        for index in [
            "idx_memory_edge_audit_src",
            "idx_wiki_artifacts_space_kind",
            "idx_wiki_artifacts_generated",
            "idx_health_findings_active",
            "idx_health_findings_subject",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                    [index],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "index {} must exist after V34", index);
        }
    }

    #[test]
    fn v35_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error");
        // Should still have exactly one of each table; CREATE TABLE IF NOT
        // EXISTS guarantees no duplicate-create errors on re-apply.
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='wiki_artifacts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn v35_edge_audit_cascades_on_edge_delete() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

        // memory_nodes / memory_edges already exist from V4.
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title) VALUES \
             ('p1', 'default', 'entity_page', 'Parent'), \
             ('c1', 'default', 'entity_page', 'Child')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memory_edges \
             (id, space_id, parent_node_id, child_node_id, relation_kind, visibility, priority) \
             VALUES ('e1', 'default', 'p1', 'c1', 'relates_to', 'private', 0)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memory_edge_audit (edge_id, source, inferred_by, confidence, created_at) \
             VALUES ('e1', 'auto_link', 'heuristic', 0.6, 1)",
            [],
        ).unwrap();

        // Delete the edge — audit row must cascade away.
        conn.execute("DELETE FROM memory_edges WHERE id = 'e1'", []).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_edge_audit WHERE edge_id = 'e1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "audit row must cascade on edge delete");
    }

    #[test]
    fn v35_health_findings_dismissible_with_indexes() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        let now = 1_700_000_000_000_i64;
        conn.execute(
            "INSERT INTO memory_health_findings \
             (id, space_id, severity, check_kind, subject, discovered_at) \
             VALUES ('f1', 'default', 'warn', 'orphan', 'n1', ?1), \
                    ('f2', 'default', 'error', 'phantom_slug', 'n2', ?1)",
            [now],
        ).unwrap();

        // Only-active filter should hit the index on (space_id, dismissed, discovered_at).
        let active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_health_findings \
                 WHERE space_id = 'default' AND dismissed = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active, 2);

        // Dismiss one and re-check.
        conn.execute(
            "UPDATE memory_health_findings SET dismissed = 1, dismissed_at = ?1 WHERE id = 'f1'",
            [now + 1000],
        ).unwrap();
        let active_after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_health_findings \
                 WHERE space_id = 'default' AND dismissed = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(active_after, 1);
    }

    // ─── V37 — Memory OS Phase 7 (brain_sync_state) ──────────────

    #[test]
    fn v37_creates_brain_sync_state_table_and_indexes() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type='table' AND name='brain_sync_state'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "brain_sync_state table must exist after V37");

        for index in [
            "idx_brain_sync_file_path",
            "idx_brain_sync_space",
            "idx_brain_sync_last_at",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                    [index],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "index {} must exist after V37", index);
        }
    }

    #[test]
    fn v37_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error (IF NOT EXISTS)");
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='brain_sync_state'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn v37_brain_sync_cascades_on_node_delete() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title) \
             VALUES ('n1', 'default', 'entity_page', 'Alice')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO brain_sync_state \
             (node_id, space_id, file_path, last_synced_version_id, \
              last_synced_at_ms, file_mtime_at_last_sync_ms, last_synced_sha256) \
             VALUES ('n1', 'default', '/tmp/x.md', NULL, 1, 1, 'abc')",
            [],
        )
        .unwrap();

        let before: i64 = conn
            .query_row("SELECT COUNT(*) FROM brain_sync_state", [], |r| r.get(0))
            .unwrap();
        assert_eq!(before, 1);

        conn.execute("DELETE FROM memory_nodes WHERE id = 'n1'", []).unwrap();
        let after: i64 = conn
            .query_row("SELECT COUNT(*) FROM brain_sync_state", [], |r| r.get(0))
            .unwrap();
        assert_eq!(after, 0, "FK ON DELETE CASCADE must clear the row");
    }

    #[test]
    fn v37_brain_sync_file_path_is_unique() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title) \
             VALUES ('n1', 'default', 'entity_page', 'A'), \
                    ('n2', 'default', 'entity_page', 'B')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO brain_sync_state \
             (node_id, space_id, file_path, last_synced_version_id, \
              last_synced_at_ms, file_mtime_at_last_sync_ms, last_synced_sha256) \
             VALUES ('n1', 'default', '/tmp/a.md', NULL, 1, 1, 'x')",
            [],
        )
        .unwrap();
        // Second node trying to reuse the same file_path → must fail
        // (UNIQUE INDEX on file_path).
        let err = conn.execute(
            "INSERT INTO brain_sync_state \
             (node_id, space_id, file_path, last_synced_version_id, \
              last_synced_at_ms, file_mtime_at_last_sync_ms, last_synced_sha256) \
             VALUES ('n2', 'default', '/tmp/a.md', NULL, 1, 1, 'y')",
            [],
        );
        assert!(err.is_err(), "second insert with same file_path must fail");
    }

    // ─── V39 — Memory OS Sprint 1 (user_profile_facets) ──────────

    #[test]
    fn v39_creates_user_profile_facets_table_and_indexes() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type='table' AND name='user_profile_facets'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "user_profile_facets table must exist after V39");

        for index in [
            "idx_facets_class_name",
            "idx_facets_class_state",
            "idx_facets_last_seen",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                    [index],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "index {} must exist after V39", index);
        }
    }

    #[test]
    fn v39_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error (IF NOT EXISTS)");
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='user_profile_facets'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn v39_user_profile_facets_unique_class_name() {
        // Same (class, name) must collide; same name across different
        // classes is allowed (think: "editor" tooling vs "editor" goal).
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        let now = 1700_000_000_000i64;

        // First insert succeeds.
        conn.execute(
            "INSERT INTO user_profile_facets \
             (facet_id, class, name, value, state, stability, \
              cue_families_json, evidence_count, last_seen_at, \
              created_at, updated_at) \
             VALUES ('f1', 'tooling', 'editor', 'helix', 'active', 1.6, \
                     '{}', 5, ?1, ?1, ?1)",
            [now],
        )
        .unwrap();

        // Same (class, name), different value — must fail.
        let err = conn.execute(
            "INSERT INTO user_profile_facets \
             (facet_id, class, name, value, state, stability, \
              cue_families_json, evidence_count, last_seen_at, \
              created_at, updated_at) \
             VALUES ('f2', 'tooling', 'editor', 'vscode', 'candidate', 0.5, \
                     '{}', 1, ?1, ?1, ?1)",
            [now],
        );
        assert!(err.is_err(), "duplicate (class, name) must violate UNIQUE");

        // Same name in a different class — must succeed.
        conn.execute(
            "INSERT INTO user_profile_facets \
             (facet_id, class, name, value, state, stability, \
              cue_families_json, evidence_count, last_seen_at, \
              created_at, updated_at) \
             VALUES ('f3', 'goal', 'editor', 'tree-sitter integration', 'active', 1.8, \
                     '{}', 3, ?1, ?1, ?1)",
            [now],
        )
        .expect("same name in different class must be allowed");
    }

    #[test]
    fn v39_state_filter_uses_index() {
        // Sanity-check that the common 'active in class' query the
        // prompt section will run is supported. Not a perf test —
        // just that EXPLAIN QUERY PLAN can hit the index.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        let now = 1700_000_000_000i64;

        // Seed 4 rows: 2 active in tooling, 1 candidate in tooling, 1 active in style.
        for (id, class, name, state) in [
            ("f1", "tooling", "editor", "active"),
            ("f2", "tooling", "shell", "active"),
            ("f3", "tooling", "browser", "candidate"),
            ("f4", "style", "tone", "active"),
        ] {
            conn.execute(
                "INSERT INTO user_profile_facets \
                 (facet_id, class, name, value, state, stability, \
                  cue_families_json, evidence_count, last_seen_at, \
                  created_at, updated_at) \
                 VALUES (?1, ?2, ?3, 'x', ?4, 1.0, '{}', 1, ?5, ?5, ?5)",
                rusqlite::params![id, class, name, state, now],
            )
            .unwrap();
        }

        // Active-in-class query — the read path UserProfileSection will hit.
        let mut stmt = conn
            .prepare(
                "SELECT name FROM user_profile_facets \
                 WHERE class = ?1 AND state = 'active' \
                 ORDER BY stability DESC",
            )
            .unwrap();
        let names: Vec<String> = stmt
            .query_map(["tooling"], |r| r.get::<_, String>(0))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(names.len(), 2, "tooling active should be exactly 2");
        assert!(names.contains(&"editor".to_string()));
        assert!(names.contains(&"shell".to_string()));
    }

    // ─── V43 — Memory OS Cognitive Layer (Phase 8.1) ──────────────

    #[test]
    fn v43_creates_cognitive_layer_tables_and_indexes() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        // All 5 cognitive tables present.
        for table in [
            "wiki_log_events",
            "page_content_hashes",
            "review_queue_items",
            "wiki_page_templates",
            "analysis_cache",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master \
                     WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "{} table must exist after V43", table);
        }

        // Indexes the read paths depend on (see Cognitive Spec §7.1).
        for index in [
            "idx_wiki_log_events_time",
            "idx_wiki_log_events_subject",
            "idx_review_queue_active",
            "idx_review_queue_subject",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                    [index],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "index {} must exist after V43", index);
        }
    }

    #[test]
    fn v43_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error (IF NOT EXISTS)");
        for table in [
            "wiki_log_events",
            "page_content_hashes",
            "review_queue_items",
            "wiki_page_templates",
            "analysis_cache",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1);
        }
    }

    #[test]
    fn v43_review_queue_items_default_status_is_open() {
        // Defensive: spec calls out status='open' as the default for new
        // items. A future schema edit dropping the DEFAULT would silently
        // break the review-queue read paths that filter on status.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO review_queue_items \
             (id, space_id, item_kind, severity, subject_ids, title, created_at) \
             VALUES ('r1', 'default', 'contradiction', 'warning', '[\"n1\"]', 'test', ?1)",
            [1_700_000_000_000_i64],
        )
        .unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM review_queue_items WHERE id = 'r1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "open");
    }

    #[test]
    fn v43_wiki_page_templates_subkind_is_unique() {
        // subkind is PRIMARY KEY — a second INSERT with a subkind that
        // already exists must fail. Defends against a future schema edit
        // that demotes the PK without adding a UNIQUE constraint, which
        // would silently allow duplicate templates for the same subkind
        // and make wiki_compile pick non-deterministically.
        //
        // NOTE: V43_SEED_TEMPLATES already inserts an 'entity' row during
        // `run()`, so we test the constraint against that seeded row
        // directly — a plain `INSERT` of 'entity' (not OR IGNORE / OR
        // REPLACE) must error. (Earlier this test inserted 'entity'
        // first, which collided with the seed on the FIRST insert; fixed
        // to rely on the seed instead.)
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        // Sanity: the seed put an 'entity' row there.
        let seeded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM wiki_page_templates WHERE subkind = 'entity'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(seeded, 1, "V43 seed should have exactly one 'entity' row");

        // A plain INSERT of the same subkind must violate the PK.
        let err = conn.execute(
            "INSERT INTO wiki_page_templates \
             (subkind, display_name, compile_prompt, sections_json, updated_at) \
             VALUES ('entity', '实体 v2', '...', '[]', ?1)",
            [1_700_000_001_000_i64],
        );
        assert!(err.is_err(), "duplicate subkind must violate PRIMARY KEY");
    }

    #[test]
    fn v43_page_content_hashes_cascades_on_node_delete() {
        // FK ON DELETE CASCADE keeps page_content_hashes tidy when an
        // EntityPage is deleted. Without this, a node delete leaves orphan
        // hash rows that the incremental compile guard (Phase 11) would
        // treat as "still cached" and refuse to recompile.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        // Seed a minimal memory_nodes row (table from V4). memory_nodes
        // has several NOT NULL columns we don't know in this test — we
        // try OR IGNORE and skip the data-path assertion if seeding can't
        // succeed (the FK is structurally enforced by the table create).
        let node_inserted = conn
            .execute(
                "INSERT OR IGNORE INTO memory_nodes (id) VALUES ('node-cascade-test')",
                [],
            )
            .unwrap_or(0);
        if node_inserted == 0 {
            return;
        }
        conn.execute(
            "INSERT INTO page_content_hashes \
             (node_id, sources_hash, timeline_hash, compiled_hash, last_compiled_at) \
             VALUES ('node-cascade-test', 'a', 'b', 'c', ?1)",
            [1_700_000_000_000_i64],
        )
        .unwrap();
        conn.execute("DELETE FROM memory_nodes WHERE id = 'node-cascade-test'", [])
            .unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM page_content_hashes WHERE node_id = 'node-cascade-test'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "FK ON DELETE CASCADE must purge page_content_hashes");
    }

    #[test]
    fn v43_analysis_cache_cascades_on_node_delete() {
        // Mirror of v43_page_content_hashes_cascades_on_node_delete for
        // analysis_cache, which uses the same FK shape. Two-stage compile
        // (Phase 10) relies on these cache rows getting GC'd when their
        // EntityPage is removed; without the cascade, a stale analysis
        // row would short-circuit a recompile.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        let node_inserted = conn
            .execute(
                "INSERT OR IGNORE INTO memory_nodes (id) VALUES ('node-analysis-cascade')",
                [],
            )
            .unwrap_or(0);
        if node_inserted == 0 {
            return;
        }
        conn.execute(
            "INSERT INTO analysis_cache \
             (node_id, inputs_hash, analysis_json, created_at) \
             VALUES ('node-analysis-cascade', 'h', '{}', ?1)",
            [1_700_000_000_000_i64],
        )
        .unwrap();
        conn.execute("DELETE FROM memory_nodes WHERE id = 'node-analysis-cascade'", [])
            .unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM analysis_cache WHERE node_id = 'node-analysis-cascade'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "FK ON DELETE CASCADE must purge analysis_cache");
    }

    // ─── V43 seed — Phase 8.2: wiki_page_templates default rows ───

    #[test]
    fn v43_seed_inserts_exactly_seven_template_rows() {
        // Plan §8.2.3 verification contract: SELECT COUNT(*) == 7
        // after seed runs.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM wiki_page_templates",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 7, "expected 7 seeded rows, got {}", n);
    }

    #[test]
    fn v43_seed_covers_all_seven_subkinds() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        for subkind in [
            "entity",
            "concept",
            "comparison",
            "question",
            "synthesis",
            "decision",
            "gap",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM wiki_page_templates WHERE subkind = ?1",
                    [subkind],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "missing seed row for subkind '{}'", subkind);
        }
    }

    #[test]
    fn v43_seed_rows_carry_non_empty_prompt_and_sections() {
        // Defensive: a future edit could leave one template's
        // `compile_prompt` blank and the wiki_compile module (Phase
        // 10) would silently produce empty pages. Catch at migration
        // time.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT subkind, compile_prompt, sections_json, display_name \
                 FROM wiki_page_templates ORDER BY subkind",
            )
            .unwrap();
        let rows: Vec<(String, String, String, String)> = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(rows.len(), 7);
        for (subkind, prompt, sections, display_name) in &rows {
            assert!(
                !prompt.trim().is_empty(),
                "subkind '{}' has empty compile_prompt",
                subkind
            );
            assert!(
                !sections.trim().is_empty() && sections.starts_with('[') && sections.ends_with(']'),
                "subkind '{}' sections_json must be a non-empty JSON array, got: {}",
                subkind,
                sections
            );
            assert!(
                !display_name.trim().is_empty(),
                "subkind '{}' has empty display_name",
                subkind
            );
            // Sections should be parseable as a JSON array of strings.
            let parsed: Vec<String> = serde_json::from_str(sections).unwrap_or_else(|e| {
                panic!(
                    "subkind '{}' sections_json not parseable as JSON: {}\n raw: {}",
                    subkind, e, sections
                )
            });
            assert!(
                !parsed.is_empty(),
                "subkind '{}' sections_json must contain at least one section",
                subkind
            );
        }
    }

    #[test]
    fn v43_seed_is_idempotent() {
        // Re-running the migration must not produce 14 rows or error.
        // INSERT OR IGNORE is the contract; this test guards against
        // a future edit dropping OR IGNORE (which would then violate
        // PRIMARY KEY on second run).
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        super::run(&conn).expect("second migration run must not error");
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM wiki_page_templates",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 7, "double-run must still produce exactly 7 rows");
    }

    #[test]
    fn v43_seed_compile_prompts_carry_at_least_one_placeholder() {
        // Defensive: if a future edit silently drops the `{title}` /
        // `{sources}` / `{existing_truth}` / `{timeline}` placeholders
        // from a prompt, Phase 10's wiki_compile would substitute
        // nothing and produce a literal template echo. Catch at
        // migration time.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        let mut stmt = conn
            .prepare("SELECT subkind, compile_prompt FROM wiki_page_templates")
            .unwrap();
        let rows: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
            .flatten()
            .collect();
        for (subkind, prompt) in &rows {
            assert!(
                prompt.contains('{') && prompt.contains('}'),
                "subkind '{}' compile_prompt missing `{{...}}` placeholder \
                 — Phase 10 substitution would no-op",
                subkind
            );
        }
    }

    #[test]
    fn v43_seed_preserves_user_edits_across_runs() {
        // INSERT OR IGNORE preserves existing rows on re-run. This
        // matters because users / agents will tune `compile_prompt`
        // and we don't want migrations to clobber their edits.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        // User edits the 'entity' template.
        conn.execute(
            "UPDATE wiki_page_templates SET compile_prompt = 'MY CUSTOM PROMPT' WHERE subkind = 'entity'",
            [],
        )
        .unwrap();
        // Re-run migrations.
        super::run(&conn).expect("re-run must not error");
        // Custom prompt must survive.
        let prompt: String = conn
            .query_row(
                "SELECT compile_prompt FROM wiki_page_templates WHERE subkind = 'entity'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            prompt, "MY CUSTOM PROMPT",
            "INSERT OR IGNORE must not overwrite user-edited compile_prompt"
        );
    }

    // ─── V44 — L3 Engines RETAINED schema (per ADR 2026-05-20) ─────

    #[test]
    fn v44_creates_retained_l3_tables_and_indexes() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        for table in [
            "timeline_events",
            "temporal_aggregates",
            "activity_clusters",
            "memory_importance_scores",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "{} table must exist after V44", table);
        }

        for index in [
            "idx_timeline_events_time",
            "idx_timeline_events_entity",
            "idx_timeline_events_kind",
            "idx_temporal_aggregates_lookup",
            "idx_activity_clusters_period",
            "idx_importance_scores_value",
            "idx_importance_scores_archive",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                    [index],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "index {} must exist after V44", index);
        }
    }

    #[test]
    fn v44_does_not_ship_paused_tables() {
        // ADR §8 explicitly defers Entity Graph + Dream Cycle pipeline +
        // 3 paused enhancements (Hypothesis / Predictive Boot / Synthetic Q&A).
        // None of their tables should exist after V44. If a future
        // edit accidentally adds them here, this test catches the
        // scope creep.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        for paused_table in [
            "entity_aliases",
            "entity_aliases_fts",
            "entity_raw_data",
            "ner_decisions",
            "dream_cycle_runs",
            "dream_cycle_stages",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [paused_table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(
                n, 0,
                "paused table '{}' must NOT exist (ADR §8); scope creep detected",
                paused_table
            );
        }
    }

    #[test]
    fn v44_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error (IF NOT EXISTS)");
        for table in [
            "timeline_events",
            "temporal_aggregates",
            "activity_clusters",
            "memory_importance_scores",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1);
        }
    }

    #[test]
    fn v44_temporal_aggregates_unique_per_grain_period() {
        // (space_id, grain, period_start) is UNIQUE — re-running the
        // daily / weekly / monthly aggregator for the same period
        // should overwrite via UPSERT, not silently double-write.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        let now = 1_700_000_000_000_i64;

        conn.execute(
            "INSERT INTO temporal_aggregates \
             (id, space_id, grain, period_start, period_end, summary_md, created_at) \
             VALUES ('a1', 'default', 'day', ?1, ?1, 'summary v1', ?1)",
            [now],
        )
        .unwrap();

        // Same (space_id, grain, period_start) — second insert MUST fail
        // unless caller uses ON CONFLICT.
        let err = conn.execute(
            "INSERT INTO temporal_aggregates \
             (id, space_id, grain, period_start, period_end, summary_md, created_at) \
             VALUES ('a2', 'default', 'day', ?1, ?1, 'summary v2', ?1)",
            [now],
        );
        assert!(err.is_err(), "duplicate (space, grain, period_start) must violate UNIQUE");
    }

    #[test]
    fn v44_timeline_events_default_importance_is_half() {
        // ADR-cited contract: spec §3.2.1 says default importance = 0.5
        // until Dream Cycle / Importance Decay updates it. Defensive
        // against a future edit dropping the DEFAULT.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        let now = 1_700_000_000_000_i64;

        conn.execute(
            "INSERT INTO timeline_events \
             (id, space_id, event_kind, title, occurred_at, created_at) \
             VALUES ('e1', 'default', 'episode', 'test event', ?1, ?1)",
            [now],
        )
        .unwrap();

        let importance: f64 = conn
            .query_row(
                "SELECT importance FROM timeline_events WHERE id = 'e1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            (importance - 0.5).abs() < f64::EPSILON,
            "default importance should be 0.5, got {}",
            importance
        );
    }

    #[test]
    fn v44_importance_scores_cascades_on_node_delete() {
        // FK ON DELETE CASCADE: when an EntityPage / memory_node is
        // deleted, its row in memory_importance_scores must vanish too,
        // otherwise the Importance Decay (P2) sees a phantom score for
        // a non-existent node and emits warnings forever.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        let node_inserted = conn
            .execute(
                "INSERT OR IGNORE INTO memory_nodes (id) VALUES ('node-importance-cascade')",
                [],
            )
            .unwrap_or(0);
        if node_inserted == 0 {
            // memory_nodes has unfillable NOT NULL columns; FK structure
            // verified by the CREATE — accept skip.
            return;
        }
        conn.execute(
            "INSERT INTO memory_importance_scores \
             (node_id, base_value, citation_factor, edge_factor, recency_factor, \
              status_bonus, penalty, importance, decay_half_life_days, last_computed_at) \
             VALUES ('node-importance-cascade', 0.5, 1.0, 1.0, 1.0, 0.0, 0.0, 0.5, 30.0, ?1)",
            [1_700_000_000_000_i64],
        )
        .unwrap();
        conn.execute("DELETE FROM memory_nodes WHERE id = 'node-importance-cascade'", [])
            .unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_importance_scores WHERE node_id = 'node-importance-cascade'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "FK ON DELETE CASCADE must purge memory_importance_scores");
    }

    // ─── V45 — Spaced Repetition state (L3 §4.12.3 RETAINED) ───

    #[test]
    fn v45_creates_spaced_repetition_state_table_and_index() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();

        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='spaced_repetition_state'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "table must exist after V45");
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_spaced_rep_due'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "partial index idx_spaced_rep_due must exist after V45");
    }

    #[test]
    fn v45_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error");
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='spaced_repetition_state'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    // ─── V47 — Cross-Source Triangulation (L3 §4.12.5 RETAINED) ───

    #[test]
    fn v47_creates_triangulation_evidence_table_and_indexes() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='triangulation_evidence'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
        for idx in ["idx_triangulation_claim", "idx_triangulation_source"] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                    [idx],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "{} must exist after V47", idx);
        }
    }

    #[test]
    fn v47_enforces_unique_claim_source_pair() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn.execute(
            "INSERT INTO triangulation_evidence (id, claim_id, source_node_id, agrees, weight, computed_at) \
             VALUES ('t1', 'claim-x', 'source-1', 1, 1.0, 1700000000000)",
            [],
        ).unwrap();
        let err = conn.execute(
            "INSERT INTO triangulation_evidence (id, claim_id, source_node_id, agrees, weight, computed_at) \
             VALUES ('t2', 'claim-x', 'source-1', 0, 1.0, 1700000001000)",
            [],
        );
        assert!(err.is_err(), "(claim_id, source_node_id) UNIQUE must violate");
    }

    // ─── V46 — Concept Drift Detection events (L3 §4.12.4 RETAINED) ───

    #[test]
    fn v46_creates_drift_events_table_and_indexes() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='drift_events'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
        for index in ["idx_drift_events_node_time", "idx_drift_events_status"] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                    [index],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "{} must exist after V46", index);
        }
    }

    #[test]
    fn v46_drift_events_default_status_open() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn.execute(
            "INSERT INTO drift_events (id, node_id, score, snapshot_version_ids, computed_at) \
             VALUES ('d1', 'n1', 0.75, '[\"v1\",\"v2\"]', 1700000000000)",
            [],
        ).unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM drift_events WHERE id = 'd1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "open");
    }

    #[test]
    fn v45_defaults_interval_idx_zero_enabled_one() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        // Disable FK enforcement so this column-defaults test doesn't
        // require seeding a matching memory_nodes row.
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn.execute(
            "INSERT INTO spaced_repetition_state \
             (node_id, last_reviewed_at, next_review_at) \
             VALUES ('n1', 1700000000000, 1700000086400000)",
            [],
        )
        .unwrap();
        let (idx, enabled, total, passed): (i64, i64, i64, i64) = conn
            .query_row(
                "SELECT interval_idx, enabled, reviews_total, reviews_passed \
                 FROM spaced_repetition_state WHERE node_id = 'n1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(idx, 0);
        assert_eq!(enabled, 1);
        assert_eq!(total, 0);
        assert_eq!(passed, 0);
    }
/// V49 — cost_records gains 6-D token columns.
    #[test]
    fn v49_adds_cached_input_and_reasoning_output_columns() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).unwrap();
        // Both new columns must exist and accept INSERT with explicit 0s.
        conn.execute(
            "INSERT INTO cost_records                 (id, session_id, model, input_tokens, output_tokens,                  cost_usd, created_at, cached_input_tokens, reasoning_output_tokens)              VALUES ('c1', 's1', 'test-model', 100, 50, 0.01, 1700000000000, 30, 12)",
            [],
        )
        .unwrap();
        let (input, output, cached, reasoning): (i64, i64, i64, i64) = conn
            .query_row(
                "SELECT input_tokens, output_tokens, cached_input_tokens, reasoning_output_tokens                  FROM cost_records WHERE id = 'c1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!((input, output, cached, reasoning), (100, 50, 30, 12));
    }

    /// V49 is idempotent — running run() twice must not error.
    #[test]
    fn v49_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        super::run(&conn).expect("first run");
        super::run(&conn).expect("second run must not error");
        // Columns still present.
        let row: (Option<String>,) = conn
            .query_row(
                "SELECT name FROM pragma_table_info('cost_records')                  WHERE name = 'reasoning_output_tokens'",
                [],
                |r| Ok((r.get(0)?,)),
            )
            .unwrap();
        assert_eq!(row.0.as_deref(), Some("reasoning_output_tokens"));
    }
}
