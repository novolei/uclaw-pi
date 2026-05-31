//! MCP-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), the bulk of this domain has **no
//! service**: every server-lifecycle command delegates to the in-memory
//! [`crate::mcp::McpManager`] held in `state.mcp_manager` (a `SharedMcpManager`
//! async `RwLock`). The manager *is* the logic holder — config CRUD, connection
//! lifecycle, tool listing, ping all live there — so the JUDGMENT RULE resolves
//! to a thin move for those.
//!
//! The **one** exception is [`list_mcp_audit`], which reads the `mcp_audit` table
//! directly (not the manager). Its SQL is lifted into
//! [`crate::services::mcp_audit_service`]; the command here just clamps the
//! limit, hops onto `spawn_blocking`, locks `state.db`, and delegates.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{McpServerInfo, McpServerInput};
use crate::services::mcp_audit_service::{DbMcpAudit, McpAuditService};

#[tauri::command]
pub async fn list_mcp_servers(state: State<'_, AppState>) -> Result<Vec<McpServerInfo>, Error> {
    let mgr = state.mcp_manager.read().await;
    let statuses: std::collections::HashMap<String, (crate::mcp::McpServerStatus, Option<String>)> =
        mgr.all_server_statuses()
            .into_iter()
            .map(|(id, st, err)| (id, (st, err)))
            .collect();
    Ok(mgr
        .all_servers()
        .into_iter()
        .map(|c| {
            let (status_enum, err) = statuses
                .get(&c.id)
                .cloned()
                .unwrap_or((crate::mcp::McpServerStatus::Disconnected, None));
            let status = match status_enum {
                crate::mcp::McpServerStatus::Disconnected => "disconnected",
                crate::mcp::McpServerStatus::Connecting => "connecting",
                crate::mcp::McpServerStatus::Connected => "connected",
                crate::mcp::McpServerStatus::Error => "error",
            };
            McpServerInfo {
                id: c.id.clone(),
                name: c.name.clone(),
                description: c.description.clone(),
                transport_type: c.transport_type.clone(),
                command: c.command.clone(),
                args: c.args.clone(),
                env: Some(c.env.clone()),
                url: c.url.clone(),
                enabled: c.enabled,
                auto_approve: c.auto_approve,
                error_message: err,
                status: status.into(),
            }
        })
        .collect())
}

#[tauri::command]
pub async fn add_mcp_server(
    state: State<'_, AppState>,
    input: McpServerInput,
) -> Result<McpServerInfo, Error> {
    let config = crate::mcp::McpServerConfig {
        id: input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: input.name.clone(),
        description: input.description.clone(),
        transport_type: input.transport_type.clone().unwrap_or_default(),
        command: input.command.clone(),
        args: input.args.clone().unwrap_or_default(),
        env: input.env.clone().unwrap_or_default(),
        url: input.url.clone(),
        enabled: true,
        auto_approve: input.auto_approve.unwrap_or(false),
        tool_allowlist: None,
    };
    let mut mgr = state.mcp_manager.write().await;
    mgr.add_server(config.clone()).map_err(Error::InvalidInput)?;
    Ok(McpServerInfo {
        id: config.id,
        name: config.name,
        description: config.description,
        transport_type: config.transport_type,
        command: config.command,
        args: config.args,
        env: Some(config.env),
        url: config.url,
        enabled: config.enabled,
        auto_approve: config.auto_approve,
        error_message: None,
        status: "disconnected".into(),
    })
}

#[tauri::command]
pub async fn update_mcp_server(
    state: State<'_, AppState>,
    id: String,
    input: McpServerInput,
) -> Result<McpServerInfo, Error> {
    let mut mgr = state.mcp_manager.write().await;
    // 保留 enabled —— 编辑表单不拥有这个状态(卡片/抽屉的开关才管它)。
    let enabled = mgr
        .all_servers()
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| c.enabled)
        .ok_or_else(|| Error::NotFound(format!("MCP server '{}' not found", id)))?;
    let config = crate::mcp::McpServerConfig {
        id: id.clone(),
        name: input.name.clone(),
        description: input.description.clone(),
        transport_type: input.transport_type.clone().unwrap_or_default(),
        command: input.command.clone(),
        args: input.args.clone().unwrap_or_default(),
        env: input.env.clone().unwrap_or_default(),
        url: input.url.clone(),
        enabled,
        auto_approve: input.auto_approve.unwrap_or(false),
        tool_allowlist: None,
    };
    mgr.update_server(&id, config.clone())
        .map_err(Error::InvalidInput)?;
    // update_server only rewrites config — read the actual in-memory status
    // so the return value isn't stale for an already-connected server.
    let (actual_status, actual_err) = mgr
        .all_server_statuses()
        .into_iter()
        .find(|(sid, _, _)| sid == &id)
        .map(|(_, st, err)| (st, err))
        .unwrap_or((crate::mcp::McpServerStatus::Disconnected, None));
    let status = match actual_status {
        crate::mcp::McpServerStatus::Disconnected => "disconnected",
        crate::mcp::McpServerStatus::Connecting => "connecting",
        crate::mcp::McpServerStatus::Connected => "connected",
        crate::mcp::McpServerStatus::Error => "error",
    };
    Ok(McpServerInfo {
        id: config.id,
        name: config.name,
        description: config.description,
        transport_type: config.transport_type,
        command: config.command,
        args: config.args,
        env: Some(config.env),
        url: config.url,
        enabled: config.enabled,
        auto_approve: config.auto_approve,
        error_message: actual_err,
        status: status.into(),
    })
}

#[tauri::command]
pub async fn remove_mcp_server(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let mut mgr = state.mcp_manager.write().await;
    Ok(mgr.remove_server(&id).is_some())
}

#[tauri::command]
pub async fn toggle_mcp_server(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<bool, Error> {
    let mut mgr = state.mcp_manager.write().await;
    Ok(mgr.set_enabled(&id, enabled))
}

#[tauri::command]
pub async fn connect_mcp_server(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let shared = state.mcp_manager.clone();
    crate::mcp::connect_server_shared(&state.mcp_manager, &id)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    // PR-3 — spawn the per-server health loop now that we're
    // connected. The loop is idempotent: if one's already running for
    // this id (e.g. restart path) it gets aborted first.
    state.mcp_manager.write().await.start_health_loop(shared, &id);
    Ok(true)
}

#[tauri::command]
pub async fn disconnect_mcp_server(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let mut mgr = state.mcp_manager.write().await;
    mgr.disconnect_server(&id)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(true)
}

#[tauri::command]
pub async fn restart_mcp_server(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let shared = state.mcp_manager.clone();
    crate::mcp::restart_server_shared(&state.mcp_manager, &id)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    // PR-3 — restart_server_shared's inner disconnect aborted the old loop;
    // start a fresh one now that we're connected again.
    state.mcp_manager.write().await.start_health_loop(shared, &id);
    Ok(true)
}

/// MCP PR-2 — manually re-fetch the tools/list from a connected server.
/// Useful when an MCP server adds a tool while uClaw is running (the
/// notification path is not yet wired — see PR-4). Returns the fresh
/// tool defs so the UI can re-render without a follow-up list call.
#[tauri::command]
pub async fn refresh_mcp_tools(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<serde_json::Value>, Error> {
    let mut mgr = state.mcp_manager.write().await;
    let tools = mgr
        .refresh_tools(&id)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(tools
        .into_iter()
        .map(|t| {
            serde_json::json!({
                "serverId": t.server_id,
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            })
        })
        .collect())
}

/// MCP PR-2 — JSON-RPC ping a connected server. Returns the elapsed
/// time in milliseconds so the UI can show a "round-trip 23ms ✓"
/// success state. Distinct from `restart_mcp_server` because it
/// doesn't tear down the transport — just verifies the connection is
/// alive end-to-end.
#[tauri::command]
pub async fn ping_mcp_server(state: State<'_, AppState>, id: String) -> Result<u64, Error> {
    let start = std::time::Instant::now();
    let mgr = state.mcp_manager.read().await;
    mgr.ping_server(&id)
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    Ok(start.elapsed().as_millis() as u64)
}

/// MCP PR-5 — read the audit log. `server_id=None` returns all rows,
/// most recent first, capped at `limit` (default 100, ceiling 1000 to
/// keep IPC payloads bounded). Values are always env-redacted. The SQL
/// lives in [`crate::services::mcp_audit_service`]; this command clamps
/// the limit, hops onto a blocking thread, and delegates.
#[tauri::command]
pub async fn list_mcp_audit(
    state: State<'_, AppState>,
    server_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<crate::mcp::McpAuditEntry>, Error> {
    let cap = limit.unwrap_or(100).clamp(1, 1000) as i64;
    let db = state.db.clone();
    let rows = tokio::task::spawn_blocking(
        move || -> Result<Vec<crate::mcp::McpAuditEntry>, String> {
            let conn = db.lock().map_err(|e| format!("DB lock: {}", e))?;
            DbMcpAudit.list(&conn, server_id.as_deref(), cap)
        },
    )
    .await
    .map_err(|e| Error::Internal(format!("spawn_blocking: {}", e)))?
    .map_err(Error::Internal)?;
    Ok(rows)
}

#[tauri::command]
pub async fn list_mcp_tools(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, Error> {
    let mgr = state.mcp_manager.read().await;
    Ok(mgr
        .all_tools()
        .into_iter()
        .map(|t| {
            serde_json::json!({
                "serverId": t.server_id,
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            })
        })
        .collect())
}
