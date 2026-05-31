//! Browser-runtime-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), this domain has **no service**:
//! every command delegates to an in-memory manager/supervisor held in
//! [`crate::app::AppState`] — the [`crate::browser::context::BrowserContextManager`]
//! (`state.browser_context_manager`), the Browser Runtime status service
//! (`state.browser_runtime_status_service`), and the automation
//! [`crate::automation::runtime::AppRuntimeService`] (`state.runtime_service`,
//! used only by the login-completion persistence path). Those managers *are* the
//! logic holders — there is **no inline `state.db` SQL to lift** — so the
//! JUDGMENT RULE resolves to a thin move for the whole section.
//!
//! Relocated verbatim from the legacy `tauri_commands.rs` god file: the 20
//! `#[tauri::command]`s plus every Browser-only helper, type, constant, and
//! inline `#[cfg(test)]` module — none of which had any caller outside this
//! domain. The file is named `browser_cmds` (not `browser`) to avoid colliding
//! with the crate-level [`crate::browser`] module it leans on.

use std::sync::Arc;

use tauri::{Emitter, Manager, State};

use crate::app::AppState;
use crate::browser::action::{BrowserAction, BrowserActionResult};
use crate::browser::provider_execution::{
    BrowserProviderActionExecution, BrowserProviderActionExecutionOutcome,
    BrowserProviderActionExecutor, BrowserProviderActionRouteOptions,
};
use crate::browser::runtime_execution::route_options_from_runtime_status;
use crate::error::Error;

async fn browser_ui_runtime_route_options(
    state: &AppState,
    command_name: &'static str,
) -> BrowserProviderActionRouteOptions {
    let provider_config = state.settings.read().await.browser_runtime_provider_config.clone();
    match state
        .browser_runtime_status_service
        .inspect_with_provider_config(provider_config)
        .await
    {
        Ok(status) => {
            tracing::debug!(
                command_name,
                supervisor_state = ?status.supervisor.runtime_state,
                doctor_status = ?status.supervisor.doctor_status,
                active_context_count = status.supervisor.active_context_count,
                runtime_ready = status.runtime_pack.ready,
                can_run_browser_tasks = status.runtime_pack.can_run_browser_tasks,
                "Browser UI command inspected Browser Runtime status before execution"
            );
            route_options_from_runtime_status(status).with_mcp_manager(state.mcp_manager.clone())
        }
        Err(error) => {
            tracing::warn!(
                command_name,
                error = %error,
                "Browser UI command could not inspect Browser Runtime status; using default provider route options"
            );
            BrowserProviderActionRouteOptions::default().with_mcp_manager(state.mcp_manager.clone())
        }
    }
}

async fn touch_browser_ui_runtime_status(state: &AppState, command_name: &'static str) {
    let _ = browser_ui_runtime_route_options(state, command_name).await;
}

async fn execute_browser_ui_provider_action(
    state: &AppState,
    command_name: &'static str,
    session_id: &str,
    action: BrowserAction,
) -> Result<BrowserActionResult, String> {
    let route_options = browser_ui_runtime_route_options(state, command_name).await;
    let executor = BrowserProviderActionExecutor::new(Arc::clone(&state.browser_context_manager))
        .with_route_options(route_options);
    let route_decision = executor.route_action(&action);
    tracing::debug!(
        command_name,
        provider_route_status = ?route_decision.status,
        selected_provider_id = ?route_decision.selected_provider_id,
        "Browser UI command routed through BrowserProviderActionExecutor"
    );
    let execution = executor
        .execute_routed_with_identity(session_id, None, action, route_decision)
        .await
        .map_err(|error| error.to_string())?;
    browser_provider_action_result_or_error(execution)
}

fn browser_provider_action_result_or_error(
    execution: BrowserProviderActionExecution,
) -> Result<BrowserActionResult, String> {
    match execution.outcome {
        BrowserProviderActionExecutionOutcome::Executed(result) if result.ok => Ok(result),
        BrowserProviderActionExecutionOutcome::Executed(result) => Err(result
            .error
            .or(result.message)
            .unwrap_or_else(|| format!("{} failed", result.action_name))),
        BrowserProviderActionExecutionOutcome::Blocked(blocked) => Err(blocked.message),
    }
}

#[cfg(test)]
mod browser_ui_runtime_command_tests {
    use super::*;
    use crate::browser::provider::{
        BrowserProviderRouteDecision, BrowserProviderRouteDecisionStatus,
    };

    fn route_decision() -> BrowserProviderRouteDecision {
        BrowserProviderRouteDecision {
            status: BrowserProviderRouteDecisionStatus::Selected,
            selected_provider_id: Some("local_chromium".to_string()),
            candidates: Vec::new(),
            event_intents: Vec::new(),
            skipped_providers: Vec::new(),
        }
    }

    #[test]
    fn browser_provider_action_result_helper_returns_success() {
        let execution = BrowserProviderActionExecution {
            route_decision: route_decision(),
            outcome: BrowserProviderActionExecutionOutcome::Executed(
                BrowserActionResult::success("browser_ui_navigate", Some("ok".to_string())),
            ),
        };

        let result = browser_provider_action_result_or_error(execution).expect("success result");

        assert_eq!(result.action_name, "browser_ui_navigate");
        assert!(result.ok);
    }

    #[test]
    fn browser_provider_action_result_helper_surfaces_failure_message() {
        let execution = BrowserProviderActionExecution {
            route_decision: route_decision(),
            outcome: BrowserProviderActionExecutionOutcome::Executed(
                BrowserActionResult::failure("browser_ui_navigate", "route failed".to_string()),
            ),
        };

        let error = browser_provider_action_result_or_error(execution).expect_err("error result");

        assert_eq!(error, "route failed");
    }
}

const LEGACY_BROWSER_COMPAT_SESSION_ID: &str = "legacy-browser-service";

async fn touch_legacy_browser_runtime_status(
    state: &AppState,
    command: &'static str,
) -> Result<(), Error> {
    let provider_config = state.settings.read().await.browser_runtime_provider_config.clone();
    state
        .browser_runtime_status_service
        .inspect_with_provider_config(provider_config)
        .await
        .map(|_| ())
        .map_err(|error| {
            tracing::warn!(
                command,
                error = %error,
                "Browser Runtime status unavailable for legacy browser command"
            );
            error
        })
}

fn legacy_browser_state_from_tabs(
    running: bool,
    tabs: Vec<crate::browser::types::TabInfo>,
) -> crate::browser::types::BrowserState {
    let active_tab_id = tabs
        .iter()
        .find(|tab| tab.active)
        .or_else(|| tabs.first())
        .map(|tab| tab.tab_id.clone());
    let tabs = tabs
        .into_iter()
        .map(|tab| crate::browser::types::BrowserTab {
            tab_id: tab.tab_id,
            url: tab.url,
            title: tab.title,
        })
        .collect();

    crate::browser::types::BrowserState {
        running,
        tabs,
        active_tab_id,
    }
}

async fn legacy_browser_state(
    state: &AppState,
) -> Result<crate::browser::types::BrowserState, Error> {
    if !state
        .browser_context_manager
        .has_context(LEGACY_BROWSER_COMPAT_SESSION_ID)
        .await
    {
        return Ok(legacy_browser_state_from_tabs(false, Vec::new()));
    }

    let ctx = state
        .browser_context_manager
        .get_or_create(LEGACY_BROWSER_COMPAT_SESSION_ID)
        .await
        .map_err(|error| Error::Internal(error.to_string()))?;

    Ok(legacy_browser_state_from_tabs(true, ctx.get_all_tabs().await))
}

#[tauri::command]
pub async fn browser_get_state(
    state: State<'_, AppState>,
) -> Result<crate::browser::types::BrowserState, Error> {
    touch_legacy_browser_runtime_status(&state, "browser_get_state").await?;
    legacy_browser_state(&state).await
}

#[tauri::command]
pub async fn browser_launch(
    state: State<'_, AppState>,
) -> Result<bool, Error> {
    touch_legacy_browser_runtime_status(&state, "browser_launch").await?;
    state
        .browser_context_manager
        .get_or_create(LEGACY_BROWSER_COMPAT_SESSION_ID)
        .await
        .map_err(|error| Error::Internal(error.to_string()))?;
    Ok(true)
}

#[tauri::command]
pub async fn browser_shutdown(
    state: State<'_, AppState>,
) -> Result<bool, Error> {
    touch_legacy_browser_runtime_status(&state, "browser_shutdown").await?;
    state
        .browser_context_manager
        .destroy(LEGACY_BROWSER_COMPAT_SESSION_ID)
        .await;
    Ok(true)
}

#[tauri::command]
pub async fn browser_take_screenshot(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<String, Error> {
    touch_legacy_browser_runtime_status(&state, "browser_take_screenshot").await?;
    if !state
        .browser_context_manager
        .has_context(LEGACY_BROWSER_COMPAT_SESSION_ID)
        .await
    {
        return Err(Error::Internal(
            "Legacy browser compatibility session is not running; call browser_launch first."
                .into(),
        ));
    }

    let ctx = state
        .browser_context_manager
        .get_or_create(LEGACY_BROWSER_COMPAT_SESSION_ID)
        .await
        .map_err(|error| Error::Internal(error.to_string()))?;
    ctx.screenshot(&tab_id)
        .await
        .map_err(|error| Error::Internal(error.to_string()))
}

#[cfg(test)]
mod browser_legacy_runtime_tests {
    use super::*;

    fn tab(tab_id: &str, url: &str, title: &str, active: bool) -> crate::browser::types::TabInfo {
        crate::browser::types::TabInfo {
            tab_id: tab_id.to_string(),
            url: url.to_string(),
            title: title.to_string(),
            active,
        }
    }

    #[test]
    fn stopped_legacy_state_has_no_tabs() {
        let state = legacy_browser_state_from_tabs(false, Vec::new());

        assert!(!state.running);
        assert!(state.tabs.is_empty());
        assert_eq!(state.active_tab_id, None);
    }

    #[test]
    fn legacy_state_preserves_tabs_and_active_tab() {
        let state = legacy_browser_state_from_tabs(
            true,
            vec![
                tab("tab-a", "https://example.test/a", "A", false),
                tab("tab-b", "https://example.test/b", "B", true),
            ],
        );

        assert!(state.running);
        assert_eq!(state.active_tab_id.as_deref(), Some("tab-b"));
        assert_eq!(state.tabs.len(), 2);
        assert_eq!(state.tabs[0].tab_id, "tab-a");
        assert_eq!(state.tabs[1].url, "https://example.test/b");
    }

    #[test]
    fn legacy_state_falls_back_to_first_tab_when_no_tab_is_active() {
        let state = legacy_browser_state_from_tabs(
            true,
            vec![
                tab("tab-a", "https://example.test/a", "A", false),
                tab("tab-b", "https://example.test/b", "B", false),
            ],
        );

        assert_eq!(state.active_tab_id.as_deref(), Some("tab-a"));
    }
}

#[tauri::command]
pub async fn browser_list_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    Ok(state.browser_context_manager.list_active_sessions().await)
}

#[tauri::command]
pub async fn browser_destroy_session(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.browser_context_manager.destroy(&session_id).await;
    Ok(())
}

#[tauri::command]
pub async fn browser_start_screencast(
    session_id: String,
    tab_id: String,
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    touch_browser_ui_runtime_status(state.inner(), "browser_start_screencast").await;
    let ctx = state.browser_context_manager.get_or_create(&session_id).await
        .map_err(|e| e.to_string())?;
    ctx.start_screencast(&tab_id, app_handle).await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_capture_screenshot(
    session_id: String,
    tab_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    touch_browser_ui_runtime_status(state.inner(), "browser_capture_screenshot").await;
    let ctx = state.browser_context_manager.get_or_create(&session_id).await
        .map_err(|e| e.to_string())?;
    ctx.screenshot(&tab_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_stop_screencast(
    session_id: String,
    tab_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    touch_browser_ui_runtime_status(state.inner(), "browser_stop_screencast").await;
    if let Ok(ctx) = state.browser_context_manager.get_or_create(&session_id).await {
        ctx.stop_screencast(&tab_id).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_get_dom_state(
    session_id: String,
    tab_id: String,
    state: State<'_, AppState>,
) -> Result<crate::browser::types::DOMState, String> {
    touch_browser_ui_runtime_status(state.inner(), "browser_get_dom_state").await;
    let ctx = state.browser_context_manager.get_or_create(&session_id).await
        .map_err(|e| e.to_string())?;
    ctx.get_dom_state(&tab_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_ui_navigate(
    session_id: String,
    tab_id: String,
    url: String,
    _app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let result = execute_browser_ui_provider_action(
        state.inner(),
        "browser_ui_navigate",
        &session_id,
        BrowserAction::Navigate {
            url,
            tab_id: Some(tab_id.clone()),
        },
    )
    .await?;
    Ok(result.tab_id.unwrap_or(tab_id))
}

#[tauri::command]
pub async fn browser_ui_go_back(
    session_id: String,
    tab_id: String,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    touch_browser_ui_runtime_status(state.inner(), "browser_ui_go_back").await;
    let ctx = state.browser_context_manager.get_or_create(&session_id).await
        .map_err(|e| e.to_string())?;
    ctx.go_back(&tab_id, &app_handle).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_ui_go_forward(
    session_id: String,
    tab_id: String,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    touch_browser_ui_runtime_status(state.inner(), "browser_ui_go_forward").await;
    let ctx = state.browser_context_manager.get_or_create(&session_id).await
        .map_err(|e| e.to_string())?;
    ctx.go_forward(&tab_id, &app_handle).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_ui_switch_tab(
    session_id: String,
    tab_id: String,
    _app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    execute_browser_ui_provider_action(
        state.inner(),
        "browser_ui_switch_tab",
        &session_id,
        BrowserAction::SwitchTab { tab_id },
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn browser_ui_reload(
    session_id: String,
    tab_id: String,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    touch_browser_ui_runtime_status(state.inner(), "browser_ui_reload").await;
    let ctx = state.browser_context_manager.get_or_create(&session_id).await
        .map_err(|e| e.to_string())?;
    ctx.reload(&tab_id, &app_handle).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_ui_close_tab(
    session_id: String,
    tab_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    touch_browser_ui_runtime_status(state.inner(), "browser_ui_close_tab").await;
    if let Ok(ctx) = state.browser_context_manager.get_or_create(&session_id).await {
        let _ = ctx.close_tab(&tab_id).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_ui_click(
    session_id: String,
    tab_id: String,
    x: f64,
    y: f64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    touch_browser_ui_runtime_status(state.inner(), "browser_ui_click").await;
    let ctx = state.browser_context_manager.get_or_create(&session_id).await
        .map_err(|e| e.to_string())?;
    ctx.click_at(&tab_id, x, y).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_ui_mouse_event(
    session_id: String,
    tab_id: String,
    event_type: String,
    x: f64,
    y: f64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let event_type = parse_browser_mouse_event_type(&event_type)?;
    touch_browser_ui_runtime_status(state.inner(), "browser_ui_mouse_event").await;
    let ctx = state.browser_context_manager.get_or_create(&session_id).await
        .map_err(|e| e.to_string())?;
    ctx.mouse_event_at(&tab_id, event_type, x, y).await.map_err(|e| e.to_string())
}

fn parse_browser_mouse_event_type(
    value: &str,
) -> Result<chromiumoxide::cdp::browser_protocol::input::DispatchMouseEventType, String> {
    use chromiumoxide::cdp::browser_protocol::input::DispatchMouseEventType;
    match value {
        "mousePressed" => Ok(DispatchMouseEventType::MousePressed),
        "mouseMoved" => Ok(DispatchMouseEventType::MouseMoved),
        "mouseReleased" => Ok(DispatchMouseEventType::MouseReleased),
        other => Err(format!("unsupported mouse event type: {other}")),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserLoginCompletionPayload {
    pub spec_id: String,
    pub label: String,
    pub url: String,
    pub profile_id: String,
    pub status: String,
    pub completed_at: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserLoginCompletionProbe {
    pub completed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<BrowserLoginCompletionPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[tauri::command]
pub async fn browser_ui_complete_login(
    session_id: String,
    tab_id: String,
    spec_id: String,
    label: String,
    url: String,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<BrowserLoginCompletionProbe, String> {
    touch_browser_ui_runtime_status(state.inner(), "browser_ui_complete_login").await;
    let ctx = state.browser_context_manager.get_or_create(&session_id).await
        .map_err(|e| e.to_string())?;
    let cookies = ctx.get_cookies(&tab_id, Some(&url)).await.map_err(|e| e.to_string())?;
    if !crate::browser::identity_authorization::is_likely_authenticated_cookie(&url, &cookies) {
        return Ok(BrowserLoginCompletionProbe {
            completed: false,
            payload: None,
            message: Some("等待站点写入登录态...".to_string()),
        });
    }

    let state_snapshot = ctx.capture_storage_state(&tab_id, &url).await
        .map_err(|e| e.to_string())?;
    complete_browser_login_from_storage_state(spec_id, label, url, state_snapshot, app_handle, state).await
}

#[tauri::command]
pub async fn browser_webview_complete_login(
    webview_label: String,
    spec_id: String,
    label: String,
    url: String,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<BrowserLoginCompletionProbe, String> {
    let parsed_url = url::Url::parse(&url).map_err(|e| e.to_string())?;
    let fallback_host = parsed_url.host_str().unwrap_or_default();
    let fallback_secure = parsed_url.scheme() == "https";
    let webview = app_handle
        .get_webview_window(&webview_label)
        .ok_or_else(|| format!("login webview not found: {webview_label}"))?;
    let scoped_cookies = webview.cookies_for_url(parsed_url.clone()).map_err(|e| e.to_string())?;
    let mut cookie_infos: Vec<crate::browser::context::CookieInfo> = scoped_cookies
        .iter()
        .map(|cookie| {
            crate::browser::identity_authorization::cookie_info_from_webview_cookie(
                cookie,
                fallback_host,
                fallback_secure,
            )
        })
        .collect();

    if !crate::browser::identity_authorization::is_likely_authenticated_cookie(
        &url,
        &cookie_infos,
    ) {
        let all_cookies = webview.cookies().map_err(|e| e.to_string())?;
        cookie_infos = all_cookies
            .iter()
            .map(|cookie| {
                crate::browser::identity_authorization::cookie_info_from_webview_cookie(
                    cookie,
                    fallback_host,
                    fallback_secure,
                )
            })
            .filter(|cookie| {
                crate::browser::identity_authorization::cookie_matches_login_host(
                    &cookie.domain,
                    fallback_host,
                )
            })
            .collect();
        if !crate::browser::identity_authorization::is_likely_authenticated_cookie(
            &url,
            &cookie_infos,
        ) {
            let cookie_names = cookie_infos
                .iter()
                .take(12)
                .map(|cookie| cookie.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            tracing::debug!(
                webview_label = %webview_label,
                url = %url,
                cookie_count = cookie_infos.len(),
                cookie_names = %cookie_names,
                "browser webview login probe did not detect auth cookies"
            );
            return Ok(BrowserLoginCompletionProbe {
                completed: false,
                payload: None,
                message: Some(format!(
                    "等待站点写入登录态... 已读取 {} 个同站点 cookie",
                    cookie_infos.len()
                )),
            });
        }
    }

    let state_snapshot =
        crate::browser::identity_authorization::storage_state_from_cookies(cookie_infos);
    complete_browser_login_from_storage_state(spec_id, label, url, state_snapshot, app_handle, state).await
}

async fn complete_browser_login_from_storage_state(
    spec_id: String,
    label: String,
    url: String,
    state_snapshot: crate::browser::identity::PlaywrightStorageState,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<BrowserLoginCompletionProbe, String> {
    let broker = crate::browser::identity::BrowserAuthProfileBroker::system_default()
        .map_err(|e| e.to_string())?;
    let auth_report =
        crate::browser::identity_ipc::complete_browser_identity_authorization_for_broker(
            &broker,
            format!("{label} ({spec_id})"),
            url.clone(),
            crate::browser::identity::BrowserIdentityScope::Workspace,
            &state_snapshot,
            state_snapshot.cookies.len(),
            state_snapshot.origins.len(),
        )
        .map_err(|e| e.to_string())?;
    let profile_id = auth_report
        .profile_id
        .clone()
        .ok_or_else(|| "browser identity authorization did not return a profile id".to_string())?;

    let completed_at = chrono::Utc::now().timestamp_millis();
    let payload = BrowserLoginCompletionPayload {
        spec_id: spec_id.clone(),
        label: label.clone(),
        url: url.clone(),
        profile_id: profile_id.clone(),
        status: "live".to_string(),
        completed_at,
    };

    let spec = state.runtime_service.get_spec(&spec_id).map_err(|e| e.to_string())?;
    let mut values = serde_json::from_str::<serde_json::Value>(&spec.user_config_values)
        .unwrap_or_else(|_| serde_json::json!({}));
    if !values.is_object() {
        values = serde_json::json!({});
    }
    let values_obj = values.as_object_mut().expect("object initialized");
    let profiles = values_obj
        .entry("browser_login_profiles".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !profiles.is_object() {
        *profiles = serde_json::json!({});
    }
    profiles
        .as_object_mut()
        .expect("profiles object initialized")
        .insert(
            url.clone(),
            serde_json::json!({
                "status": "live",
                "profileId": profile_id,
                "label": label,
                "completedAt": completed_at,
            }),
        );
    state.runtime_service.update_user_config(&spec_id, &values)
        .map_err(|e| e.to_string())?;

    let _ = app_handle.emit("automation:browser-login-completed", &payload);
    Ok(BrowserLoginCompletionProbe {
        completed: true,
        payload: Some(payload),
        message: None,
    })
}

#[cfg(test)]
mod browser_login_completion_tests {
    use super::*;
    use crate::browser::identity_authorization::{
        cookie_matches_login_host, is_likely_authenticated_cookie,
    };
    use chromiumoxide::cdp::browser_protocol::input::DispatchMouseEventType;

    fn cookie(name: &str) -> crate::browser::context::CookieInfo {
        crate::browser::context::CookieInfo {
            name: name.to_string(),
            value: "value".to_string(),
            domain: ".bilibili.com".to_string(),
            path: "/".to_string(),
            secure: true,
            http_only: true,
            same_site: None,
            expires: 0.0,
        }
    }

    #[test]
    fn detects_bilibili_authenticated_cookie() {
        assert!(is_likely_authenticated_cookie(
            "https://www.bilibili.com",
            &[cookie("SESSDATA")]
        ));
    }

    #[test]
    fn ignores_bilibili_anonymous_cookie() {
        assert!(!is_likely_authenticated_cookie(
            "https://www.bilibili.com",
            &[cookie("buvid3")]
        ));
    }

    #[test]
    fn detects_douyin_authenticated_cookie_variants() {
        assert!(is_likely_authenticated_cookie(
            "https://www.douyin.com/",
            &[cookie("sessionid_ss")]
        ));
        assert!(is_likely_authenticated_cookie(
            "https://www.douyin.com/",
            &[cookie("passport_auth_status")]
        ));
        assert!(is_likely_authenticated_cookie(
            "https://www.douyin.com/",
            &[cookie("sid_ucp_sso_v1")]
        ));
    }

    #[test]
    fn matches_same_site_cookie_domains_for_login_host() {
        assert!(cookie_matches_login_host(".douyin.com", "www.douyin.com"));
        assert!(cookie_matches_login_host("passport.douyin.com", "www.douyin.com"));
        assert!(!cookie_matches_login_host("example.com", "www.douyin.com"));
    }

    #[test]
    fn parses_supported_browser_mouse_event_types() {
        assert_eq!(
            parse_browser_mouse_event_type("mouseMoved").unwrap(),
            DispatchMouseEventType::MouseMoved,
        );
        assert!(parse_browser_mouse_event_type("dragStart").is_err());
    }
}
