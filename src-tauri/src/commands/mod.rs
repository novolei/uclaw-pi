//! Tauri command layer, organized **one domain per file**.
//!
//! Per the code-organization ADR (`docs/adr/2026-05-31-pi-code-organization-discipline.md`):
//! command bodies are thin — parse input → call a [`crate::services`] service →
//! map result/error → emit event — with all business logic in the services.
//!
//! New domains are added HERE, never appended to the legacy `tauri_commands.rs`
//! god file (~13k lines), which is being decomposed into this module one domain
//! at a time. `settings` is the first slice (the HTTP-API toggle).

pub mod background_task;
pub mod conversation;
pub mod cost;
pub mod llm_config;
pub mod notification;
pub mod safety;
pub mod search;
pub mod settings;
pub mod space;
