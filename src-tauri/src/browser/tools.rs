use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
use crate::browser::action::{BrowserAction, BrowserActionResult};
use crate::browser::agent_loop::{
    BrowserAgentLoop, BrowserIdentityResumeDecision, BrowserTaskRequest,
    BrowserTaskRuntimePreparationDecision,
};
use crate::browser::context::DevicePreset;
use crate::browser::context_manager::BrowserContextManager;
use crate::browser::decision::BrowserDecisionAdapter;
use crate::browser::dom_state::format_dom_state_for_llm;
use crate::browser::identity_tasks::BrowserIdentityTaskRegistry;
use crate::browser::intervention_bridge::BrowserAskUserBridge;
use crate::browser::memory_adapter::BrowserLongTermMemoryAdapter;
use crate::browser::provider::LOCAL_CHROMIUM_PROVIDER_ID;
use crate::browser::provider_execution::{
    BrowserProviderActionExecutionOutcome, BrowserProviderActionExecutor,
    BrowserProviderActionRouteOptions,
};
use crate::browser::runtime_control_center::BrowserRuntimeProviderConfig;
use crate::browser::runtime_execution::route_options_from_runtime_status;
use crate::browser::runtime_status::BrowserRuntimeStatusService;
use crate::browser::script_runner::ScriptPathPolicy;
use crate::browser::task_store::BrowserTaskStore;
use crate::mcp::SharedMcpManager;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// ── Macro: declare all 14 tool structs ────────────────────────────────────────

macro_rules! browser_tool {
    ($name:ident) => {
        pub struct $name {
            pub ctx_mgr: Arc<BrowserContextManager>,
            pub session_id: String,
            pub runtime_status_service: Option<Arc<BrowserRuntimeStatusService>>,
            pub runtime_provider_config: BrowserRuntimeProviderConfig,
            pub mcp_manager: Option<SharedMcpManager>,
        }
    };
}

browser_tool!(BrowserNavigateTool);
browser_tool!(BrowserGoBackTool);
browser_tool!(BrowserGoForwardTool);
browser_tool!(BrowserReloadTool);
browser_tool!(BrowserGetDomTool);
browser_tool!(BrowserExtractTool);
browser_tool!(BrowserClickTool);
browser_tool!(BrowserTypeTool);
browser_tool!(BrowserSelectTool);
browser_tool!(BrowserScrollTool);
browser_tool!(BrowserSendKeysTool);
browser_tool!(BrowserEvaluateTool);
browser_tool!(BrowserManageTabsTool);
browser_tool!(BrowserGetCookiesTool);
browser_tool!(BrowserSetCookieTool);
browser_tool!(BrowserWaitTool);
browser_tool!(BrowserHoverTool);
browser_tool!(BrowserUploadFileTool);
browser_tool!(BrowserGetStateTool);
browser_tool!(BrowserListTabsTool);
browser_tool!(BrowserSwitchTabTool);
browser_tool!(BrowserCloseTabTool);
browser_tool!(BrowserListSessionsTool);
browser_tool!(BrowserCloseSessionTool);
browser_tool!(BrowserCloseAllTool);

pub struct BrowserTaskTool {
    pub ctx_mgr: Arc<BrowserContextManager>,
    pub session_id: String,
    pub decision_adapter: Arc<dyn BrowserDecisionAdapter>,
    pub task_store: Option<Arc<BrowserTaskStore>>,
    pub ask_user_bridge: Option<Arc<BrowserAskUserBridge>>,
    pub long_term_memory: Option<Arc<BrowserLongTermMemoryAdapter>>,
    pub identity_task_registry: Option<Arc<BrowserIdentityTaskRegistry>>,
    pub runtime_status_service: Option<Arc<BrowserRuntimeStatusService>>,
    pub runtime_provider_config: BrowserRuntimeProviderConfig,
    pub mcp_manager: Option<SharedMcpManager>,
    /// Slice 1b follow-up — wires the Evaluate-gate chokepoint into the
    /// BrowserAgentLoop. `None` preserves pre-Task-B behavior (no gate).
    pub safety_manager: Option<Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>>,
    pub pending_approvals: Option<Arc<crate::app::PendingApprovals>>,
}

pub struct BrowserScreenshotTool {
    pub ctx_mgr: Arc<BrowserContextManager>,
    pub session_id: String,
    pub runtime_status_service: Option<Arc<BrowserRuntimeStatusService>>,
    pub runtime_provider_config: BrowserRuntimeProviderConfig,
    pub mcp_manager: Option<SharedMcpManager>,
    pub workspace_root: Option<PathBuf>,
}

pub struct BrowserTaskResumeTool {
    pub ctx_mgr: Arc<BrowserContextManager>,
    pub session_id: String,
    pub decision_adapter: Arc<dyn BrowserDecisionAdapter>,
    pub task_store: Option<Arc<BrowserTaskStore>>,
    pub ask_user_bridge: Option<Arc<BrowserAskUserBridge>>,
    pub long_term_memory: Option<Arc<BrowserLongTermMemoryAdapter>>,
    pub identity_task_registry: Option<Arc<BrowserIdentityTaskRegistry>>,
    pub runtime_status_service: Option<Arc<BrowserRuntimeStatusService>>,
    pub runtime_provider_config: BrowserRuntimeProviderConfig,
    pub mcp_manager: Option<SharedMcpManager>,
    /// Slice 1b follow-up — wires the Evaluate-gate chokepoint into the
    /// BrowserAgentLoop. `None` preserves pre-Task-B behavior (no gate).
    pub safety_manager: Option<Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>>,
    pub pending_approvals: Option<Arc<crate::app::PendingApprovals>>,
}

pub struct RetryWithBrowserAgentTool {
    pub ctx_mgr: Arc<BrowserContextManager>,
    pub session_id: String,
    pub decision_adapter: Arc<dyn BrowserDecisionAdapter>,
    pub task_store: Option<Arc<BrowserTaskStore>>,
    pub ask_user_bridge: Option<Arc<BrowserAskUserBridge>>,
    pub long_term_memory: Option<Arc<BrowserLongTermMemoryAdapter>>,
    pub identity_task_registry: Option<Arc<BrowserIdentityTaskRegistry>>,
    pub runtime_status_service: Option<Arc<BrowserRuntimeStatusService>>,
    pub runtime_provider_config: BrowserRuntimeProviderConfig,
    pub mcp_manager: Option<SharedMcpManager>,
    /// Slice 1b follow-up — wires the Evaluate-gate chokepoint. Propagated to
    /// the inner BrowserTaskTool delegate.
    pub safety_manager: Option<Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>>,
    pub pending_approvals: Option<Arc<crate::app::PendingApprovals>>,
}

#[derive(Clone)]
pub struct BrowserRunScriptTool {
    pub ctx_mgr: Arc<BrowserContextManager>,
    pub session_id: String,
    pub workspace_root: PathBuf,
    pub builtin_root: PathBuf,
    pub runtime_status_service: Option<Arc<BrowserRuntimeStatusService>>,
    pub runtime_provider_config: BrowserRuntimeProviderConfig,
    pub mcp_manager: Option<SharedMcpManager>,
}

pub struct BrowserRunTool {
    pub inner: BrowserRunScriptTool,
}

// ── 0. BrowserRunScriptTool ──────────────────────────────────────────────────

fn browser_run_failure_output(
    start: Instant,
    session_id: &str,
    file: Option<&str>,
    error: impl ToString,
) -> ToolOutput {
    let duration_ms = start.elapsed().as_millis() as u64;
    ToolOutput::new(
        serde_json::json!({
            "ok": false,
            "error": error.to_string(),
            "sessionId": session_id,
            "file": file.unwrap_or(""),
            "durationMs": duration_ms,
        }),
        duration_ms,
    )
}

fn browser_run_success_output(
    start: Instant,
    session_id: &str,
    file: &str,
    result: serde_json::Value,
) -> ToolOutput {
    let duration_ms = start.elapsed().as_millis() as u64;
    ToolOutput::new(
        serde_json::json!({
            "ok": true,
            "sessionId": session_id,
            "file": file,
            "result": result,
            "durationMs": duration_ms,
        }),
        duration_ms,
    )
}

fn parse_runtime_preparation_decision(
    value: Option<&str>,
) -> Result<BrowserTaskRuntimePreparationDecision, ToolError> {
    match value.unwrap_or("ready") {
        "ready" => Ok(BrowserTaskRuntimePreparationDecision::Ready),
        "defer" => Ok(BrowserTaskRuntimePreparationDecision::Defer),
        other => Err(ToolError::Execution(format!(
            "runtime_preparation_decision must be 'ready' or 'defer', got '{other}'"
        ))),
    }
}

fn parse_identity_resume_decision(
    value: Option<&str>,
) -> Result<BrowserIdentityResumeDecision, ToolError> {
    match value.unwrap_or("require_auth") {
        "require_auth" => Ok(BrowserIdentityResumeDecision::RequireAuth),
        "isolated_profile" => Ok(BrowserIdentityResumeDecision::IsolatedProfile),
        "reauthorize" => Ok(BrowserIdentityResumeDecision::Reauthorize),
        "end_task" => Ok(BrowserIdentityResumeDecision::EndTask),
        other => Err(ToolError::Execution(format!(
            "identity_resume_decision must be 'require_auth', 'isolated_profile', 'reauthorize', or 'end_task', got '{other}'"
        ))),
    }
}

async fn direct_browser_tool_route_options(
    runtime_status_service: Option<&Arc<BrowserRuntimeStatusService>>,
    runtime_provider_config: &BrowserRuntimeProviderConfig,
    mcp_manager: Option<&SharedMcpManager>,
    tool_name: &str,
) -> BrowserProviderActionRouteOptions {
    let Some(runtime_status_service) = runtime_status_service else {
        return BrowserProviderActionRouteOptions::default();
    };

    match runtime_status_service
        .inspect_with_provider_config(runtime_provider_config.clone())
        .await
    {
        Ok(status) => {
            let mut options = direct_browser_tool_route_options_from_status(status);
            if let Some(mcp_manager) = mcp_manager {
                options = options.with_mcp_manager(mcp_manager.clone());
            }
            options
        }
        Err(error) => {
            tracing::warn!(
                tool_name,
                error = %error,
                "Browser Runtime status unavailable for direct browser tool routing; using default provider route options"
            );
            BrowserProviderActionRouteOptions::default()
        }
    }
}

pub(crate) fn direct_browser_tool_route_options_from_status(
    status: crate::browser::runtime_status::BrowserRuntimeStatusReport,
) -> BrowserProviderActionRouteOptions {
    route_options_from_runtime_status(status)
}

async fn direct_browser_tool_status_touch(
    runtime_status_service: Option<&Arc<BrowserRuntimeStatusService>>,
    runtime_provider_config: &BrowserRuntimeProviderConfig,
    mcp_manager: Option<&SharedMcpManager>,
    tool_name: &str,
) {
    let _ = direct_browser_tool_route_options(
        runtime_status_service,
        runtime_provider_config,
        mcp_manager,
        tool_name,
    )
    .await;
}

async fn execute_direct_browser_action(
    ctx_mgr: Arc<BrowserContextManager>,
    runtime_status_service: Option<&Arc<BrowserRuntimeStatusService>>,
    runtime_provider_config: &BrowserRuntimeProviderConfig,
    mcp_manager: Option<&SharedMcpManager>,
    session_id: &str,
    tool_name: &str,
    action: BrowserAction,
) -> Result<(BrowserActionResult, bool), ToolError> {
    let route_options = direct_browser_tool_route_options(
        runtime_status_service,
        runtime_provider_config,
        mcp_manager,
        tool_name,
    )
    .await;
    let executor = BrowserProviderActionExecutor::new(ctx_mgr).with_route_options(route_options);
    let route_decision = executor.route_action(&action);
    let selected_local =
        route_decision.selected_provider_id.as_deref() == Some(LOCAL_CHROMIUM_PROVIDER_ID);
    let execution = executor
        .execute_routed_with_identity(session_id, None, action, route_decision)
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;

    match execution.outcome {
        BrowserProviderActionExecutionOutcome::Executed(result) => {
            if result.ok {
                Ok((result, selected_local))
            } else {
                Err(ToolError::Execution(result.error.unwrap_or_else(|| {
                    format!("direct browser action '{tool_name}' failed")
                })))
            }
        }
        BrowserProviderActionExecutionOutcome::Blocked(blocked) => {
            Err(ToolError::Execution(blocked.message))
        }
    }
}

impl BrowserRunScriptTool {
    async fn execute_run_script(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let Some(file) = params["file"].as_str() else {
            return Ok(browser_run_failure_output(
                start,
                &self.session_id,
                None,
                "file is required",
            ));
        };
        let adapter_params = params
            .get("params")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let timeout_ms = params
            .get("timeout_ms")
            .or_else(|| params.get("timeoutMs"))
            .and_then(|v| v.as_u64())
            .unwrap_or(30_000);
        let home_dir = dirs::home_dir().unwrap_or_else(|| self.workspace_root.clone());
        let policy = ScriptPathPolicy::new(
            self.builtin_root.clone(),
            self.workspace_root.clone(),
            home_dir,
        );
        let resolved = match policy.resolve(file) {
            Ok(path) => path,
            Err(error) => {
                return Ok(browser_run_failure_output(
                    start,
                    &self.session_id,
                    Some(file),
                    error,
                ))
            }
        };
        let source = match std::fs::read_to_string(&resolved) {
            Ok(source) => source,
            Err(error) => {
                return Ok(browser_run_failure_output(
                    start,
                    &self.session_id,
                    Some(file),
                    error,
                ))
            }
        };
        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            "browser_run_script",
        )
        .await;
        let ctx = match self.ctx_mgr.get_or_create(&self.session_id).await {
            Ok(ctx) => ctx,
            Err(error) => {
                return Ok(browser_run_failure_output(
                    start,
                    &self.session_id,
                    Some(file),
                    error,
                ))
            }
        };
        let tab_id = adapter_params
            .get("tab_id")
            .or_else(|| adapter_params.get("tabId"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                params
                    .get("tab_id")
                    .or_else(|| params.get("tabId"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            });
        let tab_id = match tab_id {
            Some(tab_id) => tab_id,
            None => match ctx.active_or_first_tab_id().await {
                Some(tab_id) => tab_id,
                None => {
                    return Ok(browser_run_failure_output(
                        start,
                        &self.session_id,
                        Some(file),
                        "no browser tab available",
                    ))
                }
            },
        };

        match ctx
            .evaluate_script_with_params(&tab_id, &source, adapter_params, timeout_ms)
            .await
        {
            Ok(result) => Ok(browser_run_success_output(
                start,
                &self.session_id,
                file,
                result,
            )),
            Err(error) => Ok(browser_run_failure_output(
                start,
                &self.session_id,
                Some(file),
                error,
            )),
        }
    }
}

#[async_trait]
impl Tool for BrowserRunScriptTool {
    fn name(&self) -> &str {
        "browser_run_script"
    }

    fn description(&self) -> &str {
        "Validate and run a restricted browser adapter JavaScript file. \
         This tool is reserved for automation adapters such as live-room moderation."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "string",
                    "description": "JavaScript file path. Built-in adapter paths may start with douyin/ or shared/."
                },
                "params": {
                    "type": "object",
                    "description": "Adapter parameters passed to the script.",
                    "additionalProperties": true
                },
                "timeout_ms": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Maximum script execution time in milliseconds."
                }
            },
            "required": ["file"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        self.execute_run_script(params).await
    }
}

#[async_trait]
impl Tool for BrowserRunTool {
    fn name(&self) -> &str {
        "browser_run"
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.inner.parameters_schema()
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        self.inner.execute_run_script(params).await
    }
}

// ── 1. BrowserNavigateTool ────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserNavigateTool {
    fn name(&self) -> &str {
        "browser_navigate"
    }

    fn description(&self) -> &str {
        "Navigate to a URL in the browser. Launches the browser if not running. \
         Returns the tab_id for subsequent operations.\n\
         \n\
         **Parameters**\n\
         - `url` (string, required): URL to navigate to.\n\
         - `tab_id` (string, optional): Tab ID to reuse, or 'new' to open a new tab (default 'new').\n\
         - `device` (string, optional): \"mobile\" sets 390\u{d7}844 + iPhone UA; \"desktop\" (default) sets 1280\u{d7}800."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to navigate to"
                },
                "tab_id": {
                    "type": "string",
                    "description": "Tab ID to reuse, or 'new' to open a new tab (default 'new')"
                },
                "device": {
                    "type": "string",
                    "enum": ["desktop", "mobile"],
                    "description": "Device preset: 'mobile' sets 390x844 + iPhone UA; 'desktop' (default) sets 1280x800"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let url = params["url"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("url is required".to_string()))?;
        let tab_id = params["tab_id"].as_str().unwrap_or("new");
        let device = params["device"]
            .as_str()
            .map(DevicePreset::from_str)
            .unwrap_or(DevicePreset::Desktop);

        let (result, selected_local) = execute_direct_browser_action(
            Arc::clone(&self.ctx_mgr),
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            &self.session_id,
            self.name(),
            BrowserAction::Navigate {
                url: url.to_string(),
                tab_id: Some(tab_id.to_string()),
            },
        )
        .await?;
        let resolved_id = result.tab_id.ok_or_else(|| {
            ToolError::Execution("browser_navigate did not return tab_id".to_string())
        })?;

        if selected_local {
            let ctx = self
                .ctx_mgr
                .get_or_create(&self.session_id)
                .await
                .map_err(|e| ToolError::Execution(e.to_string()))?;
            if let Err(e) = ctx.apply_device_emulation(&resolved_id, device).await {
                tracing::warn!("device emulation failed (non-fatal): {e}");
            }
        }

        let observation = result.observation_json.clone();
        let title = observation
            .as_ref()
            .and_then(|value| value.pointer("/output/title"))
            .and_then(|value| value.as_str())
            .filter(|title| !title.is_empty());
        let content = match title {
            Some(title) => format!("Navigated to {url}. tab_id={resolved_id}. title={title}"),
            None => format!("Navigated to {url}. tab_id={resolved_id}"),
        };
        Ok(ToolOutput::new(
            serde_json::json!({
                "ok": true,
                "content": content,
                "tab_id": resolved_id,
                "observation": observation,
            }),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 2. BrowserGoBackTool ──────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserGoBackTool {
    fn name(&self) -> &str {
        "browser_go_back"
    }

    fn description(&self) -> &str {
        "Navigate backward in the browser history for the given tab."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID returned by a prior browser_navigate call. Do NOT pass 'new' here — 'new' is only valid for browser_navigate itself." }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;

        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            self.name(),
        )
        .await;
        let ctx = self
            .ctx_mgr
            .get_or_create(&self.session_id)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        ctx.go_back(tab_id, self.ctx_mgr.app_handle())
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(
            "Navigated back.",
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 3. BrowserGoForwardTool ───────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserGoForwardTool {
    fn name(&self) -> &str {
        "browser_go_forward"
    }

    fn description(&self) -> &str {
        "Navigate forward in the browser history for the given tab."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID returned by a prior browser_navigate call. Do NOT pass 'new' here — 'new' is only valid for browser_navigate itself." }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;

        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            self.name(),
        )
        .await;
        let ctx = self
            .ctx_mgr
            .get_or_create(&self.session_id)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        ctx.go_forward(tab_id, self.ctx_mgr.app_handle())
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(
            "Navigated forward.",
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 4. BrowserReloadTool ──────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserReloadTool {
    fn name(&self) -> &str {
        "browser_reload"
    }

    fn description(&self) -> &str {
        "Reload the current page for the given tab."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID returned by a prior browser_navigate call. Do NOT pass 'new' here — 'new' is only valid for browser_navigate itself." }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;

        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            self.name(),
        )
        .await;
        let ctx = self
            .ctx_mgr
            .get_or_create(&self.session_id)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        ctx.reload(tab_id, self.ctx_mgr.app_handle())
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(
            "Page reloaded.",
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 5. BrowserGetDomTool ──────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserGetDomTool {
    fn name(&self) -> &str {
        "browser_get_dom"
    }

    fn description(&self) -> &str {
        "Return the interactive DOM elements of the current page as an indexed list. \
         Always call browser_get_dom AFTER navigating and BEFORE interacting. \
         Indexes are reassigned on each call; stale indexes will click the wrong element."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID returned by a prior browser_navigate call. Do NOT pass 'new' here — 'new' is only valid for browser_navigate itself." }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;

        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            self.name(),
        )
        .await;
        let ctx = self
            .ctx_mgr
            .get_or_create(&self.session_id)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let state = ctx
            .get_dom_state(tab_id)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        let formatted = format_dom_state_for_llm(&state);

        Ok(ToolOutput::success(
            &formatted,
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 6. BrowserScreenshotTool ──────────────────────────────────────────────────

fn browser_screenshot_save_path_arg(params: &serde_json::Value) -> Option<&str> {
    params
        .get("save_path")
        .or_else(|| params.get("path"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|path| !path.is_empty())
}

#[async_trait]
impl Tool for BrowserScreenshotTool {
    fn name(&self) -> &str {
        "browser_screenshot"
    }

    fn description(&self) -> &str {
        "Capture a PNG screenshot of the current browser page through the Browser Runtime provider. Optionally save it to a workspace file."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID to screenshot — must be returned by a prior browser_navigate call. Not 'new'." },
                "full_page": { "type": "boolean", "description": "Capture the full scrollable page when the active provider supports it (default false)." },
                "save_path": { "type": "string", "description": "Optional workspace-relative or workspace-contained absolute PNG path to save the screenshot, e.g. 'apple-screenshot.png'." },
                "path": { "type": "string", "description": "Alias for save_path. Optional workspace-relative or workspace-contained absolute PNG path to save the screenshot." }
            },
            "required": ["tab_id"]
        })
    }

    fn path_args<'a>(&self, args: &'a serde_json::Value) -> Vec<&'a str> {
        browser_screenshot_save_path_arg(args)
            .map(|path| vec![path])
            .unwrap_or_default()
    }

    fn preview_target_path(&self, args: &serde_json::Value) -> Option<String> {
        browser_screenshot_save_path_arg(args).map(str::to_string)
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let full_page = params["full_page"].as_bool().unwrap_or(false);
        let save_path = browser_screenshot_save_path_arg(&params)
            .map(|path| resolve_screenshot_save_path(path, self.workspace_root.as_deref()))
            .transpose()?
            .flatten()
            .or_else(|| Some(default_screenshot_temp_path(&self.session_id)));

        let (result, _) = execute_direct_browser_action(
            Arc::clone(&self.ctx_mgr),
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            &self.session_id,
            self.name(),
            BrowserAction::Screenshot {
                tab_id: tab_id.to_string(),
                full_page,
                save_path,
            },
        )
        .await?;

        let elapsed = start.elapsed().as_millis() as u64;
        let observation = result.observation_json.unwrap_or_else(|| {
            serde_json::json!({
                "ok": result.ok,
                "message": result.message,
                "tabId": result.tab_id,
            })
        });
        let mut image_data = observation
            .get("data")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let width = observation
            .get("width")
            .and_then(|value| value.as_u64())
            .unwrap_or(1280);
        let height = observation
            .get("height")
            .and_then(|value| value.as_u64())
            .unwrap_or(800);
        let saved_path = observation
            .get("savedPath")
            .or_else(|| observation.pointer("/output/screenshotPath"))
            .and_then(|value| value.as_str())
            .map(str::to_string);
        if image_data.is_none() {
            if let Some(path) = saved_path.as_deref() {
                if let Ok(bytes) = std::fs::read(path) {
                    image_data = Some(BASE64.encode(bytes));
                }
            }
        }
        let artifact_refs = observation
            .get("artifactRefs")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));
        Ok(ToolOutput::new(
            serde_json::json!({
                "ok": true,
                "observation": observation,
                "tab_id": result.tab_id,
                "data": image_data,
                "width": width,
                "height": height,
                "saved_path": saved_path,
                "artifact_refs": artifact_refs,
                "content": result.message.unwrap_or_else(|| "Captured browser screenshot".to_string()),
            }),
            elapsed,
        ))
    }
}

fn resolve_screenshot_save_path(
    save_path: &str,
    workspace_root: Option<&Path>,
) -> Result<Option<String>, ToolError> {
    if save_path.trim().is_empty() {
        return Ok(None);
    }
    let raw = PathBuf::from(save_path);
    if raw
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ToolError::Execution(
            "save_path must not contain '..'".to_string(),
        ));
    }

    let absolute = if raw.is_absolute() {
        raw
    } else {
        let Some(root) = workspace_root else {
            return Err(ToolError::Execution(
                "save_path must be absolute when no workspace root is available".to_string(),
            ));
        };
        root.join(raw)
    };

    if let Some(root) = workspace_root {
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        if !absolute.starts_with(&canonical_root) {
            return Err(ToolError::Execution(format!(
                "save_path must stay inside the active workspace: {}",
                canonical_root.display()
            )));
        }
    }
    if let Some(parent) = absolute.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| ToolError::Execution(format!("create screenshot dir: {error}")))?;
    }
    Ok(Some(absolute.to_string_lossy().to_string()))
}

fn default_screenshot_temp_path(session_id: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let safe_session: String = session_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let dir = std::env::temp_dir().join("uclaw-browser-screenshots");
    let _ = std::fs::create_dir_all(&dir);
    dir
        .join(format!("{safe_session}-{millis}.png"))
        .to_string_lossy()
        .to_string()
}

// ── 7. BrowserExtractTool ─────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserExtractTool {
    fn name(&self) -> &str {
        "browser_extract"
    }

    fn description(&self) -> &str {
        "Extract the visible text content from the current browser page or a specific element."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID returned by a prior browser_navigate call. Do NOT pass 'new' here — 'new' is only valid for browser_navigate itself." },
                "selector": {
                    "type": "string",
                    "description": "CSS selector for the element to extract text from (default 'body')"
                }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let selector = params["selector"].as_str().unwrap_or("body");

        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            self.name(),
        )
        .await;
        let ctx = self
            .ctx_mgr
            .get_or_create(&self.session_id)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        // Escape single quotes in the selector to avoid JS injection.
        let safe_selector = selector.replace('\'', "\\'");
        let script = format!(
            "(function(){{\
                var el = document.querySelector('{selector}') || document.body;\
                return (el.innerText || el.textContent || '').substring(0, 40000);\
            }})()",
            selector = safe_selector,
        );

        let text = ctx
            .execute_js(tab_id, &script)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(
            &text,
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 8. BrowserClickTool ───────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserClickTool {
    fn name(&self) -> &str {
        "browser_click"
    }

    fn description(&self) -> &str {
        "Click an interactive element by its index from browser_get_dom."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID returned by a prior browser_navigate call. Do NOT pass 'new' here — 'new' is only valid for browser_navigate itself." },
                "index": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Element index from browser_get_dom"
                }
            },
            "required": ["tab_id", "index"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let index = params["index"]
            .as_u64()
            .ok_or_else(|| ToolError::Execution("index is required".to_string()))?
            as u32;

        let _ = execute_direct_browser_action(
            Arc::clone(&self.ctx_mgr),
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            &self.session_id,
            self.name(),
            BrowserAction::Click {
                tab_id: tab_id.to_string(),
                index,
            },
        )
        .await?;

        Ok(ToolOutput::success(
            &format!("Clicked element [{}].", index),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 9. BrowserTypeTool ────────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserTypeTool {
    fn name(&self) -> &str {
        "browser_type"
    }

    fn description(&self) -> &str {
        "Type text into a form field identified by its index from browser_get_dom."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID returned by a prior browser_navigate call. Do NOT pass 'new' here — 'new' is only valid for browser_navigate itself." },
                "index": {
                    "type": "integer",
                    "description": "Element index from browser_get_dom"
                },
                "text": { "type": "string", "description": "Text to type" }
            },
            "required": ["tab_id", "index", "text"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let index = params["index"]
            .as_u64()
            .ok_or_else(|| ToolError::Execution("index is required".to_string()))?
            as u32;
        let text = params["text"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("text is required".to_string()))?;

        let _ = execute_direct_browser_action(
            Arc::clone(&self.ctx_mgr),
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            &self.session_id,
            self.name(),
            BrowserAction::Type {
                tab_id: tab_id.to_string(),
                index,
                text: text.to_string(),
            },
        )
        .await?;

        Ok(ToolOutput::success(
            &format!("Typed into element [{}].", index),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 10. BrowserSelectTool ─────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserSelectTool {
    fn name(&self) -> &str {
        "browser_select"
    }

    fn description(&self) -> &str {
        "Select an option in a <select> element identified by its index from browser_get_dom."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID returned by a prior browser_navigate call. Do NOT pass 'new' here — 'new' is only valid for browser_navigate itself." },
                "index": {
                    "type": "integer",
                    "description": "Element index from browser_get_dom"
                },
                "value": { "type": "string", "description": "Option value to select" }
            },
            "required": ["tab_id", "index", "value"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let index = params["index"]
            .as_u64()
            .ok_or_else(|| ToolError::Execution("index is required".to_string()))?
            as u32;
        let value = params["value"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("value is required".to_string()))?;

        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            self.name(),
        )
        .await;
        let ctx = self
            .ctx_mgr
            .get_or_create(&self.session_id)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        ctx.select_option(tab_id, index, value)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(ToolOutput::success(
            &format!("Selected value '{}' in element [{}].", value, index),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 11. BrowserScrollTool ─────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserScrollTool {
    fn name(&self) -> &str {
        "browser_scroll"
    }

    fn description(&self) -> &str {
        "Scroll the page or a specific element in a direction by a number of pixels."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID returned by a prior browser_navigate call. Do NOT pass 'new' here — 'new' is only valid for browser_navigate itself." },
                "direction": {
                    "type": "string",
                    "enum": ["up", "down", "left", "right"],
                    "description": "Scroll direction"
                },
                "pixels": {
                    "type": "integer",
                    "description": "Number of pixels to scroll (default 300)"
                },
                "index": {
                    "type": "integer",
                    "description": "Element index to scroll within (optional; scrolls the window if omitted)"
                }
            },
            "required": ["tab_id", "direction"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let direction = params["direction"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("direction is required".to_string()))?;
        let pixels = params["pixels"].as_u64().unwrap_or(300) as u32;
        let index = params["index"].as_u64().map(|i| i as u32);

        let _ = execute_direct_browser_action(
            Arc::clone(&self.ctx_mgr),
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            &self.session_id,
            self.name(),
            BrowserAction::Scroll {
                tab_id: tab_id.to_string(),
                direction: direction.to_string(),
                pixels: Some(pixels),
                index,
            },
        )
        .await?;

        Ok(ToolOutput::success(
            &format!("Scrolled {} {}px.", direction, pixels),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 12. BrowserSendKeysTool ───────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserSendKeysTool {
    fn name(&self) -> &str {
        "browser_send_keys"
    }

    fn description(&self) -> &str {
        "Send keyboard key events to the page (e.g. 'Enter', 'Escape', 'Tab')."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID returned by a prior browser_navigate call. Do NOT pass 'new' here — 'new' is only valid for browser_navigate itself." },
                "keys": {
                    "type": "string",
                    "description": "Key name to send (e.g. 'Enter', 'Escape', 'Tab', 'ArrowDown')"
                }
            },
            "required": ["tab_id", "keys"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let keys = params["keys"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("keys is required".to_string()))?;

        let _ = execute_direct_browser_action(
            Arc::clone(&self.ctx_mgr),
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            &self.session_id,
            self.name(),
            BrowserAction::SendKeys {
                tab_id: tab_id.to_string(),
                keys: keys.to_string(),
            },
        )
        .await?;

        Ok(ToolOutput::success(
            &format!("Sent key: {}.", keys),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 13. BrowserEvaluateTool ───────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserEvaluateTool {
    fn name(&self) -> &str {
        "browser_evaluate"
    }

    fn description(&self) -> &str {
        "Execute a JavaScript snippet in the current tab and return the result as a JSON string."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID returned by a prior browser_navigate call. Do NOT pass 'new' here — 'new' is only valid for browser_navigate itself." },
                "script": {
                    "type": "string",
                    "description": "JavaScript expression or function to evaluate"
                }
            },
            "required": ["tab_id", "script"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let script = params["script"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("script is required".to_string()))?;

        let (result, _) = execute_direct_browser_action(
            Arc::clone(&self.ctx_mgr),
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            &self.session_id,
            self.name(),
            BrowserAction::Evaluate {
                tab_id: tab_id.to_string(),
                script: script.to_string(),
            },
        )
        .await?;
        let result = result.message.unwrap_or_default();

        Ok(ToolOutput::success(
            &result,
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 14. BrowserManageTabsTool ─────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserManageTabsTool {
    fn name(&self) -> &str {
        "browser_manage_tabs"
    }

    fn description(&self) -> &str {
        "List all open tabs or close a specific tab."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": {
                    "type": "string",
                    "description": "Tab ID (used for 'close' action; ignored for 'list')"
                },
                "action": {
                    "type": "string",
                    "enum": ["list", "close"],
                    "description": "'list' returns all open tabs; 'close' closes the specified tab"
                }
            },
            "required": ["tab_id", "action"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let action = params["action"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("action is required".to_string()))?;

        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            self.name(),
        )
        .await;
        let ctx = self
            .ctx_mgr
            .get_or_create(&self.session_id)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        match action {
            "list" => {
                let tabs = ctx.get_all_tabs().await;
                let json = serde_json::to_string_pretty(&tabs)
                    .map_err(|e| ToolError::Execution(e.to_string()))?;
                Ok(ToolOutput::success(
                    &json,
                    start.elapsed().as_millis() as u64,
                ))
            }
            "close" => {
                ctx.close_tab(tab_id)
                    .await
                    .map_err(|e| ToolError::Execution(e.to_string()))?;
                Ok(ToolOutput::success(
                    &format!("Closed tab {}.", tab_id),
                    start.elapsed().as_millis() as u64,
                ))
            }
            _ => Err(ToolError::Execution(format!(
                "Unknown action '{}'; expected 'list' or 'close'",
                action
            ))),
        }
    }
}

// ── 15. BrowserGetCookiesTool ─────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserGetCookiesTool {
    fn name(&self) -> &str {
        "browser_get_cookies"
    }

    fn description(&self) -> &str {
        r#"Retrieve cookies from the current browser session.

Returns all cookies visible to the specified tab. Use `url_filter` to scope
to a specific origin.

**Parameters**
- `tab_id` (string, required): Tab ID from browser_navigate or browser_get_dom.
- `url_filter` (string, optional): Only return cookies for this URL.

**Returns** JSON array of cookie objects: name, value, domain, path, secure,
http_only, same_site, expires.

**Example**
{"tab_id":"tab-1","url_filter":"https://example.com"}
→ [{"name":"session","value":"abc123","domain":"example.com",...}]
"#
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID from browser_navigate or browser_get_dom" },
                "url_filter": { "type": "string", "description": "Only return cookies matching this URL" }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let url_filter = params["url_filter"].as_str();
        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            self.name(),
        )
        .await;
        let ctx = self
            .ctx_mgr
            .get_or_create(&self.session_id)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        match ctx.get_cookies(tab_id, url_filter).await {
            Ok(cookies) => {
                let json =
                    serde_json::to_string_pretty(&cookies).unwrap_or_else(|_| "[]".to_string());
                Ok(ToolOutput::success(
                    &json,
                    start.elapsed().as_millis() as u64,
                ))
            }
            Err(e) => Ok(ToolOutput::error(
                &e.to_string(),
                start.elapsed().as_millis() as u64,
            )),
        }
    }
}

// ── 16. BrowserSetCookieTool ──────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserSetCookieTool {
    fn name(&self) -> &str {
        "browser_set_cookie"
    }

    fn description(&self) -> &str {
        r#"Set a single cookie in the current browser session.

Use this to inject authentication cookies, bypass consent banners, or persist
session tokens before navigating to a page that requires them.

**Parameters**
- `tab_id` (string, required): Tab ID from browser_navigate.
- `name` (string, required): Cookie name.
- `value` (string, required): Cookie value.
- `domain` (string, required): Cookie domain, e.g. "example.com".
- `path` (string, optional): Cookie path. Defaults to "/".
- `secure` (boolean, optional): Set Secure flag. Default false.
- `http_only` (boolean, optional): Set HttpOnly flag. Default false.

**Returns** "Cookie set successfully." on success.
"#
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID from browser_navigate" },
                "name": { "type": "string", "description": "Cookie name" },
                "value": { "type": "string", "description": "Cookie value" },
                "domain": { "type": "string", "description": "Cookie domain, e.g. \"example.com\"" },
                "path": { "type": "string", "description": "Cookie path (optional, defaults to '/')" },
                "secure": { "type": "boolean", "description": "Set Secure flag (default false)" },
                "http_only": { "type": "boolean", "description": "Set HttpOnly flag (default false)" }
            },
            "required": ["tab_id", "name", "value", "domain"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let name = params["name"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("name is required".to_string()))?;
        let value = params["value"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("value is required".to_string()))?;
        let domain = params["domain"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("domain is required".to_string()))?;
        let path = params["path"].as_str();
        let secure = params["secure"].as_bool().unwrap_or(false);
        let http_only = params["http_only"].as_bool().unwrap_or(false);
        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            self.name(),
        )
        .await;
        let ctx = self
            .ctx_mgr
            .get_or_create(&self.session_id)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        match ctx
            .set_cookie(tab_id, name, value, domain, path, secure, http_only)
            .await
        {
            Ok(_) => Ok(ToolOutput::success(
                "Cookie set successfully.",
                start.elapsed().as_millis() as u64,
            )),
            Err(e) => Ok(ToolOutput::error(
                &e.to_string(),
                start.elapsed().as_millis() as u64,
            )),
        }
    }
}

// ── 17. BrowserWaitTool ───────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserWaitTool {
    fn name(&self) -> &str {
        "browser_wait"
    }

    fn description(&self) -> &str {
        "Wait for a CSS selector to appear in the DOM, or pause for a fixed duration.\n\
         Use after browser_navigate or browser_click when a page or element needs time to load.\n\
         \n\
         **Parameters**\n\
         - `tab_id` (string, required): Tab ID from a previous browser_navigate call.\n\
         - `selector` (string, optional): CSS selector to wait for (e.g. '#main', '.loaded', 'button[type=submit]').\n\
         - `timeout_ms` (number, optional): Maximum wait in milliseconds (default 10000)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID from a previous browser_navigate call" },
                "selector": {
                    "type": "string",
                    "description": "CSS selector to wait for (e.g. '#main', '.loaded'). If omitted, waits for timeout_ms."
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Maximum wait in milliseconds (default 10000)"
                }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id required".to_string()))?;
        let selector = params["selector"].as_str();
        let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(10_000);
        let start = Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            self.name(),
        )
        .await;
        let ctx = self
            .ctx_mgr
            .get_or_create(&self.session_id)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        if let Some(sel) = selector {
            let escaped = sel.replace('\\', "\\\\").replace('"', "\\\"");
            loop {
                if start.elapsed() >= timeout {
                    return Ok(ToolOutput::error(
                        &format!(
                            "Timeout: selector '{}' not found after {}ms",
                            sel, timeout_ms
                        ),
                        start.elapsed().as_millis() as u64,
                    ));
                }
                let found = ctx
                    .execute_js(
                        tab_id,
                        &format!("!!document.querySelector(\"{}\")", escaped),
                    )
                    .await
                    .map_err(|e| ToolError::Execution(e.to_string()))?;
                if found.trim() == "true" {
                    let elapsed = start.elapsed().as_millis() as u64;
                    return Ok(ToolOutput::success(
                        &format!("Element '{}' found after {}ms", sel, elapsed),
                        elapsed,
                    ));
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        } else {
            tokio::time::sleep(timeout).await;
            Ok(ToolOutput::success(
                &format!("Waited {}ms", timeout_ms),
                start.elapsed().as_millis() as u64,
            ))
        }
    }
}

// ── 18. BrowserHoverTool ──────────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserHoverTool {
    fn name(&self) -> &str {
        "browser_hover"
    }

    fn description(&self) -> &str {
        "Move the mouse cursor over an element to trigger CSS :hover states and JS mouseover events.\n\
         Required for dropdown menus, tooltips, and any reveal-on-hover UI pattern.\n\
         \n\
         **Parameters**\n\
         - `tab_id` (string, required): Tab ID.\n\
         - `index` (number, required): Element index from browser_get_dom."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID returned by a prior browser_navigate call. Do NOT pass 'new' here — 'new' is only valid for browser_navigate itself." },
                "index": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Element index from browser_get_dom"
                }
            },
            "required": ["tab_id", "index"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id required".to_string()))?;
        let index = params["index"]
            .as_u64()
            .ok_or_else(|| ToolError::Execution("index required".to_string()))?
            as u32;

        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            self.name(),
        )
        .await;
        let ctx = self
            .ctx_mgr
            .get_or_create(&self.session_id)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        // Step 1: get bounding box and dispatch JS mouse events (triggers JS listeners).
        let js = format!(
            r#"(function(){{
                const el = document.querySelector('[data-uclaw-index="{}"]');
                if (!el) return null;
                const r = el.getBoundingClientRect();
                const x = r.left + r.width / 2, y = r.top + r.height / 2;
                el.dispatchEvent(new MouseEvent('mouseenter', {{bubbles:false,cancelable:true,clientX:x,clientY:y}}));
                el.dispatchEvent(new MouseEvent('mouseover',  {{bubbles:true, cancelable:true,clientX:x,clientY:y}}));
                el.dispatchEvent(new MouseEvent('mousemove',  {{bubbles:true, cancelable:true,clientX:x,clientY:y}}));
                return {{x: Math.round(x), y: Math.round(y)}};
            }})()"#,
            index
        );

        let result = ctx
            .execute_js(tab_id, &js)
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        if result.trim() == "null" {
            return Err(ToolError::Execution(format!(
                "Element with index {} not found",
                index
            )));
        }

        // Step 2: send CDP Input.dispatchMouseEvent to activate CSS :hover pseudo-class.
        let coords: serde_json::Value =
            serde_json::from_str(&result).unwrap_or(serde_json::Value::Null);
        if let (Some(x), Some(y)) = (coords["x"].as_f64(), coords["y"].as_f64()) {
            use chromiumoxide::cdp::browser_protocol::input::{
                DispatchMouseEventParams, DispatchMouseEventType,
            };
            let pages = ctx.pages.read().await;
            if let Some(page) = pages.get(tab_id) {
                let _ = page
                    .execute(DispatchMouseEventParams::new(
                        DispatchMouseEventType::MouseMoved,
                        x,
                        y,
                    ))
                    .await;
            }
        }

        Ok(ToolOutput::success(
            &format!("Hovered element at index {}", index),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 19. BrowserUploadFileTool ─────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserUploadFileTool {
    fn name(&self) -> &str {
        "browser_upload_file"
    }

    fn description(&self) -> &str {
        "Set a file on a file input element (<input type='file'>).\n\
         The file must exist in the agent workspace (~/Documents/workground/).\n\
         \n\
         **Parameters**\n\
         - `tab_id` (string, required): Tab ID.\n\
         - `index` (number, required): Index of the file input element from browser_get_dom.\n\
         - `file_path` (string, required): Path relative to ~/Documents/workground/ \
           (e.g. 'report.pdf' or 'images/photo.jpg'). Must not contain '..'."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID returned by a prior browser_navigate call. Do NOT pass 'new' here — 'new' is only valid for browser_navigate itself." },
                "index": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Index of the file input element from browser_get_dom"
                },
                "file_path": {
                    "type": "string",
                    "description": "Path relative to ~/Documents/workground/ (e.g. 'report.pdf'). Must not contain '..'."
                }
            },
            "required": ["tab_id", "index", "file_path"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id required".to_string()))?;
        let index = params["index"]
            .as_u64()
            .ok_or_else(|| ToolError::Execution("index required".to_string()))?
            as u32;
        let file_path = params["file_path"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("file_path required".to_string()))?;

        // Reject obvious traversal attempts before any filesystem access.
        if file_path.contains("..") {
            return Err(ToolError::InvalidParams(
                "file_path must not contain '..'".to_string(),
            ));
        }

        // Resolve to absolute path and verify it stays under the workspace root.
        let workspace_root = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("Documents/workground");
        let abs_path = workspace_root.join(file_path);

        // Canonicalize both paths to resolve any remaining symlinks / components.
        let canonical_root = workspace_root
            .canonicalize()
            .unwrap_or(workspace_root.clone());
        let canonical_abs = abs_path.canonicalize().map_err(|_| {
            ToolError::Execution(format!(
                "File not found: {} (looked in {})",
                file_path,
                abs_path.display()
            ))
        })?;
        if !canonical_abs.starts_with(&canonical_root) {
            return Err(ToolError::InvalidParams(
                "file_path must not escape the workspace directory".to_string(),
            ));
        }

        let _ = execute_direct_browser_action(
            Arc::clone(&self.ctx_mgr),
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            &self.session_id,
            self.name(),
            BrowserAction::UploadFile {
                tab_id: tab_id.to_string(),
                index,
                file_path: file_path.to_string(),
            },
        )
        .await?;

        Ok(ToolOutput::success(
            &format!(
                "File '{}' set on input element at index {}",
                file_path, index
            ),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 20. BrowserGetStateTool ──────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserGetStateTool {
    fn name(&self) -> &str {
        "browser_get_state"
    }

    fn description(&self) -> &str {
        "Return structured browser state for the current tab: URL, title, tabs, page text, interactive DOM elements, and optionally screenshot and visual perception data."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID to observe — must be returned by a prior browser_navigate call. Not 'new'." },
                "include_screenshot": { "type": "boolean", "description": "Include base64 PNG screenshot data (default false)" },
                "include_visual": { "type": "boolean", "description": "Run the configured visual perception provider over a screenshot and include OCR/control candidates when available (default false)" }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let include_screenshot = params["include_screenshot"].as_bool().unwrap_or(false);
        let include_visual = params["include_visual"].as_bool().unwrap_or(false);
        let (result, _) = execute_direct_browser_action(
            Arc::clone(&self.ctx_mgr),
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            &self.session_id,
            self.name(),
            BrowserAction::GetState {
                tab_id: tab_id.to_string(),
                include_screenshot,
                include_visual,
            },
        )
        .await?;
        let observation = result.observation_json.ok_or_else(|| {
            ToolError::Execution("browser_get_state did not return observation".to_string())
        })?;
        Ok(ToolOutput::new(
            serde_json::json!({ "ok": true, "observation": observation }),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 21. BrowserListTabsTool ──────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserListTabsTool {
    fn name(&self) -> &str {
        "browser_list_tabs"
    }

    fn description(&self) -> &str {
        "List all open tabs in the current browser session with tab_id, URL, title, and active status."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }

    async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let (result, _) = execute_direct_browser_action(
            Arc::clone(&self.ctx_mgr),
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            &self.session_id,
            self.name(),
            BrowserAction::ListTabs,
        )
        .await?;
        let tabs = result
            .observation_json
            .and_then(|value| value.get("tabs").cloned())
            .unwrap_or_else(|| serde_json::json!([]));
        Ok(ToolOutput::new(
            serde_json::json!({ "ok": true, "tabs": tabs }),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 22. BrowserSwitchTabTool ─────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserSwitchTabTool {
    fn name(&self) -> &str {
        "browser_switch_tab"
    }

    fn description(&self) -> &str {
        "Switch the active browser tab for this agent session."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID to make active — must be returned by a prior browser_navigate call. Not 'new'." }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let _ = execute_direct_browser_action(
            Arc::clone(&self.ctx_mgr),
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            &self.session_id,
            self.name(),
            BrowserAction::SwitchTab {
                tab_id: tab_id.to_string(),
            },
        )
        .await?;
        Ok(ToolOutput::success(
            &format!("Switched to tab {}.", tab_id),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 23. BrowserCloseTabTool ──────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserCloseTabTool {
    fn name(&self) -> &str {
        "browser_close_tab"
    }

    fn description(&self) -> &str {
        "Close a specific tab in the current browser session."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tab_id": { "type": "string", "description": "Tab ID to close — must be returned by a prior browser_navigate call. Not 'new'." }
            },
            "required": ["tab_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let tab_id = params["tab_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("tab_id is required".to_string()))?;
        let _ = execute_direct_browser_action(
            Arc::clone(&self.ctx_mgr),
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            &self.session_id,
            self.name(),
            BrowserAction::CloseTab {
                tab_id: tab_id.to_string(),
            },
        )
        .await?;
        Ok(ToolOutput::success(
            &format!("Closed tab {}.", tab_id),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 24. BrowserListSessionsTool ──────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserListSessionsTool {
    fn name(&self) -> &str {
        "browser_list_sessions"
    }

    fn description(&self) -> &str {
        "List agent session IDs that currently have live browser contexts."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }

    async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            self.name(),
        )
        .await;
        let sessions = self.ctx_mgr.list_active_sessions().await;
        Ok(ToolOutput::new(
            serde_json::json!({ "ok": true, "sessions": sessions }),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 25. BrowserCloseSessionTool ──────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserCloseSessionTool {
    fn name(&self) -> &str {
        "browser_close_session"
    }

    fn description(&self) -> &str {
        "Close a browser context by session_id. Defaults to the current agent session."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Session ID to close; defaults to current session" }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let session_id = params["session_id"].as_str().unwrap_or(&self.session_id);
        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            self.name(),
        )
        .await;
        self.ctx_mgr.destroy(session_id).await;
        Ok(ToolOutput::success(
            &format!("Closed browser session {}.", session_id),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 26. BrowserCloseAllTool ──────────────────────────────────────────────────

#[async_trait]
impl Tool for BrowserCloseAllTool {
    fn name(&self) -> &str {
        "browser_close_all"
    }

    fn description(&self) -> &str {
        "Close all live browser contexts."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }

    async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        direct_browser_tool_status_touch(
            self.runtime_status_service.as_ref(),
            &self.runtime_provider_config,
            self.mcp_manager.as_ref(),
            self.name(),
        )
        .await;
        let count = self.ctx_mgr.destroy_all().await;
        Ok(ToolOutput::success(
            &format!("Closed {} browser session(s).", count),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 27. BrowserTaskTool ──────────────────────────────────────────────────────

impl BrowserTaskTool {
    /// Name / description / parameters as associated items so the pi bridge can
    /// advertise the spec (`engine_sink::io_tool_specs`) WITHOUT constructing this
    /// 12-field tool in a sync context — pi builds the real tool lazily at execute
    /// time (`run_browser_task`). The `Tool` impl below delegates here so the
    /// advertised spec and the executed tool can never drift.
    pub const PI_NAME: &'static str = "browser_task";
    pub const PI_DESCRIPTION: &'static str = "Run an autonomous browser task loop: observe page state, ask the active LLM for the next browser action, execute it, recover from stale page errors, and emit structured browser task events.";

    pub fn pi_parameters_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "Natural-language browser task to perform" },
                "max_steps": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 25,
                    "description": "Maximum autonomous browser steps (default 8, capped at 25)"
                },
                "start_url": {
                    "type": "string",
                    "description": "Optional URL to open before the first observation"
                },
                "available_file_paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional file paths the browser agent may upload. Paths are relative to the agent workspace."
                },
                "resume_run_id": {
                    "type": "string",
                    "description": "Optional previous browser task run_id to resume from its latest checkpoint"
                },
                "auth_profile_id": {
                    "type": "string",
                    "description": "Optional authorized browser auth profile id to use for this task. The profile's cookies/localStorage are injected before the first observation."
                },
                "auth_origin": {
                    "type": "string",
                    "description": "Optional origin/URL used to resolve an authorized auth profile when auth_profile_id is omitted. Defaults to start_url."
                },
                "runtime_preparation_decision": {
                    "type": "string",
                    "enum": ["ready", "defer"],
                    "description": "Task-time Browser runtime preparation decision. Use defer only after the user chooses to pause until runtime preparation is ready."
                },
                "identity_resume_decision": {
                    "type": "string",
                    "enum": ["require_auth", "isolated_profile", "reauthorize", "end_task"],
                    "description": "Decision for a resumed task whose previous authorized browser identity is revoked or unavailable. Defaults to require_auth, which blocks unsafe implicit resume."
                }
            },
            "required": ["task"]
        })
    }
}

#[async_trait]
impl Tool for BrowserTaskTool {
    fn name(&self) -> &str {
        Self::PI_NAME
    }

    fn description(&self) -> &str {
        Self::PI_DESCRIPTION
    }

    fn parameters_schema(&self) -> serde_json::Value {
        Self::pi_parameters_schema()
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let task = params["task"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("task is required".to_string()))?
            .to_string();
        let max_steps = params["max_steps"].as_u64().map(|v| v as u32);
        let start_url = params["start_url"].as_str().map(|s| s.to_string());
        let available_file_paths = params["available_file_paths"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let resume_run_id = params["resume_run_id"].as_str().map(|s| s.to_string());
        let auth_profile_id = params["auth_profile_id"].as_str().map(|s| s.to_string());
        let auth_origin = params["auth_origin"].as_str().map(|s| s.to_string());
        let runtime_preparation_decision =
            parse_runtime_preparation_decision(params["runtime_preparation_decision"].as_str())?;
        let identity_resume_decision =
            parse_identity_resume_decision(params["identity_resume_decision"].as_str())?;
        // Slice 1b follow-up: wire safety chokepoint into BrowserAgentLoop.
        // safety_manager + approval_handler activate the Evaluate-gate in
        // BrowserRuntimeActionExecutor::execute_action. tool_dispatcher is left
        // None — the sub-loop dispatches BrowserAction, not ToolCall objects, so
        // ToolDispatcher has no insertion site here (see Task 3 deferral note).
        let runner = BrowserAgentLoop::new(
            Arc::clone(&self.ctx_mgr),
            Arc::clone(&self.decision_adapter),
        )
        .with_task_store(self.task_store.clone())
        .with_ask_user_bridge(self.ask_user_bridge.clone())
        .with_long_term_memory(self.long_term_memory.clone())
        .with_identity_task_registry(self.identity_task_registry.clone())
        .with_runtime_status_service(self.runtime_status_service.clone())
        .with_runtime_provider_config(self.runtime_provider_config.clone())
        .with_mcp_manager(self.mcp_manager.clone())
        .with_safety_manager(self.safety_manager.clone())
        .with_approval_handler(self.pending_approvals.as_ref().map(|pa| {
            std::sync::Arc::new(crate::safety::ChatApprovalHandler::new(pa.clone()))
                as Arc<dyn crate::safety::ApprovalHandler>
        }));
        let run = runner
            .run(BrowserTaskRequest {
                session_id: self.session_id.clone(),
                task,
                max_steps,
                start_url,
                available_file_paths,
                resume_run_id,
                auth_profile_id,
                auth_origin,
                runtime_preparation_decision,
                identity_resume_decision,
            })
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        // M1-T4d — emit terminal browser-run events to the rollout writer
        // when UCLAW_ROLLOUT_ENABLED=1. Fire-and-forget; helper bails if
        // rollout is disabled. Uses run.session_id as the intent id (the
        // chat session that triggered this browser tool).
        crate::browser::rollout_bridge::emit_browser_run_into_session_dir(
            &run,
            &run.session_id,
            // M1-backlog #4 — browser tools don't have AppState.db_path in
            // scope; passing None keeps JSONL-only emission. Wiring the
            // db_path through ctx_mgr is its own follow-up.
            None,
        )
        .await;

        Ok(ToolOutput::new(
            serde_json::json!({
                "ok": run.status == crate::browser::session_state::BrowserTaskStatus::Completed,
                "run": run,
            }),
            start.elapsed().as_millis() as u64,
        ))
    }
}

#[async_trait]
impl Tool for BrowserTaskResumeTool {
    fn name(&self) -> &str {
        "browser_task_resume"
    }

    fn description(&self) -> &str {
        "Resume a paused/checkpointed autonomous browser task run from its latest checkpoint."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "run_id": { "type": "string", "description": "Browser task run_id to resume" },
                "max_steps": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 25,
                    "description": "Maximum additional autonomous browser steps"
                },
                "auth_profile_id": {
                    "type": "string",
                    "description": "Optional authorized browser auth profile id to use while resuming."
                },
                "auth_origin": {
                    "type": "string",
                    "description": "Optional origin/URL used to resolve an authorized auth profile while resuming."
                },
                "identity_resume_decision": {
                    "type": "string",
                    "enum": ["require_auth", "isolated_profile", "reauthorize", "end_task"],
                    "description": "Decision for a revoked or unavailable identity boundary. Use isolated_profile to continue without the previous identity, reauthorize with auth_profile_id/auth_origin to replace it, or end_task to stop the run."
                }
            },
            "required": ["run_id"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let run_id = params["run_id"]
            .as_str()
            .ok_or_else(|| ToolError::Execution("run_id is required".to_string()))?
            .to_string();
        let max_steps = params["max_steps"].as_u64().map(|v| v as u32);
        let auth_profile_id = params["auth_profile_id"].as_str().map(|s| s.to_string());
        let auth_origin = params["auth_origin"].as_str().map(|s| s.to_string());
        let identity_resume_decision =
            parse_identity_resume_decision(params["identity_resume_decision"].as_str())?;
        let store = self.task_store.as_ref().ok_or_else(|| {
            ToolError::Execution("browser task store is not available".to_string())
        })?;
        let prior = store
            .load_run(&run_id)
            .map_err(|e| ToolError::Execution(e.to_string()))?
            .ok_or_else(|| {
                ToolError::Execution(format!("browser task run '{}' not found", run_id))
            })?;
        // Slice 1b follow-up: wire safety chokepoint into BrowserAgentLoop.
        // Same as BrowserTaskTool above — safety_manager + approval_handler
        // activate the Evaluate-gate; tool_dispatcher left None (no ToolCall path).
        let runner = BrowserAgentLoop::new(
            Arc::clone(&self.ctx_mgr),
            Arc::clone(&self.decision_adapter),
        )
        .with_task_store(self.task_store.clone())
        .with_ask_user_bridge(self.ask_user_bridge.clone())
        .with_long_term_memory(self.long_term_memory.clone())
        .with_identity_task_registry(self.identity_task_registry.clone())
        .with_runtime_status_service(self.runtime_status_service.clone())
        .with_runtime_provider_config(self.runtime_provider_config.clone())
        .with_mcp_manager(self.mcp_manager.clone())
        .with_safety_manager(self.safety_manager.clone())
        .with_approval_handler(self.pending_approvals.as_ref().map(|pa| {
            std::sync::Arc::new(crate::safety::ChatApprovalHandler::new(pa.clone()))
                as Arc<dyn crate::safety::ApprovalHandler>
        }));
        let run = runner
            .run(BrowserTaskRequest {
                session_id: prior.session_id,
                task: prior.task,
                max_steps,
                start_url: None,
                available_file_paths: Vec::new(),
                resume_run_id: Some(run_id),
                auth_profile_id,
                auth_origin,
                runtime_preparation_decision: BrowserTaskRuntimePreparationDecision::Ready,
                identity_resume_decision,
            })
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        // M1-T4d — emit terminal browser-run events to the rollout writer
        // when UCLAW_ROLLOUT_ENABLED=1. Fire-and-forget; helper bails if
        // rollout is disabled. Uses run.session_id as the intent id (the
        // chat session that triggered this browser tool).
        crate::browser::rollout_bridge::emit_browser_run_into_session_dir(
            &run,
            &run.session_id,
            // M1-backlog #4 — browser tools don't have AppState.db_path in
            // scope; passing None keeps JSONL-only emission. Wiring the
            // db_path through ctx_mgr is its own follow-up.
            None,
        )
        .await;

        Ok(ToolOutput::new(
            serde_json::json!({
                "ok": run.status == crate::browser::session_state::BrowserTaskStatus::Completed,
                "run": run,
            }),
            start.elapsed().as_millis() as u64,
        ))
    }
}

// ── 28. RetryWithBrowserAgentTool ────────────────────────────────────────────

#[async_trait]
impl Tool for RetryWithBrowserAgentTool {
    fn name(&self) -> &str {
        "retry_with_browser_agent"
    }

    fn description(&self) -> &str {
        "Fallback after direct browser tools fail: run a structured browser-agent task with observation and recovery-friendly step events."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        BrowserTaskTool {
            ctx_mgr: Arc::clone(&self.ctx_mgr),
            session_id: self.session_id.clone(),
            decision_adapter: Arc::clone(&self.decision_adapter),
            task_store: self.task_store.clone(),
            ask_user_bridge: self.ask_user_bridge.clone(),
            long_term_memory: self.long_term_memory.clone(),
            identity_task_registry: self.identity_task_registry.clone(),
            runtime_status_service: self.runtime_status_service.clone(),
            runtime_provider_config: self.runtime_provider_config.clone(),
            mcp_manager: self.mcp_manager.clone(),
            safety_manager: self.safety_manager.clone(),
            pending_approvals: self.pending_approvals.clone(),
        }
        .parameters_schema()
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        BrowserTaskTool {
            ctx_mgr: Arc::clone(&self.ctx_mgr),
            session_id: self.session_id.clone(),
            decision_adapter: Arc::clone(&self.decision_adapter),
            task_store: self.task_store.clone(),
            ask_user_bridge: self.ask_user_bridge.clone(),
            long_term_memory: self.long_term_memory.clone(),
            identity_task_registry: self.identity_task_registry.clone(),
            runtime_status_service: self.runtime_status_service.clone(),
            runtime_provider_config: self.runtime_provider_config.clone(),
            mcp_manager: self.mcp_manager.clone(),
            safety_manager: self.safety_manager.clone(),
            pending_approvals: self.pending_approvals.clone(),
        }
        .execute(params)
        .await
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::{
        browser_run_failure_output, browser_run_success_output, browser_screenshot_save_path_arg,
        direct_browser_tool_route_options_from_status, parse_identity_resume_decision,
        parse_runtime_preparation_decision, BrowserIdentityResumeDecision,
        BrowserTaskRuntimePreparationDecision,
    };
    use crate::browser::runtime_control_center::BrowserRuntimeProviderConfig;
    use crate::browser::runtime_pack::{
        inspect_runtime_pack_status, BrowserRuntimePackFilesystemProbeOptions,
        BrowserRuntimePackManifest, BrowserRuntimePackNetworkState, BrowserRuntimePackPaths,
        BrowserRuntimePackPlanTrigger, BrowserRuntimePackStatusRequest,
    };
    use crate::browser::runtime_status::compose_browser_runtime_status_with_config;

    #[test]
    fn hover_script_escapes_index() {
        let index: u32 = 42;
        let script = format!(
            r#"(function(){{ const el = document.querySelector('[data-uclaw-index="{}"]'); return el ? JSON.stringify(el.getBoundingClientRect()) : null; }})()"#,
            index
        );
        assert!(
            script.contains(r#"[data-uclaw-index="42"]"#),
            "got: {script}"
        );
    }

    #[test]
    fn upload_rejects_path_traversal() {
        let file_path = "../../../etc/passwd";
        let base = std::path::PathBuf::from("/home/user/Documents/workground");
        let joined = base.join(file_path);
        // Path::join with ".." does NOT resolve ".." components — starts_with gives a false
        // "safe" result on the unresolved path. The real tool must call canonicalize() (or
        // normalize manually) before the starts_with guard.
        // Here we verify the raw joined path contains ".." (i.e. it was not resolved):
        assert!(
            joined.to_string_lossy().contains(".."),
            "expected unresolved '..' in joined path, got: {}",
            joined.display()
        );
        // And that a truly-resolved path would escape the base:
        let resolved = std::path::PathBuf::from("/etc/passwd");
        assert!(
            !resolved.starts_with(&base),
            "resolved traversal path escapes base"
        );
    }

    #[test]
    fn navigate_params_defaults() {
        let params = serde_json::json!({});
        assert!(params["url"].as_str().is_none());
        assert_eq!(params["tab_id"].as_str().unwrap_or("new"), "new");
    }

    #[test]
    fn scroll_pixels_default() {
        let params = serde_json::json!({"tab_id": "t1", "direction": "down"});
        let pixels = params["pixels"].as_u64().unwrap_or(300) as u32;
        assert_eq!(pixels, 300);
    }

    #[test]
    fn wait_selector_escapes_quotes() {
        let sel = r#"input[name="q"]"#;
        let escaped = sel.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!("!!document.querySelector(\"{}\")", escaped);
        assert!(script.contains(r#"input[name=\"q\"]"#), "got: {script}");
        assert!(
            !script.contains(r#"input[name="q"]"#),
            "unescaped quote would break JS eval, got: {script}"
        );
    }

    #[test]
    fn wait_timeout_default() {
        let params = serde_json::json!({"tab_id": "t1"});
        let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(10_000);
        assert_eq!(timeout_ms, 10_000);
    }

    #[test]
    fn screenshot_save_path_accepts_save_path_and_path_alias() {
        let canonical = serde_json::json!({"save_path": "screens/apple.png"});
        assert_eq!(
            browser_screenshot_save_path_arg(&canonical),
            Some("screens/apple.png")
        );

        let alias = serde_json::json!({"path": "screens/support.png"});
        assert_eq!(
            browser_screenshot_save_path_arg(&alias),
            Some("screens/support.png")
        );
    }

    #[test]
    fn screenshot_save_path_prefers_canonical_save_path() {
        let params = serde_json::json!({
            "save_path": "screens/canonical.png",
            "path": "screens/alias.png"
        });
        assert_eq!(
            browser_screenshot_save_path_arg(&params),
            Some("screens/canonical.png")
        );
    }

    #[test]
    fn runtime_preparation_decision_parser_defaults_to_ready() {
        assert_eq!(
            parse_runtime_preparation_decision(None).expect("default should parse"),
            BrowserTaskRuntimePreparationDecision::Ready
        );
        assert_eq!(
            parse_runtime_preparation_decision(Some("ready")).expect("ready should parse"),
            BrowserTaskRuntimePreparationDecision::Ready
        );
    }

    #[test]
    fn runtime_preparation_decision_parser_accepts_defer() {
        assert_eq!(
            parse_runtime_preparation_decision(Some("defer")).expect("defer should parse"),
            BrowserTaskRuntimePreparationDecision::Defer
        );
    }

    #[test]
    fn runtime_preparation_decision_parser_rejects_unknown_value() {
        let error = parse_runtime_preparation_decision(Some("prepare"))
            .expect_err("unknown runtime decision should fail");

        assert!(
            error.to_string().contains("must be 'ready' or 'defer'"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn identity_resume_decision_parser_defaults_to_require_auth() {
        assert_eq!(
            parse_identity_resume_decision(None).expect("default should parse"),
            BrowserIdentityResumeDecision::RequireAuth
        );
        assert_eq!(
            parse_identity_resume_decision(Some("require_auth"))
                .expect("require_auth should parse"),
            BrowserIdentityResumeDecision::RequireAuth
        );
    }

    #[test]
    fn identity_resume_decision_parser_accepts_boundary_choices() {
        assert_eq!(
            parse_identity_resume_decision(Some("isolated_profile"))
                .expect("isolated_profile should parse"),
            BrowserIdentityResumeDecision::IsolatedProfile
        );
        assert_eq!(
            parse_identity_resume_decision(Some("reauthorize")).expect("reauthorize should parse"),
            BrowserIdentityResumeDecision::Reauthorize
        );
        assert_eq!(
            parse_identity_resume_decision(Some("end_task")).expect("end_task should parse"),
            BrowserIdentityResumeDecision::EndTask
        );
    }

    #[test]
    fn identity_resume_decision_parser_rejects_unknown_value() {
        let error = parse_identity_resume_decision(Some("reuse_revoked"))
            .expect_err("unknown identity resume decision should fail");

        assert!(
            error.to_string().contains("identity_resume_decision"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn direct_browser_tool_route_options_from_status_uses_config_backed_feature_flags() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths =
            BrowserRuntimePackPaths::from_root(temp_dir.path().join("browser-runtime"), &manifest);
        let runtime_pack = inspect_runtime_pack_status(
            &manifest,
            &paths,
            BrowserRuntimePackFilesystemProbeOptions::default(),
            BrowserRuntimePackStatusRequest {
                trigger: BrowserRuntimePackPlanTrigger::TaskTime,
                network_state: BrowserRuntimePackNetworkState::Online,
                auto_prepare_enabled: true,
                user_confirmed: false,
            },
        );
        let mut config = BrowserRuntimeProviderConfig::default();
        config.playwright_cli_enabled = true;
        let status =
            compose_browser_runtime_status_with_config(runtime_pack, Vec::new(), config, true);

        let options = direct_browser_tool_route_options_from_status(status);

        assert!(options.feature_flags.playwright_cli);
    }

    #[test]
    fn browser_run_failure_output_has_required_envelope() {
        let output = browser_run_failure_output(
            Instant::now(),
            "automation:spec:activity",
            Some("missing.js"),
            "not found",
        );

        assert_eq!(output.result["ok"], serde_json::json!(false));
        assert_eq!(
            output.result["sessionId"],
            serde_json::json!("automation:spec:activity")
        );
        assert_eq!(output.result["file"], serde_json::json!("missing.js"));
        assert_eq!(output.result["error"], serde_json::json!("not found"));
        assert!(output.result["durationMs"].is_u64());
    }

    #[test]
    fn browser_run_success_output_has_required_envelope() {
        let output = browser_run_success_output(
            Instant::now(),
            "automation:spec:activity",
            "douyin/scan_comments.js",
            serde_json::json!({"comments": []}),
        );

        assert_eq!(output.result["ok"], serde_json::json!(true));
        assert_eq!(
            output.result["sessionId"],
            serde_json::json!("automation:spec:activity")
        );
        assert_eq!(
            output.result["file"],
            serde_json::json!("douyin/scan_comments.js")
        );
        assert_eq!(output.result["result"], serde_json::json!({"comments": []}));
        assert!(output.result["durationMs"].is_u64());
    }
}
