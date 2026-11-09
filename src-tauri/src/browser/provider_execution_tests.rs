use super::*;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use crate::browser::context_manager::BrowserContextManager;
use crate::browser::provider::BrowserProviderRouteDecisionStatus;
use crate::browser::runtime_contracts::BrowserRuntimeFeatureFlags;
use crate::browser::runtime_pack::{
    diagnose_runtime_pack, plan_runtime_pack_operation, BrowserRuntimePackAction,
    BrowserRuntimePackFilesystemProbeReport, BrowserRuntimePackFilesystemSnapshot,
    BrowserRuntimePackManifest, BrowserRuntimePackManifestLoadOutcome,
    BrowserRuntimePackManifestLoadStatus, BrowserRuntimePackNetworkState,
    BrowserRuntimePackOperation, BrowserRuntimePackOperationRequest, BrowserRuntimePackPaths,
    BrowserRuntimePackPlanTrigger, BrowserRuntimePackProbe, BrowserRuntimePackStatusReport,
};

#[test]
fn live_provider_route_selects_local_chromium_for_click() {
    let action = BrowserAction::Click {
        tab_id: "tab-1".to_string(),
        index: 3,
    };

    let decision = route_live_browser_action_provider(&action);

    assert_eq!(
        decision.status,
        BrowserProviderRouteDecisionStatus::Selected
    );
    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(LOCAL_CHROMIUM_PROVIDER_ID)
    );
    assert!(decision.event_intents.iter().any(|intent| {
        intent.event_name.as_str() == "browser.provider.selected"
            && intent.provider_id.as_deref() == Some(LOCAL_CHROMIUM_PROVIDER_ID)
    }));
    assert!(!provider_route_blocks_local_action(&decision));
}

#[test]
fn provider_selection_maps_get_state_to_snapshot_request() {
    let action = BrowserAction::GetState {
        tab_id: "tab-1".to_string(),
        include_screenshot: true,
        include_visual: true,
    };

    let selection = provider_selection_request_for_action(&action);

    assert_eq!(selection.action.as_deref(), Some("screenshot"));
    assert_eq!(selection.observation_mode.as_deref(), Some("screenshot"));
    assert!(!selection.requires_mcp_specific_capability);
}

#[test]
fn evaluate_route_preserves_local_registry_without_raw_provider_promotion() {
    let action = BrowserAction::Evaluate {
        tab_id: "tab-1".to_string(),
        script: "document.title".to_string(),
    };

    let selection = provider_selection_request_for_action(&action);
    let decision = route_live_browser_action_provider(&action);

    assert!(selection.action.is_none());
    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(LOCAL_CHROMIUM_PROVIDER_ID)
    );
}

#[test]
fn non_local_provider_route_blocks_local_action_registry() {
    let decision = BrowserProviderRouteDecision {
        status: BrowserProviderRouteDecisionStatus::Selected,
        selected_provider_id: Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID.to_string()),
        candidates: Vec::new(),
        event_intents: Vec::new(),
        skipped_providers: Vec::new(),
    };

    assert!(provider_route_blocks_local_action(&decision));
}

#[test]
fn cli_candidate_can_be_selected_when_enabled_ready_and_local_disabled() {
    let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_cli = true;
    let options = BrowserProviderActionRouteOptions::default()
        .with_feature_flags(flags)
        .with_runtime_report(ready_runtime_report())
        .with_disabled_provider(LOCAL_CHROMIUM_PROVIDER_ID);
    let action = BrowserAction::Navigate {
        tab_id: Some("tab-1".to_string()),
        url: "https://example.test".to_string(),
    };

    let decision = route_live_browser_action_provider_with_options(&action, &options);

    assert_eq!(
        decision.status,
        BrowserProviderRouteDecisionStatus::Selected
    );
    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
    );
    assert!(decision.candidates.iter().any(|candidate| {
        candidate.provider_id == crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID && candidate.eligible
    }));
}

#[test]
fn active_control_center_cli_route_preempts_local_chromium() {
    let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_cli = true;
    let options = BrowserProviderActionRouteOptions::default()
        .with_feature_flags(flags)
        .with_runtime_report(ready_runtime_report())
        .with_active_control_center_route(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID, Vec::new());
    let action = BrowserAction::Navigate {
        tab_id: Some("tab-1".to_string()),
        url: "https://example.test".to_string(),
    };

    let decision = route_live_browser_action_provider_with_options(&action, &options);

    assert_eq!(
        decision.status,
        BrowserProviderRouteDecisionStatus::Selected
    );
    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
    );
    assert!(decision.candidates.iter().any(|candidate| {
        candidate.provider_id == crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID && candidate.eligible
    }));
}

#[test]
fn active_control_center_cli_route_does_not_fall_back_for_unsupported_action() {
    let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_cli = true;
    let options = BrowserProviderActionRouteOptions::default()
        .with_feature_flags(flags)
        .with_runtime_report(ready_runtime_report())
        .with_active_control_center_route(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID, Vec::new());
    let action = BrowserAction::Scroll {
        tab_id: "tab-1".to_string(),
        direction: "down".to_string(),
        pixels: Some(300),
        index: None,
    };

    let decision = route_live_browser_action_provider_with_options(&action, &options);

    assert_eq!(
        decision.status,
        BrowserProviderRouteDecisionStatus::Selected
    );
    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
    );
}

#[test]
fn mcp_selected_route_is_not_blocked_as_local_registry() {
    let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_mcp = true;
    let options = BrowserProviderActionRouteOptions::default()
        .with_feature_flags(flags)
        .with_active_control_center_route(crate::browser::PLAYWRIGHT_MCP_PROVIDER_ID, Vec::new());
    let action = BrowserAction::Navigate {
        tab_id: Some("tab-1".to_string()),
        url: "https://example.test".to_string(),
    };

    let decision = route_live_browser_action_provider_with_options(&action, &options);

    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(crate::browser::PLAYWRIGHT_MCP_PROVIDER_ID)
    );
}

#[test]
fn capability_override_selects_mcp_for_snapshot_need() {
    let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_cli = true;
    flags.playwright_mcp = true;
    let options = BrowserProviderActionRouteOptions::default()
        .with_feature_flags(flags)
        .with_capability_override_reason("locator_discovery_needed");
    let action = BrowserAction::GetState {
        tab_id: "tab-1".to_string(),
        include_screenshot: false,
        include_visual: false,
    };

    let decision = route_live_browser_action_provider_with_options(&action, &options);

    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(crate::browser::PLAYWRIGHT_MCP_PROVIDER_ID)
    );
    assert!(decision.event_intents.iter().any(|intent| {
        intent.provider_id.as_deref() == Some(crate::browser::PLAYWRIGHT_MCP_PROVIDER_ID)
            && intent.reason == "locator_discovery_needed"
    }));
}

#[test]
fn inferred_accessibility_snapshot_override_selects_mcp_even_when_cli_is_first() {
    let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_cli = true;
    flags.playwright_mcp = true;
    let options = BrowserProviderActionRouteOptions::default()
        .with_feature_flags(flags)
        .with_active_control_center_route(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID, Vec::new());
    let action = BrowserAction::GetState {
        tab_id: "tab-1".to_string(),
        include_screenshot: false,
        include_visual: false,
    };

    let decision = route_live_browser_action_provider_with_options(&action, &options);

    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(crate::browser::PLAYWRIGHT_MCP_PROVIDER_ID)
    );
    assert!(decision.event_intents.iter().any(|intent| {
        intent.provider_id.as_deref() == Some(crate::browser::PLAYWRIGHT_MCP_PROVIDER_ID)
            && intent.reason == "accessibility_snapshot_needed"
    }));
}

#[tokio::test]
async fn selected_cli_route_blocks_unsupported_browser_action() {
    let ctx_mgr = Arc::new(BrowserContextManager::new_for_test(
        "/tmp/uclaw-browser-provider-execution-test".into(),
    ));
    let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_cli = true;
    let options = BrowserProviderActionRouteOptions::default()
        .with_feature_flags(flags)
        .with_runtime_report(ready_runtime_report())
        .with_disabled_provider(LOCAL_CHROMIUM_PROVIDER_ID);
    let executor = BrowserProviderActionExecutor::new(ctx_mgr).with_route_options(options);
    let action = BrowserAction::Scroll {
        tab_id: "tab-1".to_string(),
        direction: "down".to_string(),
        pixels: Some(300),
        index: None,
    };
    let route_decision = BrowserProviderRouteDecision {
        status: BrowserProviderRouteDecisionStatus::Selected,
        selected_provider_id: Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID.to_string()),
        candidates: Vec::new(),
        event_intents: Vec::new(),
        skipped_providers: Vec::new(),
    };

    let execution = executor
        .execute_routed_with_identity("session-1", None, action, route_decision)
        .await
        .expect("unsupported selected CLI route should not call local registry");

    assert_eq!(
        execution.route_decision.selected_provider_id.as_deref(),
        Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
    );
    match execution.outcome {
        BrowserProviderActionExecutionOutcome::Blocked(blocked) => {
            assert_eq!(
                blocked.selected_provider_id.as_deref(),
                Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
            );
            assert!(blocked
                .message
                .contains("does not support this browser action"));
        }
        BrowserProviderActionExecutionOutcome::Executed(_) => {
            panic!("unsupported CLI route must not fall through to local Chromium execution");
        }
    }
}

#[tokio::test]
async fn selected_cli_route_uses_official_adapter_without_runtime_pack() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake_cli = temp.path().join("playwright-cli");
    write_executable(
        &fake_cli,
        "#!/bin/sh\nprintf '%s\\n' '### Page' '- Page URL: https://example.test/' '- Page Title: Example Domain' '### Snapshot' '[Snapshot](.playwright-cli/page.yml)'\n",
    );
    let ctx_mgr = Arc::new(BrowserContextManager::new_for_test(
        temp.path().join("contexts"),
    ));
    let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_cli = true;
    let options = BrowserProviderActionRouteOptions::default()
        .with_feature_flags(flags)
        .with_disabled_provider(LOCAL_CHROMIUM_PROVIDER_ID)
        .with_runtime_report(unready_runtime_report())
        .with_playwright_cli_command(fake_cli)
        .with_playwright_cli_cwd(temp.path());
    let executor = BrowserProviderActionExecutor::new(ctx_mgr).with_route_options(options);
    let action = BrowserAction::Navigate {
        tab_id: Some("tab-1".to_string()),
        url: "https://example.test".to_string(),
    };
    let route_decision = executor.route_action(&action);

    let execution = executor
        .execute_routed_with_identity("session-1", None, action, route_decision)
        .await
        .expect("selected CLI route should execute through worker");

    assert_eq!(
        execution.route_decision.selected_provider_id.as_deref(),
        Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
    );
    match execution.outcome {
        BrowserProviderActionExecutionOutcome::Executed(result) => {
            assert!(result.ok);
            assert_eq!(result.action_name, "browser_playwright_cli_navigate");
            assert_eq!(
                result.message.as_deref(),
                Some("Navigated with Playwright CLI. Title: Example Domain")
            );
            assert_eq!(
                result.tab_id.as_deref(),
                Some(
                    PlaywrightCliOfficialCommandConfig::for_uclaw_session("session-1")
                        .tab_id()
                        .as_str()
                )
            );
            let observation = result.observation_json.expect("provider observation");
            assert_eq!(
                observation["providerId"],
                crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID
            );
            assert_eq!(observation["status"], "succeeded");
            assert_eq!(observation["output"]["url"], "https://example.test/");
            assert_eq!(observation["output"]["title"], "Example Domain");
            assert_eq!(observation["routeEvidence"]["runtimePackRequired"], false);
            assert_eq!(
                observation["output"]["routeEvidence"]["source"],
                "official_playwright_cli"
            );
        }
        BrowserProviderActionExecutionOutcome::Blocked(blocked) => {
            panic!("selected CLI route should execute, got blocked: {blocked:?}");
        }
    }
}

#[tokio::test]
async fn selected_cli_route_executes_screenshot_with_filename() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake_cli = temp.path().join("playwright-cli");
    write_executable(
        &fake_cli,
        "#!/bin/sh\nprintf '%s\\n' '### Result' '- [Screenshot of viewport](./shot.png)'\n",
    );
    let ctx_mgr = Arc::new(BrowserContextManager::new_for_test(
        temp.path().join("contexts"),
    ));
    let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_cli = true;
    let options = BrowserProviderActionRouteOptions::default()
        .with_feature_flags(flags)
        .with_disabled_provider(LOCAL_CHROMIUM_PROVIDER_ID)
        .with_runtime_report(unready_runtime_report())
        .with_playwright_cli_command(fake_cli)
        .with_playwright_cli_cwd(temp.path());
    let executor = BrowserProviderActionExecutor::new(ctx_mgr).with_route_options(options);
    let save_path = temp.path().join("workspace").join("apple.png");
    let action = BrowserAction::Screenshot {
        tab_id: "playwright-cli:u-session".to_string(),
        full_page: true,
        save_path: Some(save_path.to_string_lossy().to_string()),
    };
    let route_decision = executor.route_action(&action);

    let execution = executor
        .execute_routed_with_identity("session-1", None, action, route_decision)
        .await
        .expect("selected CLI route should execute screenshot");

    match execution.outcome {
        BrowserProviderActionExecutionOutcome::Executed(result) => {
            assert!(result.ok);
            assert_eq!(result.action_name, "browser_playwright_cli_screenshot");
            let observation = result.observation_json.expect("provider observation");
            assert_eq!(
                observation["output"]["command"]["args"],
                serde_json::json!([
                    format!(
                        "-s={}",
                        PlaywrightCliOfficialCommandConfig::for_uclaw_session("session-1")
                            .session_name
                    ),
                    "screenshot",
                    format!("--filename={}", save_path.to_string_lossy()),
                    "--full-page"
                ])
            );
            assert_eq!(
                observation["output"]["screenshotPath"],
                serde_json::json!(save_path.to_string_lossy())
            );
            assert_eq!(
                observation["artifactRefs"][0],
                format!(
                    "playwright-cli://screenshot/{}",
                    save_path.to_string_lossy()
                )
            );
        }
        BrowserProviderActionExecutionOutcome::Blocked(blocked) => {
            panic!("selected CLI route should execute, got blocked: {blocked:?}");
        }
    }
}

#[test]
fn cli_preview_mirror_success_returns_local_preview_tab_and_preserves_provider_tab() {
    let mut result = BrowserActionResult {
        ok: true,
        action_name: "browser_playwright_cli_navigate".to_string(),
        message: Some("Navigated with Playwright CLI.".to_string()),
        tab_id: Some("playwright-cli:u-1234".to_string()),
        observation_json: Some(serde_json::json!({
            "providerId": crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID,
            "output": {
                "tabId": "playwright-cli:u-1234",
                "url": "https://example.test/"
            }
        })),
        error: None,
        duration_ms: 7,
    };

    apply_preview_mirror_success(
        &mut result,
        Some("playwright-cli:u-1234".to_string()),
        "tab-local-1".to_string(),
    );

    assert_eq!(result.tab_id.as_deref(), Some("tab-local-1"));
    let observation = result.observation_json.expect("observation");
    assert_eq!(observation["providerTabId"], "playwright-cli:u-1234");
    assert_eq!(
        observation["previewMirror"]["source"],
        "local_chromium_mirror"
    );
    assert_eq!(observation["previewMirror"]["previewTabId"], "tab-local-1");
    assert_eq!(
        observation["output"]["providerTabId"],
        "playwright-cli:u-1234"
    );
    assert_eq!(observation["output"]["previewTabId"], "tab-local-1");
    assert_eq!(observation["output"]["tabId"], "playwright-cli:u-1234");
}

#[tokio::test]
async fn failed_cli_route_retries_with_mcp_and_records_route_evidence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake_cli = temp.path().join("playwright-cli");
    write_executable(
        &fake_cli,
        "#!/bin/sh\nprintf '%s\\n' 'simulated cli failure' >&2\nexit 1\n",
    );
    let ctx_mgr = Arc::new(BrowserContextManager::new_for_test(
        temp.path().join("contexts"),
    ));
    let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_cli = true;
    flags.playwright_mcp = true;
    let options = BrowserProviderActionRouteOptions::default()
        .with_feature_flags(flags)
        .with_disabled_provider(LOCAL_CHROMIUM_PROVIDER_ID)
        .with_runtime_report(unready_runtime_report())
        .with_playwright_cli_command(fake_cli)
        .with_playwright_cli_cwd(temp.path());
    let executor = BrowserProviderActionExecutor::new(ctx_mgr).with_route_options(options);
    let action = BrowserAction::Navigate {
        tab_id: Some("tab-1".to_string()),
        url: "https://example.test".to_string(),
    };
    let route_decision = executor.route_action(&action);

    assert_eq!(
        route_decision.selected_provider_id.as_deref(),
        Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
    );

    let execution = executor
        .execute_routed_with_identity("session-1", None, action, route_decision)
        .await
        .expect("failed CLI route should retry through MCP adapter");

    assert_eq!(
        execution.route_decision.status,
        BrowserProviderRouteDecisionStatus::RolledBack
    );
    assert_eq!(
        execution.route_decision.selected_provider_id.as_deref(),
        Some(crate::browser::PLAYWRIGHT_MCP_PROVIDER_ID)
    );
    match execution.outcome {
        BrowserProviderActionExecutionOutcome::Executed(result) => {
            assert!(result.ok);
            assert_eq!(result.action_name, "browser_playwright_mcp_navigate");
            assert_eq!(result.tab_id.as_deref(), Some("playwright-mcp:session-1"));
            let observation = result.observation_json.expect("provider observation");
            assert_eq!(
                observation["providerId"],
                crate::browser::PLAYWRIGHT_MCP_PROVIDER_ID
            );
            assert_eq!(
                observation["routeEvidence"]["fallbackFromProviderId"],
                crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID
            );
            assert_eq!(
                observation["routeEvidence"]["fallbackReason"],
                "playwright_cli_execution_failed"
            );
            assert!(observation["routeEvidence"]["previousProviderError"]
                .as_str()
                .unwrap_or_default()
                .contains("playwright-cli exited with code"));
            assert_eq!(
                observation["output"]["routeEvidence"]["fallbackFromProviderId"],
                crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID
            );
        }
        BrowserProviderActionExecutionOutcome::Blocked(blocked) => {
            panic!("failed CLI route should retry through MCP, got blocked: {blocked:?}");
        }
    }
}

#[tokio::test]
async fn selected_mcp_route_uses_official_adapter_evidence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let ctx_mgr = Arc::new(BrowserContextManager::new_for_test(
        temp.path().join("contexts"),
    ));
    let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_mcp = true;
    let options = BrowserProviderActionRouteOptions::default()
        .with_feature_flags(flags)
        .with_active_control_center_route(crate::browser::PLAYWRIGHT_MCP_PROVIDER_ID, Vec::new());
    let executor = BrowserProviderActionExecutor::new(ctx_mgr).with_route_options(options);
    let action = BrowserAction::Navigate {
        tab_id: Some("tab-1".to_string()),
        url: "https://example.test".to_string(),
    };
    let route_decision = executor.route_action(&action);

    let execution = executor
        .execute_routed_with_identity("session-1", None, action, route_decision)
        .await
        .expect("selected MCP route should execute through adapter");

    assert_eq!(
        execution.route_decision.selected_provider_id.as_deref(),
        Some(crate::browser::PLAYWRIGHT_MCP_PROVIDER_ID)
    );
    match execution.outcome {
        BrowserProviderActionExecutionOutcome::Executed(result) => {
            assert!(result.ok);
            assert_eq!(result.action_name, "browser_playwright_mcp_navigate");
            let observation = result.observation_json.expect("provider observation");
            assert_eq!(
                observation["providerId"],
                crate::browser::PLAYWRIGHT_MCP_PROVIDER_ID
            );
            assert_eq!(observation["mcpToolName"], "browser_navigate");
            assert_eq!(observation["rawToolsExposed"], false);
            assert_eq!(
                observation["output"]["routeEvidence"]["source"],
                "browser_runtime_adapter"
            );
        }
        BrowserProviderActionExecutionOutcome::Blocked(blocked) => {
            panic!("selected MCP route should execute, got blocked: {blocked:?}");
        }
    }
}

#[test]
fn safe_default_route_options_do_not_make_playwright_candidates_eligible() {
    let action = BrowserAction::Navigate {
        tab_id: Some("tab-1".to_string()),
        url: "https://example.test".to_string(),
    };

    let decision = route_live_browser_action_provider(&action);

    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(LOCAL_CHROMIUM_PROVIDER_ID)
    );
    assert!(decision.candidates.iter().any(|candidate| {
        candidate.provider_id == crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID
            && !candidate.eligible
            && candidate.blocked_reason.as_deref() == Some("provider_status_missing")
    }));
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write executable fixture");
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(path).expect("fixture metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("chmod fixture");
    }
}

fn ready_runtime_report() -> BrowserRuntimePackStatusReport {
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths =
        BrowserRuntimePackPaths::from_root(Path::new("/tmp/uclaw-browser-runtime"), &manifest);
    runtime_report_for_paths(manifest, paths)
}

/// A runtime report that is present but not ready, so CLI-route tests use the
/// injected fake `playwright-cli` instead of the machine's installed runtime pack
/// (`execute_playwright_cli_route` falls back to probing `~/.uclaw-pi` otherwise).
fn unready_runtime_report() -> BrowserRuntimePackStatusReport {
    let mut report = ready_runtime_report();
    report.ready = false;
    report.can_run_browser_tasks = false;
    report
}

fn runtime_report_for_paths(
    manifest: BrowserRuntimePackManifest,
    paths: BrowserRuntimePackPaths,
) -> BrowserRuntimePackStatusReport {
    let probe = BrowserRuntimePackProbe::ready();
    let doctor = diagnose_runtime_pack(&manifest, &probe);
    let primary_action = doctor
        .actions
        .first()
        .copied()
        .unwrap_or(BrowserRuntimePackAction::KeepCurrent);
    let operation_plan = plan_runtime_pack_operation(
        &manifest,
        &paths,
        &doctor,
        BrowserRuntimePackOperationRequest {
            operation: BrowserRuntimePackOperation::from_action(primary_action),
            trigger: BrowserRuntimePackPlanTrigger::TaskTime,
            network_state: BrowserRuntimePackNetworkState::Online,
            auto_prepare_enabled: true,
            user_confirmed: false,
            active_tasks: probe.active_tasks,
        },
    );

    BrowserRuntimePackStatusReport {
        manifest_pack_version: manifest.pack_version.clone(),
        runtime_root: paths.runtime_root.clone(),
        current_pack_dir: paths.current_pack_dir.clone(),
        filesystem: BrowserRuntimePackFilesystemProbeReport {
            snapshot: BrowserRuntimePackFilesystemSnapshot {
                current_pack_dir: paths.current_pack_dir,
                previous_pack_dir: None,
                manifest_path: paths.manifest_path.clone(),
                manifest_status: BrowserRuntimePackManifestLoadStatus::Loaded,
                manifest_present: probe.manifest_present,
                node_present: probe.node_present,
                playwright_package_present: probe.playwright_package_present,
                playwright_mcp_package_present: probe.playwright_mcp_package_present,
                worker_script_present: true,
                browser_binary_present: probe.browser_binary_present,
                previous_pack_available: probe.previous_pack_available,
                versions_match: probe.versions_match,
                cache_corrupt: probe.cache_corrupt,
                active_tasks: probe.active_tasks,
                offline: probe.offline,
            },
            probe,
            manifest_load: BrowserRuntimePackManifestLoadOutcome {
                status: BrowserRuntimePackManifestLoadStatus::Loaded,
                path: paths.manifest_path,
                manifest: Some(manifest),
                error: None,
            },
        },
        doctor,
        primary_action,
        operation_plan,
        ready: true,
        can_run_browser_tasks: true,
        event_names: vec!["browser.runtime.status.reported".to_string()],
    }
}
