//! uclaw-patch(P0§4): stub for `session_sqlite` when `sqlite-sessions` is OFF.
//!
//! Why this exists: uClaw embeds pi in-process alongside uClaw's own `rusqlite`
//! (93 files / 4565 call-sites — migrating them is a multi-week effort). pi's
//! real SQLite session backend pulls `sqlmodel-sqlite` → `libsqlite3-sys 0.37`,
//! which conflicts with uClaw's `rusqlite` (`libsqlite3-sys 0.30`) — only one
//! crate may link native `sqlite3`. Running pi STATELESS (`no_session=true`,
//! original F2: uClaw owns persistence) means pi needs no SQLite, so we make
//! `sqlite-sessions` optional and default-OFF. This stub keeps the un-gated
//! `crate::session_sqlite::*` callers compiling when the feature is off; they
//! are unreachable under `no_session` and return a clear error if hit.
//!
//! This is a minimal, recorded pi change (P0 rule 4) — the only alternative was
//! 4565 rusqlite call-site migrations on the uClaw side. Logged in
//! docs/MIGRATION_GOALS.md / docs/R5-removal-plan.md.

use std::path::Path;

use crate::error::{Error, Result};
use crate::session::{SessionEntry, SessionHeader};

#[derive(Debug, Clone)]
pub struct SqliteSessionMeta {
    pub header: SessionHeader,
    pub message_count: u64,
    pub name: Option<String>,
}

fn disabled<T>() -> Result<T> {
    Err(Error::session(
        "SQLite session backend disabled (sqlite-sessions feature off); pi runs \
         no_session and uClaw owns persistence (F2)"
            .to_string(),
    ))
}

pub async fn load_session(_path: &Path) -> Result<(SessionHeader, Vec<SessionEntry>)> {
    disabled()
}

pub async fn load_session_meta(_path: &Path) -> Result<SqliteSessionMeta> {
    disabled()
}

pub async fn save_session(
    _path: &Path,
    _header: &SessionHeader,
    _entries: &[SessionEntry],
) -> Result<()> {
    disabled()
}

pub async fn append_entries(
    _path: &Path,
    _new_entries: &[SessionEntry],
    _start_seq: usize,
    _message_count: u64,
    _session_name: Option<&str>,
) -> Result<()> {
    disabled()
}
