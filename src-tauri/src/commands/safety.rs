//! Safety-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), these have **no service**: every
//! command delegates to the in-memory [`crate::safety::SafetyManager`] held in
//! `state.safety_manager` (an async `RwLock`). The manager *is* the logic
//! holder — there is no inline SQL or business logic to lift — so the JUDGMENT
//! RULE resolves to a thin move. The commands just lock the manager, mutate or
//! read the policy, and serialize it to [`SafetyPolicyResponse`].
//!
//! `parse_safety_mode` is shared with the Chat domain and stays in
//! `tauri_commands.rs` (imported here). `safety_mode_to_str` was Safety-only and
//! lives here as a module-private helper.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;
use crate::ipc::{
    AssessCommandInput, CommandRiskResponse, SafetyPolicyResponse, SetSafetyModeInput,
    SetToolOverrideInput, ToolNameInput,
};
use crate::safety::SafetyMode;
use crate::tauri_commands::parse_safety_mode;

/// Serialize a [`crate::safety::SafetyPolicy`] into the wire response. Shared by
/// every mutating command (all of which return the resulting policy) plus
/// `get_safety_policy`.
fn policy_response(policy: &crate::safety::SafetyPolicy) -> SafetyPolicyResponse {
    SafetyPolicyResponse {
        global_mode: safety_mode_to_str(&policy.global_mode).to_string(),
        tool_overrides: policy
            .tool_overrides
            .iter()
            .map(|(k, v)| (k.clone(), safety_mode_to_str(v).to_string()))
            .collect(),
        auto_approved_tools: policy.auto_approved_tools.iter().cloned().collect(),
        blocked_tools: policy.blocked_tools.iter().cloned().collect(),
    }
}

/// Render a [`SafetyMode`] as its UI string. Inverse of `parse_safety_mode`.
fn safety_mode_to_str(mode: &SafetyMode) -> &'static str {
    match mode {
        SafetyMode::Ask => "ask",
        SafetyMode::AcceptEdits => "acceptedits",
        SafetyMode::Plan => "plan",
        SafetyMode::Supervised => "supervised",
        SafetyMode::Yolo => "yolo",
    }
}

/// Read the current safety policy.
#[tauri::command]
pub async fn get_safety_policy(state: State<'_, AppState>) -> Result<SafetyPolicyResponse, Error> {
    let mgr = state.safety_manager.read().await;
    Ok(policy_response(mgr.policy()))
}

/// Set the global safety mode.
#[tauri::command]
pub async fn set_safety_mode(
    state: State<'_, AppState>,
    input: SetSafetyModeInput,
) -> Result<SafetyPolicyResponse, Error> {
    let mode = parse_safety_mode(&input.mode)?;
    let mut mgr = state.safety_manager.write().await;
    mgr.set_global_mode(mode)?;
    Ok(policy_response(mgr.policy()))
}

/// Override the safety mode for a single tool.
#[tauri::command]
pub async fn set_tool_safety_override(
    state: State<'_, AppState>,
    input: SetToolOverrideInput,
) -> Result<SafetyPolicyResponse, Error> {
    let mode = parse_safety_mode(&input.mode)?;
    let mut mgr = state.safety_manager.write().await;
    mgr.set_tool_override(&input.tool_name, mode)?;
    Ok(policy_response(mgr.policy()))
}

/// Remove a per-tool safety override.
#[tauri::command]
pub async fn remove_tool_safety_override(
    state: State<'_, AppState>,
    input: ToolNameInput,
) -> Result<SafetyPolicyResponse, Error> {
    let mut mgr = state.safety_manager.write().await;
    mgr.remove_tool_override(&input.tool_name)?;
    Ok(policy_response(mgr.policy()))
}

/// Add a tool to the auto-approved whitelist.
#[tauri::command]
pub async fn add_auto_approved_tool(
    state: State<'_, AppState>,
    input: ToolNameInput,
) -> Result<SafetyPolicyResponse, Error> {
    let mut mgr = state.safety_manager.write().await;
    mgr.add_auto_approved(&input.tool_name)?;
    Ok(policy_response(mgr.policy()))
}

/// Remove a tool from the auto-approved whitelist.
#[tauri::command]
pub async fn remove_auto_approved_tool(
    state: State<'_, AppState>,
    input: ToolNameInput,
) -> Result<SafetyPolicyResponse, Error> {
    let mut mgr = state.safety_manager.write().await;
    mgr.remove_auto_approved(&input.tool_name)?;
    Ok(policy_response(mgr.policy()))
}

/// Block a tool entirely.
#[tauri::command]
pub async fn block_tool(
    state: State<'_, AppState>,
    input: ToolNameInput,
) -> Result<SafetyPolicyResponse, Error> {
    let mut mgr = state.safety_manager.write().await;
    mgr.block_tool(&input.tool_name)?;
    Ok(policy_response(mgr.policy()))
}

/// Unblock a previously blocked tool.
#[tauri::command]
pub async fn unblock_tool(
    state: State<'_, AppState>,
    input: ToolNameInput,
) -> Result<SafetyPolicyResponse, Error> {
    let mut mgr = state.safety_manager.write().await;
    mgr.unblock_tool(&input.tool_name)?;
    Ok(policy_response(mgr.policy()))
}

/// Assess the risk level of a shell command without executing it.
#[tauri::command]
pub async fn assess_command_risk(
    state: State<'_, AppState>,
    input: AssessCommandInput,
) -> Result<CommandRiskResponse, Error> {
    let mgr = state.safety_manager.read().await;
    let assessment = mgr.assess_command_risk(&input.command);
    let suggested = match &assessment.suggested_action {
        crate::safety::ApprovalDecision::AutoApprove => "auto_approve".to_string(),
        crate::safety::ApprovalDecision::RequireApproval { .. } => "require_approval".to_string(),
        crate::safety::ApprovalDecision::Block { .. } => "block".to_string(),
    };
    Ok(CommandRiskResponse {
        level: format!("{:?}", assessment.level).to_lowercase(),
        reasons: assessment.reasons,
        suggested_action: suggested,
    })
}
