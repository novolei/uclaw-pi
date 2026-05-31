//! Channel/IM-domain logic as a Tauri-independent service — the business logic
//! behind the `commands::channel` thin commands.
//!
//! Operates on a borrowed `&Connection` (never `AppState`/`State`) so it
//! unit-tests without Tauri. Extracted from the legacy `tauri_commands.rs` god
//! file per the code-organization ADR (2026-05-31).
//!
//! Scope: the SQL-backed half of the Channel block — IM channel instance CRUD
//! (`im_channel_instances`), the ilink (WeChat) config/credentials read+merge
//! used by the QR-binding flow, spec↔channel bindings (`spec_channel_bindings`),
//! and the per-spec IM settings write (`automation_specs.trigger_phrase` /
//! `system_prompt_override`). The in-memory `channel_manager` /
//! `im_channel_manager` calls (add/remove/toggle the legacy channels, runtime
//! statuses, instance restarts, the HTTP QR fetch) have no SQL and stay in the
//! commands.
//!
//! The wire types ([`ImChannelInput`], [`ImChannelRow`], [`SpecChannelBinding`])
//! are the shapes this service reads/writes, so they live here and are imported
//! by `commands::channel`. SSRF URL validation ([`validate_im_channel_urls`]) is
//! enforced inside `create`/`update` so the guard lives with the logic.

use rusqlite::Connection;

use crate::error::Error;

/// Input payload for creating/updating an IM channel instance (the wire shape
/// from the Settings UI). Credentials are kept separate from `config` so they
/// can be persisted into `credentials_json` distinctly.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImChannelInput {
    pub space_id: String,
    pub channel_type: String,
    pub name: String,
    pub config: serde_json::Value,
    pub credentials: serde_json::Value,
    pub enabled: bool,
    pub streaming: bool,
    pub reply_scope: String,
    pub permission_enabled: bool,
    pub owners: Vec<String>,
    pub guest_policy: serde_json::Value,
}

/// A persisted IM channel instance row (credentials intentionally omitted from
/// the read shape — only `config` is surfaced to the UI).
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImChannelRow {
    pub id: String,
    pub space_id: String,
    pub channel_type: String,
    pub name: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub streaming: bool,
    pub reply_scope: String,
    pub permission_enabled: bool,
    pub owners: Vec<String>,
    pub guest_policy: serde_json::Value,
    pub created_at: i64,
    pub updated_at: i64,
}

/// One spec↔channel binding, optionally enriched with the bound instance's
/// `name` / `channel_type` (via LEFT JOIN) for display.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecChannelBinding {
    pub channel_instance_id: String,
    pub enabled: bool,
    pub channel_name: Option<String>,
    pub channel_type: Option<String>,
}

/// Reject URLs that could be used for SSRF: non-https/wss schemes or
/// loopback / RFC-1918 host targets.
fn validate_im_channel_url(raw: &str, field: &str) -> Result<(), Error> {
    if raw.is_empty() {
        return Ok(());
    }
    let parsed = url::Url::parse(raw)
        .map_err(|_| Error::InvalidInput(format!("{field}: not a valid URL")))?;

    match parsed.scheme() {
        "https" | "wss" => {}
        s => return Err(Error::InvalidInput(format!("{field}: scheme '{s}' not allowed; use https or wss"))),
    }

    let host = parsed.host_str().unwrap_or("");
    // Reject loopback
    if host == "localhost" || host == "127.0.0.1" || host == "::1" {
        return Err(Error::InvalidInput(format!("{field}: loopback target not allowed")));
    }
    // Reject RFC-1918 and link-local
    if let Ok(addr) = host.parse::<std::net::IpAddr>() {
        let blocked = match addr {
            std::net::IpAddr::V4(v4) => {
                v4.is_private() || v4.is_loopback() || v4.is_link_local()
            }
            std::net::IpAddr::V6(v6) => v6.is_loopback(),
        };
        if blocked {
            return Err(Error::InvalidInput(format!("{field}: private/loopback IP not allowed")));
        }
    }
    Ok(())
}

/// Check all URL fields embedded in the channel config/credentials JSON blobs.
pub(crate) fn validate_im_channel_urls(input: &ImChannelInput) -> Result<(), Error> {
    for field in &["ws_url", "base_url", "polling_url"] {
        if let Some(v) = input.config.get(field).and_then(|v| v.as_str()) {
            validate_im_channel_url(v, field)?;
        }
    }
    for field in &["webhook_url"] {
        if let Some(v) = input.credentials.get(field).and_then(|v| v.as_str()) {
            validate_im_channel_url(v, field)?;
        }
    }
    Ok(())
}

/// IM channel instance + spec-binding persistence (the `im_channel_instances`,
/// `spec_channel_bindings`, and `automation_specs` IM columns).
///
/// All methods are SQL-only and take a borrowed `&Connection`; the manager
/// restarts / HTTP calls that wrap them stay in `commands::channel`.
pub trait ChannelService {
    /// List IM channel instances, optionally filtered by `space_id`, newest first.
    fn list_instances(
        &self,
        conn: &Connection,
        space_id: Option<&str>,
    ) -> Result<Vec<ImChannelRow>, Error>;

    /// Validate URLs (SSRF guard) then insert a new instance. Returns its new id.
    fn create_instance(&self, conn: &Connection, input: &ImChannelInput) -> Result<String, Error>;

    /// Validate URLs (SSRF guard) then update an existing instance in place.
    fn update_instance(
        &self,
        conn: &Connection,
        id: &str,
        input: &ImChannelInput,
    ) -> Result<(), Error>;

    /// Delete an instance and its spec bindings (the bindings FK is not enforced,
    /// so we clear them explicitly).
    fn delete_instance(&self, conn: &Connection, id: &str) -> Result<(), Error>;

    /// Flip an instance's `enabled` flag (touches `updated_at`).
    fn set_instance_enabled(&self, conn: &Connection, id: &str, enabled: bool) -> Result<(), Error>;

    /// Read an instance's `config_json.base_url`, falling back to the ilink
    /// default when empty/absent. Errors `NotFound` if the instance is missing.
    fn instance_ilink_base_url(&self, conn: &Connection, instance_id: &str) -> Result<String, Error>;

    /// Persist `bot_token` into `credentials_json` and merge `account_id` into
    /// `config_json` (preserving e.g. `base_url`). Errors `NotFound` if absent.
    fn save_ilink_token(
        &self,
        conn: &Connection,
        instance_id: &str,
        bot_token: &str,
        account_id: &str,
    ) -> Result<(), Error>;

    /// Clear `credentials_json` and remove `account_id` from `config_json`.
    /// Errors `NotFound` if the instance is absent.
    fn disconnect_ilink(&self, conn: &Connection, instance_id: &str) -> Result<(), Error>;

    /// List the spec↔channel bindings for a spec, enriched with instance name/type.
    fn list_spec_bindings(
        &self,
        conn: &Connection,
        spec_id: &str,
    ) -> Result<Vec<SpecChannelBinding>, Error>;

    /// Replace a spec's bindings wholesale (delete-all + insert) in one tx.
    fn update_spec_bindings(
        &self,
        conn: &mut Connection,
        spec_id: &str,
        bindings: &[SpecChannelBinding],
    ) -> Result<(), Error>;

    /// Patch a spec's IM trigger phrase / system-prompt override (only the
    /// provided fields change; `None` leaves the column untouched).
    fn update_spec_im_settings(
        &self,
        conn: &Connection,
        spec_id: &str,
        trigger_phrase: Option<&str>,
        system_prompt_override: Option<&str>,
    ) -> Result<(), Error>;
}

/// The SQLite-backed implementation (the only one in production).
pub struct DbChannel;

impl ChannelService for DbChannel {
    fn list_instances(
        &self,
        conn: &Connection,
        space_id: Option<&str>,
    ) -> Result<Vec<ImChannelRow>, Error> {
        let sql = if space_id.is_some() {
            "SELECT id, space_id, channel_type, name, config_json, enabled, streaming, \
             reply_scope, permission_enabled, owners_json, guest_policy_json, created_at, updated_at \
             FROM im_channel_instances WHERE space_id = ?1 ORDER BY created_at DESC"
        } else {
            "SELECT id, space_id, channel_type, name, config_json, enabled, streaming, \
             reply_scope, permission_enabled, owners_json, guest_policy_json, created_at, updated_at \
             FROM im_channel_instances WHERE 1=1 ORDER BY created_at DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows: Vec<ImChannelRow> = stmt
            .query_map(
                rusqlite::params_from_iter(space_id.iter().copied()),
                |r| {
                    Ok(ImChannelRow {
                        id: r.get(0)?,
                        space_id: r.get(1)?,
                        channel_type: r.get(2)?,
                        name: r.get(3)?,
                        config: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
                        enabled: r.get::<_, i64>(5)? != 0,
                        streaming: r.get::<_, i64>(6)? != 0,
                        reply_scope: r.get(7)?,
                        permission_enabled: r.get::<_, i64>(8)? != 0,
                        owners: serde_json::from_str(&r.get::<_, String>(9)?).unwrap_or_default(),
                        guest_policy: serde_json::from_str(&r.get::<_, String>(10)?)
                            .unwrap_or_default(),
                        created_at: r.get(11)?,
                        updated_at: r.get(12)?,
                    })
                },
            )?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    fn create_instance(&self, conn: &Connection, input: &ImChannelInput) -> Result<String, Error> {
        validate_im_channel_urls(input)?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let config_json = serde_json::to_string(&input.config).unwrap_or_else(|_| "{}".into());
        let creds_json = serde_json::to_string(&input.credentials).unwrap_or_else(|_| "{}".into());
        let owners_json = serde_json::to_string(&input.owners).unwrap_or_else(|_| "[]".into());
        let gp_json = serde_json::to_string(&input.guest_policy).unwrap_or_else(|_| "{}".into());
        conn.execute(
            "INSERT INTO im_channel_instances \
             (id, space_id, channel_type, name, config_json, credentials_json, enabled, streaming, \
              reply_scope, permission_enabled, owners_json, guest_policy_json, created_at, updated_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?13)",
            rusqlite::params![
                id, input.space_id, input.channel_type, input.name,
                config_json, creds_json,
                input.enabled as i64, input.streaming as i64,
                input.reply_scope, input.permission_enabled as i64,
                owners_json, gp_json, now,
            ],
        )?;
        Ok(id)
    }

    fn update_instance(
        &self,
        conn: &Connection,
        id: &str,
        input: &ImChannelInput,
    ) -> Result<(), Error> {
        validate_im_channel_urls(input)?;
        let now = chrono::Utc::now().timestamp_millis();
        let config_json = serde_json::to_string(&input.config).unwrap_or_else(|_| "{}".into());
        let creds_json = serde_json::to_string(&input.credentials).unwrap_or_else(|_| "{}".into());
        let owners_json = serde_json::to_string(&input.owners).unwrap_or_else(|_| "[]".into());
        let gp_json = serde_json::to_string(&input.guest_policy).unwrap_or_else(|_| "{}".into());
        conn.execute(
            "UPDATE im_channel_instances SET \
             space_id=?1, channel_type=?2, name=?3, config_json=?4, credentials_json=?5, \
             enabled=?6, streaming=?7, reply_scope=?8, permission_enabled=?9, \
             owners_json=?10, guest_policy_json=?11, updated_at=?12 \
             WHERE id=?13",
            rusqlite::params![
                input.space_id, input.channel_type, input.name,
                config_json, creds_json,
                input.enabled as i64, input.streaming as i64,
                input.reply_scope, input.permission_enabled as i64,
                owners_json, gp_json, now, id,
            ],
        )?;
        Ok(())
    }

    fn delete_instance(&self, conn: &Connection, id: &str) -> Result<(), Error> {
        conn.execute(
            "DELETE FROM spec_channel_bindings WHERE channel_instance_id=?1",
            [&id],
        )?;
        conn.execute("DELETE FROM im_channel_instances WHERE id=?1", [&id])?;
        Ok(())
    }

    fn set_instance_enabled(&self, conn: &Connection, id: &str, enabled: bool) -> Result<(), Error> {
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE im_channel_instances SET enabled=?1, updated_at=?2 WHERE id=?3",
            rusqlite::params![enabled as i64, now, id],
        )?;
        Ok(())
    }

    fn instance_ilink_base_url(&self, conn: &Connection, instance_id: &str) -> Result<String, Error> {
        let config_json: String = conn
            .query_row(
                "SELECT config_json FROM im_channel_instances WHERE id = ?1",
                [&instance_id],
                |r| r.get(0),
            )
            .map_err(|_| Error::NotFound(format!("Channel {instance_id} not found")))?;
        let config: serde_json::Value = serde_json::from_str(&config_json).unwrap_or_default();
        Ok(config["base_url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(crate::channels::im::ilink_binding::ILINK_BASE_URL)
            .to_string())
    }

    fn save_ilink_token(
        &self,
        conn: &Connection,
        instance_id: &str,
        bot_token: &str,
        account_id: &str,
    ) -> Result<(), Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let creds_json = serde_json::json!({ "bot_token": bot_token }).to_string();
        // Merge account_id into existing config (preserves base_url etc.)
        let existing_config: String = conn
            .query_row(
                "SELECT config_json FROM im_channel_instances WHERE id = ?1",
                [&instance_id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "{}".to_string());
        let mut config: serde_json::Value =
            serde_json::from_str(&existing_config).unwrap_or_default();
        config["account_id"] = serde_json::Value::String(account_id.to_string());
        let config_json = config.to_string();
        let rows_changed = conn.execute(
            "UPDATE im_channel_instances \
             SET credentials_json = ?1, config_json = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![creds_json, config_json, now, instance_id],
        )?;
        if rows_changed == 0 {
            return Err(Error::NotFound(format!("Channel {instance_id} not found")));
        }
        Ok(())
    }

    fn disconnect_ilink(&self, conn: &Connection, instance_id: &str) -> Result<(), Error> {
        let now = chrono::Utc::now().timestamp_millis();
        let existing_config: String = conn
            .query_row(
                "SELECT config_json FROM im_channel_instances WHERE id = ?1",
                [&instance_id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "{}".to_string());
        let mut config: serde_json::Value =
            serde_json::from_str(&existing_config).unwrap_or_default();
        if let Some(obj) = config.as_object_mut() {
            obj.remove("account_id");
        }
        let config_json = config.to_string();
        let rows_changed = conn.execute(
            "UPDATE im_channel_instances \
             SET credentials_json = '{}', config_json = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![config_json, now, instance_id],
        )?;
        if rows_changed == 0 {
            return Err(Error::NotFound(format!("Channel {instance_id} not found")));
        }
        Ok(())
    }

    fn list_spec_bindings(
        &self,
        conn: &Connection,
        spec_id: &str,
    ) -> Result<Vec<SpecChannelBinding>, Error> {
        let mut stmt = conn.prepare(
            "SELECT b.channel_instance_id, b.enabled, i.name, i.channel_type \
             FROM spec_channel_bindings b \
             LEFT JOIN im_channel_instances i ON i.id = b.channel_instance_id \
             WHERE b.spec_id = ?1",
        )?;
        let rows = stmt
            .query_map([&spec_id], |r| {
                Ok(SpecChannelBinding {
                    channel_instance_id: r.get(0)?,
                    enabled: r.get::<_, i64>(1)? != 0,
                    channel_name: r.get(2)?,
                    channel_type: r.get(3)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    fn update_spec_bindings(
        &self,
        conn: &mut Connection,
        spec_id: &str,
        bindings: &[SpecChannelBinding],
    ) -> Result<(), Error> {
        let tx = conn.transaction().map_err(|e| Error::Internal(e.to_string()))?;
        tx.execute(
            "DELETE FROM spec_channel_bindings WHERE spec_id=?1",
            [&spec_id],
        )?;
        for b in bindings {
            tx.execute(
                "INSERT INTO spec_channel_bindings (spec_id, channel_instance_id, enabled) \
                 VALUES (?1,?2,?3)",
                rusqlite::params![spec_id, b.channel_instance_id, b.enabled as i64],
            )?;
        }
        tx.commit().map_err(|e| Error::Internal(e.to_string()))?;
        Ok(())
    }

    fn update_spec_im_settings(
        &self,
        conn: &Connection,
        spec_id: &str,
        trigger_phrase: Option<&str>,
        system_prompt_override: Option<&str>,
    ) -> Result<(), Error> {
        conn.execute(
            "UPDATE automation_specs
             SET trigger_phrase        = CASE WHEN ?2 IS NOT NULL THEN ?2 ELSE trigger_phrase END,
                 system_prompt_override = CASE WHEN ?3 IS NOT NULL THEN ?3 ELSE system_prompt_override END,
                 updated_at            = ?4
             WHERE id = ?1",
            rusqlite::params![
                spec_id,
                trigger_phrase,
                system_prompt_override,
                chrono::Utc::now().timestamp_millis(),
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn sample_input(space: &str, name: &str) -> ImChannelInput {
        ImChannelInput {
            space_id: space.to_string(),
            channel_type: "wechat".to_string(),
            name: name.to_string(),
            config: serde_json::json!({ "base_url": "https://im.example.com" }),
            credentials: serde_json::json!({}),
            enabled: true,
            streaming: false,
            reply_scope: "all".to_string(),
            permission_enabled: false,
            owners: vec!["u1".to_string()],
            guest_policy: serde_json::json!({ "mode": "deny" }),
        }
    }

    #[test]
    fn create_then_list_round_trips_and_filters_by_space() {
        let conn = test_conn();
        let id_a = DbChannel
            .create_instance(&conn, &sample_input("space-a", "Alpha"))
            .unwrap();
        let _id_b = DbChannel
            .create_instance(&conn, &sample_input("space-b", "Beta"))
            .unwrap();

        // Unfiltered: both instances.
        let all = DbChannel.list_instances(&conn, None).unwrap();
        assert_eq!(all.len(), 2);

        // Filtered by space: only space-a, and the JSON blobs round-trip.
        let only_a = DbChannel.list_instances(&conn, Some("space-a")).unwrap();
        assert_eq!(only_a.len(), 1);
        let row = &only_a[0];
        assert_eq!(row.id, id_a);
        assert_eq!(row.name, "Alpha");
        assert_eq!(row.owners, vec!["u1".to_string()]);
        assert_eq!(row.config["base_url"], "https://im.example.com");
        assert_eq!(row.guest_policy["mode"], "deny");
        assert!(row.enabled);
    }

    #[test]
    fn update_and_toggle_mutate_in_place() {
        let conn = test_conn();
        let id = DbChannel
            .create_instance(&conn, &sample_input("s", "Original"))
            .unwrap();

        let mut updated = sample_input("s", "Renamed");
        updated.reply_scope = "mention".to_string();
        DbChannel.update_instance(&conn, &id, &updated).unwrap();
        let row = &DbChannel.list_instances(&conn, None).unwrap()[0];
        assert_eq!(row.name, "Renamed");
        assert_eq!(row.reply_scope, "mention");

        DbChannel.set_instance_enabled(&conn, &id, false).unwrap();
        let row = &DbChannel.list_instances(&conn, None).unwrap()[0];
        assert!(!row.enabled, "toggle must clear the enabled flag");
    }

    #[test]
    fn create_rejects_ssrf_url_in_config() {
        let conn = test_conn();
        let mut bad = sample_input("s", "Evil");
        bad.config = serde_json::json!({ "ws_url": "http://127.0.0.1:9000/hook" });
        let err = DbChannel.create_instance(&conn, &bad).unwrap_err();
        assert!(
            matches!(err, Error::InvalidInput(_)),
            "loopback/non-https URL must be rejected before insert, got {err:?}"
        );
        // Nothing persisted.
        assert!(DbChannel.list_instances(&conn, None).unwrap().is_empty());
    }

    #[test]
    fn delete_instance_clears_bindings_too() {
        let mut conn = test_conn();
        let id = DbChannel
            .create_instance(&conn, &sample_input("s", "X"))
            .unwrap();
        DbChannel
            .update_spec_bindings(
                &mut conn,
                "spec-1",
                &[SpecChannelBinding {
                    channel_instance_id: id.clone(),
                    enabled: true,
                    channel_name: None,
                    channel_type: None,
                }],
            )
            .unwrap();
        assert_eq!(DbChannel.list_spec_bindings(&conn, "spec-1").unwrap().len(), 1);

        DbChannel.delete_instance(&conn, &id).unwrap();
        assert!(DbChannel.list_instances(&conn, None).unwrap().is_empty());
        assert!(
            DbChannel.list_spec_bindings(&conn, "spec-1").unwrap().is_empty(),
            "deleting an instance must cascade to its spec bindings"
        );
    }

    #[test]
    fn list_spec_bindings_enriches_from_join() {
        let mut conn = test_conn();
        let id = DbChannel
            .create_instance(&conn, &sample_input("s", "Bound"))
            .unwrap();
        DbChannel
            .update_spec_bindings(
                &mut conn,
                "spec-9",
                &[SpecChannelBinding {
                    channel_instance_id: id.clone(),
                    enabled: false,
                    channel_name: None,
                    channel_type: None,
                }],
            )
            .unwrap();
        let bindings = DbChannel.list_spec_bindings(&conn, "spec-9").unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].channel_instance_id, id);
        assert!(!bindings[0].enabled);
        // Enriched via LEFT JOIN onto the instance row.
        assert_eq!(bindings[0].channel_name.as_deref(), Some("Bound"));
        assert_eq!(bindings[0].channel_type.as_deref(), Some("wechat"));
    }

    #[test]
    fn update_spec_bindings_replaces_wholesale() {
        let mut conn = test_conn();
        let id1 = DbChannel.create_instance(&conn, &sample_input("s", "One")).unwrap();
        let id2 = DbChannel.create_instance(&conn, &sample_input("s", "Two")).unwrap();
        DbChannel
            .update_spec_bindings(
                &mut conn,
                "spec",
                &[SpecChannelBinding {
                    channel_instance_id: id1,
                    enabled: true,
                    channel_name: None,
                    channel_type: None,
                }],
            )
            .unwrap();
        // Replace with a different single binding — the first must be gone.
        DbChannel
            .update_spec_bindings(
                &mut conn,
                "spec",
                &[SpecChannelBinding {
                    channel_instance_id: id2.clone(),
                    enabled: false,
                    channel_name: None,
                    channel_type: None,
                }],
            )
            .unwrap();
        let bindings = DbChannel.list_spec_bindings(&conn, "spec").unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].channel_instance_id, id2);
    }

    #[test]
    fn ilink_base_url_falls_back_when_absent_and_errors_when_missing() {
        let conn = test_conn();
        // Missing instance → NotFound.
        let err = DbChannel.instance_ilink_base_url(&conn, "nope").unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));

        // Instance with no base_url in config → the ilink default.
        let mut input = sample_input("s", "NoBase");
        input.config = serde_json::json!({});
        let id = DbChannel.create_instance(&conn, &input).unwrap();
        let url = DbChannel.instance_ilink_base_url(&conn, &id).unwrap();
        assert_eq!(url, crate::channels::im::ilink_binding::ILINK_BASE_URL);

        // Instance with an explicit base_url → that value.
        let id2 = DbChannel
            .create_instance(&conn, &sample_input("s", "WithBase"))
            .unwrap();
        let url2 = DbChannel.instance_ilink_base_url(&conn, &id2).unwrap();
        assert_eq!(url2, "https://im.example.com");
    }

    #[test]
    fn save_ilink_token_merges_config_and_disconnect_clears_it() {
        let conn = test_conn();
        let id = DbChannel
            .create_instance(&conn, &sample_input("s", "WeChat"))
            .unwrap();

        DbChannel
            .save_ilink_token(&conn, &id, "tok-123", "acct-7")
            .unwrap();
        let row = &DbChannel.list_instances(&conn, None).unwrap()[0];
        // account_id merged in, base_url preserved.
        assert_eq!(row.config["account_id"], "acct-7");
        assert_eq!(row.config["base_url"], "https://im.example.com");

        DbChannel.disconnect_ilink(&conn, &id).unwrap();
        let row = &DbChannel.list_instances(&conn, None).unwrap()[0];
        assert!(
            row.config.get("account_id").is_none(),
            "disconnect must drop account_id from config"
        );
        assert_eq!(row.config["base_url"], "https://im.example.com");

        // Missing instance → NotFound for both ops.
        assert!(matches!(
            DbChannel.save_ilink_token(&conn, "nope", "t", "a").unwrap_err(),
            Error::NotFound(_)
        ));
        assert!(matches!(
            DbChannel.disconnect_ilink(&conn, "nope").unwrap_err(),
            Error::NotFound(_)
        ));
    }

    #[test]
    fn update_spec_im_settings_patches_only_provided_fields() {
        let conn = test_conn();
        // Seed an automation_specs row. The post-V20 schema requires the
        // NOT-NULL-without-default columns below; V32b later adds the IM columns
        // (trigger_phrase / system_prompt_override) this method patches.
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO automation_specs \
             (id, name, version, author, description, system_prompt, spec_yaml, spec_json, \
              created_at, updated_at) \
             VALUES (?1, ?2, '1', 'me', 'desc', 'sys', 'yaml', '{}', ?3, ?3)",
            rusqlite::params!["spec-im", "S", now],
        )
        .unwrap();

        // Set only trigger_phrase; system_prompt_override stays NULL.
        DbChannel
            .update_spec_im_settings(&conn, "spec-im", Some("/go"), None)
            .unwrap();
        let (tp, spo): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT trigger_phrase, system_prompt_override FROM automation_specs WHERE id=?1",
                ["spec-im"],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(tp.as_deref(), Some("/go"));
        assert_eq!(spo, None, "untouched field stays NULL");

        // Now set only the override; trigger_phrase must be preserved.
        DbChannel
            .update_spec_im_settings(&conn, "spec-im", None, Some("be terse"))
            .unwrap();
        let (tp, spo): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT trigger_phrase, system_prompt_override FROM automation_specs WHERE id=?1",
                ["spec-im"],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(tp.as_deref(), Some("/go"), "prior field preserved");
        assert_eq!(spo.as_deref(), Some("be terse"));
    }
}
