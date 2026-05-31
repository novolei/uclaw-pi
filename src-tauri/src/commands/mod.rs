//! Tauri command layer, organized **one domain per file**.
//!
//! Per the code-organization ADR (`docs/adr/2026-05-31-pi-code-organization-discipline.md`):
//! command bodies are thin — parse input → call a [`crate::services`] service →
//! map result/error → emit event — with all business logic in the services.
//!
//! New domains are added HERE, never appended to the legacy `tauri_commands.rs`
//! god file (~13k lines), which is being decomposed into this module one domain
//! at a time. `settings` is the first slice (the HTTP-API toggle).

pub mod artifact;
pub mod automation;
pub mod background_task;
pub mod bootstrap;
pub mod browser_cmds;
pub mod channel;
pub mod conversation;
pub mod cost;
pub mod dev_testing;
pub mod gep;
pub mod humane_automation;
pub mod knowledge_ingestion;
pub mod learned_skills;
pub mod llm_config;
pub mod mcp;
pub mod memory;
pub mod memubot;
pub mod notification;
pub mod persona;
pub mod provider;
pub mod safety;
pub mod search;
pub mod settings;
pub mod skills;
pub mod tool_approval;
pub mod trajectory;
pub mod space;
pub mod system_prompt;
pub mod system_tray;
pub mod workspace;
