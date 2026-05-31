//! Channel/IM-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31) this is a **mixed** domain:
//!
//! - The legacy channel commands (`list_channels` / `add_channel` /
//!   `remove_channel` / `toggle_channel`) and the runtime accessors
//!   (`get_im_channel_statuses`) delegate to the in-memory
//!   `state.channel_manager` / `state.im_channel_manager` — no SQL → thin move.
//! - The IM-instance CRUD, ilink (WeChat) token/QR config plumbing, spec↔channel
//!   bindings, and per-spec IM settings all carry inline `state.db` SQL → that
//!   logic is lifted into [`crate::services::channel_service::DbChannel`]. The
//!   commands here lock `state.db`, call the service, then perform the
//!   non-DB side effects that must stay in the command (async
//!   `im_channel_manager` restarts/stops, the HTTP QR fetch/poll).
//!
//! The wire types (`ImChannelInput`, `ImChannelRow`, `SpecChannelBinding`) live
//! with the service (it reads/writes them) and are re-imported here. The SSRF
//! URL validation moved into the service's `create`/`update` (the guard belongs
//! with the write).

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{ChannelInfo, ChannelInput};
use crate::services::channel_service::{
    ChannelService, DbChannel, ImChannelInput, ImChannelRow, SpecChannelBinding,
};

// ─── Legacy channel manager (in-memory, no SQL) ──────────────────────────

#[tauri::command]
pub async fn list_channels(state: State<'_, AppState>) -> Result<Vec<ChannelInfo>, Error> {
    let mgr = state.channel_manager.read().await;
    Ok(mgr.list().into_iter().map(|c| ChannelInfo {
        id: c.id.clone(),
        name: c.name.clone(),
        channel_type: match c.channel_type {
            crate::channels::ChannelType::Webhook => "webhook",
            crate::channels::ChannelType::Email => "email",
            crate::channels::ChannelType::WeChat => "wechat",
            crate::channels::ChannelType::DingTalk => "dingtalk",
            crate::channels::ChannelType::Feishu => "feishu",
            crate::channels::ChannelType::Custom => "custom",
        }.into(),
        enabled: c.enabled,
        webhook_url: c.webhook_url.clone(),
    }).collect())
}

#[tauri::command]
pub async fn add_channel(state: State<'_, AppState>, input: ChannelInput) -> Result<ChannelInfo, Error> {
    let channel_type = match input.channel_type.as_str() {
        "webhook" => crate::channels::ChannelType::Webhook,
        "email" => crate::channels::ChannelType::Email,
        "wechat" => crate::channels::ChannelType::WeChat,
        "dingtalk" => crate::channels::ChannelType::DingTalk,
        "feishu" => crate::channels::ChannelType::Feishu,
        _ => crate::channels::ChannelType::Custom,
    };
    let config = crate::channels::ChannelConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: input.name.clone(),
        channel_type: channel_type.clone(),
        enabled: true,
        webhook_url: input.webhook_url.clone(),
        config: input.config.clone(),
    };
    let id = config.id.clone();
    let mut mgr = state.channel_manager.write().await;
    mgr.add_channel(config);
    Ok(ChannelInfo {
        id,
        name: input.name,
        channel_type: input.channel_type,
        enabled: true,
        webhook_url: input.webhook_url,
    })
}

#[tauri::command]
pub async fn remove_channel(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let mut mgr = state.channel_manager.write().await;
    Ok(mgr.remove_channel(&id).is_some())
}

#[tauri::command]
pub async fn toggle_channel(state: State<'_, AppState>, id: String, enabled: bool) -> Result<bool, Error> {
    let mut mgr = state.channel_manager.write().await;
    Ok(mgr.set_enabled(&id, enabled))
}

// ─── IM Channel Instance CRUD ────────────────────────────────────────────

#[tauri::command]
pub async fn list_im_channels(
    state: tauri::State<'_, AppState>,
    space_id: Option<String>,
) -> Result<Vec<ImChannelRow>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
    DbChannel.list_instances(&conn, space_id.as_deref())
}

#[tauri::command]
pub async fn get_im_channel_statuses(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::channels::types::ChannelRuntimeStatus>, Error> {
    Ok(state.im_channel_manager.get_all_statuses().await)
}

#[tauri::command]
pub async fn create_im_channel(
    state: tauri::State<'_, AppState>,
    input: ImChannelInput,
) -> Result<String, Error> {
    let id = {
        let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
        DbChannel.create_instance(&conn, &input)?
    };
    let _ = state.im_channel_manager.restart_instance_by_id(&id).await;
    Ok(id)
}

#[tauri::command]
pub async fn update_im_channel(
    state: tauri::State<'_, AppState>,
    id: String,
    input: ImChannelInput,
) -> Result<(), Error> {
    {
        let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
        DbChannel.update_instance(&conn, &id, &input)?;
    }
    let _ = state.im_channel_manager.restart_instance_by_id(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn delete_im_channel(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), Error> {
    {
        let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
        DbChannel.delete_instance(&conn, &id)?;
    } // conn lock dropped here
    state.im_channel_manager.stop_instance(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn toggle_im_channel(
    state: tauri::State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), Error> {
    {
        let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
        DbChannel.set_instance_enabled(&conn, &id, enabled)?;
    } // lock dropped
    state
        .im_channel_manager
        .restart_instance_by_id(&id)
        .await
        .map_err(|e| Error::Internal(e))?;
    Ok(())
}

#[tauri::command]
pub async fn request_wechat_ilink_qrcode(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<serde_json::Value, Error> {
    let base_url = {
        let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
        DbChannel.instance_ilink_base_url(&conn, &instance_id)?
    };
    let info = crate::channels::im::ilink_binding::fetch_qr(&base_url)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(serde_json::json!({
        "qrcode": info.qrcode,
        "qrcode_img_content": info.qrcode_img_content,
    }))
}

#[tauri::command]
pub async fn poll_wechat_ilink_qrcode_status(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    qrcode: String,
) -> Result<serde_json::Value, Error> {
    let base_url = {
        let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
        DbChannel.instance_ilink_base_url(&conn, &instance_id)?
    };
    let status = crate::channels::im::ilink_binding::poll_qr_status(&base_url, &qrcode)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(serde_json::to_value(&status).unwrap_or_default())
}

/// Save bot_token to credentials_json and account_id to config_json, then restart instance.
#[tauri::command]
pub async fn save_wechat_ilink_token(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    bot_token: String,
    account_id: String,
) -> Result<(), Error> {
    if bot_token.trim().is_empty() || account_id.trim().is_empty() {
        return Err(Error::Validation("bot_token and account_id cannot be empty".to_string()));
    }
    {
        let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
        DbChannel.save_ilink_token(&conn, &instance_id, &bot_token, &account_id)?;
    }
    state
        .im_channel_manager
        .restart_instance_by_id(&instance_id)
        .await
        .map_err(|e| Error::Internal(e))?;
    Ok(())
}

/// Clear bot_token from credentials and account_id from config, then restart instance.
#[tauri::command]
pub async fn disconnect_wechat_ilink(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<(), Error> {
    {
        let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
        DbChannel.disconnect_ilink(&conn, &instance_id)?;
    }
    state
        .im_channel_manager
        .restart_instance_by_id(&instance_id)
        .await
        .map_err(|e| Error::Internal(e))?;
    Ok(())
}

// ─── Spec-Channel Bindings ───────────────────────────────────────────────

#[tauri::command]
pub async fn list_spec_channel_bindings(
    state: tauri::State<'_, AppState>,
    spec_id: String,
) -> Result<Vec<SpecChannelBinding>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
    DbChannel.list_spec_bindings(&conn, &spec_id)
}

#[tauri::command]
pub async fn update_spec_channel_bindings(
    state: tauri::State<'_, AppState>,
    spec_id: String,
    bindings: Vec<SpecChannelBinding>,
) -> Result<(), Error> {
    let mut conn = state.db.lock().map_err(|e| Error::Internal(e.to_string()))?;
    DbChannel.update_spec_bindings(&mut conn, &spec_id, &bindings)
}

/// Update per-spec IM settings: trigger_phrase and system_prompt_override.
#[tauri::command]
pub async fn update_spec_im_settings(
    state: State<'_, AppState>,
    spec_id: String,
    trigger_phrase: Option<String>,
    system_prompt_override: Option<String>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    DbChannel
        .update_spec_im_settings(
            &conn,
            &spec_id,
            trigger_phrase.as_deref(),
            system_prompt_override.as_deref(),
        )
        .map_err(|e| e.to_string())
}
