pub mod app;
pub mod config;
pub mod db;
pub mod error;
pub mod ipc;
pub mod settings;
pub mod tauri_commands;
pub mod cost_store;
// [R1 接线] Tauri EventSink adapter for the new pi-based agent backend (PiEngine).
pub mod engine_sink;
// [R2 消息核心闭环] Backend-only persistence of the PiEngine chat path → uClaw SQLite.
pub mod engine_persist;

pub mod agent;
pub mod llm;
pub mod api;

// B0: Infrastructure
pub mod background;
pub mod notifications;
pub mod infra;
// Phase 0.5 M1-T1 — IntentSpec/TaskSpec/TaskEvent runtime contracts.
pub mod runtime;

// M4-T1 — World projection types skeleton.
pub mod world;
// [R5] intent_classifier 已删（旧后端认知层，0 外部引用）。
// M3-T7 — IM channel adapter types (Slack/Discord/Telegram/...).
pub mod im_channels;
// Agent OS Memory Policy spine.
pub mod memory_policy;
// M3-T8 — SKILL.md frontmatter schema + parser (distinct from the
// existing `skills_manifest` module handling a different format).
pub mod skill_md_parse;
// M7-T1 — Plugin manifest schema + TOML loader.
pub mod plugin_manifest;
// P3-4.1 — Plugin discovery: scan + parse plugin.toml manifests.
pub mod plugins;
// M3-T6 — Policy evaluator (PolicySpec rules → HookDecision).
pub mod policy_eval;

// B2: Infrastructure modules
pub mod memory;
pub mod memory_adapter;
pub mod memory_bucket_seal;
pub mod memory_graph;
pub mod skills;
pub mod skills_manifest;
pub mod mcp;
// M3-T9 — MCP server: uclaw exposes its own capabilities via MCP.
pub mod mcp_server;
pub mod gbrain;
pub mod channels;
pub mod providers;
pub mod workspace;
pub mod safety;
pub mod stt;
pub mod memu;
pub mod proactive;
pub mod learning;

// Re-export key types
pub use error::Error;
pub use ipc::*;
pub mod services;
pub mod memubot_config;
pub mod memorization;
pub mod local_api;
pub mod observability;

// Phase 3: Preview Engine
pub mod preview;

// Phase 3: AI Browser
pub mod browser;

// Phase 3: Automation
pub mod automation;

// Phase 4: Symphony — DAG-of-agent-runs runtime (parallel to Chat/Agent/Automation).
// [R5] symphony_graph 已删（旧后端多-agent 编排，无 kept 依赖，-8 rusqlite）。

// Phase 3: Files Rail
pub mod files_rail;

// W6: Git integration (workspace + branch picker backbone)
pub mod git;
pub mod tauri_commands_git;

// Offline eval
// [R5] eval 已删（旧后端评测 harness，0 rusqlite）。

// Sub-project B: knowledge ingestion pipeline
pub mod ingestion;
