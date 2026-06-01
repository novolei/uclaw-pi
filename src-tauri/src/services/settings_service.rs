//! Persisted key/value app settings (the `settings` table) as a Tauri-independent
//! service — the business logic behind the `commands::settings` thin commands and
//! the startup gate in `main.rs`.
//!
//! Operates on a borrowed `&Connection` (not `AppState`) so it unit-tests without
//! Tauri. This is the first slice extracted per the code-organization ADR
//! (2026-05-31); the HTTP-API toggle previously inlined its SQL in command bodies.

use rusqlite::Connection;

/// Read/write the optional-feature flags persisted in the `settings` table.
pub trait SettingsService {
    /// Whether the optional local HTTP API server is enabled (default: off).
    fn http_api_enabled(&self, conn: &Connection) -> bool;
    /// Persist the HTTP-API enablement flag (takes effect on next restart).
    fn set_http_api_enabled(&self, conn: &Connection, enabled: bool) -> rusqlite::Result<()>;
    /// Whether the agent routes through the experimental PiEngine backend
    /// (default: off → legacy loop). Read at startup to seed the runtime override.
    fn pi_engine_enabled(&self, conn: &Connection) -> bool;
    /// Persist the engine-backend choice. The runtime override is updated
    /// separately (by the command) so the change applies without a restart.
    fn set_pi_engine_enabled(&self, conn: &Connection, enabled: bool) -> rusqlite::Result<()>;
}

/// The SQLite-backed implementation (the only one in production).
pub struct DbSettings;

impl SettingsService for DbSettings {
    fn http_api_enabled(&self, conn: &Connection) -> bool {
        conn.query_row(
            "SELECT value FROM settings WHERE key = 'http_api_enabled'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .is_some_and(|v| v == "true" || v == "1")
    }

    fn set_http_api_enabled(&self, conn: &Connection, enabled: bool) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('http_api_enabled', ?1)",
            [if enabled { "true" } else { "false" }],
        )?;
        Ok(())
    }

    fn pi_engine_enabled(&self, conn: &Connection) -> bool {
        conn.query_row(
            "SELECT value FROM settings WHERE key = 'pi_engine_enabled'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .is_some_and(|v| v == "true" || v == "1")
    }

    fn set_pi_engine_enabled(&self, conn: &Connection, enabled: bool) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('pi_engine_enabled', ?1)",
            [if enabled { "true" } else { "false" }],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .unwrap();
        c
    }

    #[test]
    fn http_api_flag_defaults_off_and_round_trips() {
        let c = conn();
        assert!(!DbSettings.http_api_enabled(&c), "default must be off");
        DbSettings.set_http_api_enabled(&c, true).unwrap();
        assert!(DbSettings.http_api_enabled(&c));
        DbSettings.set_http_api_enabled(&c, false).unwrap();
        assert!(!DbSettings.http_api_enabled(&c));
    }

    #[test]
    fn pi_engine_flag_defaults_off_and_round_trips() {
        let c = conn();
        assert!(!DbSettings.pi_engine_enabled(&c), "default must be off (legacy loop)");
        DbSettings.set_pi_engine_enabled(&c, true).unwrap();
        assert!(DbSettings.pi_engine_enabled(&c));
        DbSettings.set_pi_engine_enabled(&c, false).unwrap();
        assert!(!DbSettings.pi_engine_enabled(&c));
    }
}
