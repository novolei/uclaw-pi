mod manager;
mod power;
mod types;

pub use manager::{ManagedService, ServiceManager};
pub use power::PowerService;
pub use types::*;

// ─── Domain business-logic services ────────────────────────────────────────
// Per the code-organization ADR (docs/adr/2026-05-31-pi-code-organization-discipline.md):
// Tauri-independent, unit-testable `trait` + impl. Command bodies in
// `crate::commands` call into these; logic is extracted here out of the legacy
// `tauri_commands.rs` god file and out of bridges like `engine_sink.rs`.
pub mod cost_service;
pub mod settings_service;
pub mod space_service;
pub mod workspace_service;

