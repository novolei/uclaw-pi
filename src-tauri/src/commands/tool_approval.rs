//! Tool-approval-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), these have **no new service**:
//!   - `approve_tool_call` is pure in-memory orchestration — it resolves the
//!     PiEngine's pending approval (via the `engine` handle), optionally adds the
//!     tool to `state.safety_manager`'s auto-approved whitelist, and resolves the
//!     legacy uClaw oneshot via `state.pending_approvals`. No SQL.
//!   - `list_permission_rules` / `create_permission_rule` / `delete_permission_rule`
//!     / `list_permission_audit` already delegate to [`crate::safety::permissions`]
//!     (which takes `&state.db` and owns the SQL) — that module *is* the service
//!     layer, so these stay thin pass-throughs.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{
    ApproveToolCallInput, ApproveToolCallResponse, CreatePermissionRuleInput, PermissionAuditEntry,
    PermissionRule,
};

/// Resolve a pending tool-call approval (both the pi-engine path and the legacy
/// uClaw oneshot path — idempotent across the two), optionally promoting the tool
/// to the auto-approved whitelist when `always_allow` is set.
#[tauri::command]
pub async fn approve_tool_call(
    state: State<'_, AppState>,
    _app_handle: tauri::AppHandle,
    engine: State<'_, std::sync::Arc<uclaw_pi_engine::PiEngine>>,
    input: ApproveToolCallInput,
) -> Result<ApproveToolCallResponse, Error> {
    tracing::info!(
        session_id = %input.session_id,
        tool_id = %input.tool_id,
        approved = input.approved,
        always_allow = ?input.always_allow,
        tool_name = ?input.tool_name,
        "Tool approval response received"
    );

    // [R3 交互] Resolve the PiEngine's pending approval keyed by tool_call_id.
    // Idempotent with the legacy uClaw approval flow below — the engine registry
    // only holds this request when the pi path raised it (UCLAW_PI_ENGINE on).
    if crate::engine_sink::pi_engine_enabled() {
        engine.send(uclaw_pi_engine::EngineCmd::Respond {
            request_id: input.tool_id.clone(),
            allow: input.approved,
            reason: None,
        });
    }

    // If approved with always_allow, add tool to auto-approved whitelist immediately
    if input.approved {
        if input.always_allow.unwrap_or(false) {
            if let Some(ref tool_name) = input.tool_name {
                let mut mgr = state.safety_manager.write().await;
                let _ = mgr.add_auto_approved(tool_name);
                tracing::info!(tool_name = %tool_name, "Tool added to auto-approved whitelist via always_allow");
            }
        }
    }

    // Resolve the pending approval via oneshot channel
    let result = crate::app::ApprovalResult {
        approved: input.approved,
        always_allow: input.always_allow.unwrap_or(false),
        tool_name: input.tool_name.clone(),
        path_scope: input.path_scope.clone(),
        paths: input.paths.clone(),
    };

    let resolved = state.pending_approvals.resolve(&input.tool_id, result);
    if !resolved {
        tracing::warn!(tool_id = %input.tool_id, "No pending approval found for tool_id");
    }

    Ok(ApproveToolCallResponse { success: resolved })
}

/// List all persisted permission rules.
#[tauri::command]
pub async fn list_permission_rules(
    state: State<'_, AppState>,
) -> Result<Vec<PermissionRule>, Error> {
    crate::safety::permissions::list_rules(&state.db)
        .map_err(|e| Error::Internal(format!("list_permission_rules: {}", e)))
}

/// Create a new permission rule.
#[tauri::command]
pub async fn create_permission_rule(
    state: State<'_, AppState>,
    input: CreatePermissionRuleInput,
) -> Result<PermissionRule, Error> {
    crate::safety::permissions::create_rule(&state.db, input)
        .map_err(|e| Error::Internal(format!("create_permission_rule: {}", e)))
}

/// Delete a permission rule by id. Returns whether a row was removed.
#[tauri::command]
pub async fn delete_permission_rule(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    crate::safety::permissions::delete_rule(&state.db, &id)
        .map_err(|e| Error::Internal(format!("delete_permission_rule: {}", e)))
}

/// List permission audit entries, optionally filtered by session.
#[tauri::command]
pub async fn list_permission_audit(
    state: State<'_, AppState>,
    session_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<PermissionAuditEntry>, Error> {
    crate::safety::permissions::list_audit(&state.db, session_id.as_deref(), limit.unwrap_or(100))
        .map_err(|e| Error::Internal(format!("list_permission_audit: {}", e)))
}
