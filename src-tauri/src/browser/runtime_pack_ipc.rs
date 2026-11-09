//! IPC boundary for Browser runtime-pack status, dry runs, and confirmed installs.

use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::app::AppState;
use crate::error::Error;

use super::playwright_cli::PLAYWRIGHT_CLI_PROVIDER_ID;
use super::playwright_discovery::inspect_playwright_system;
use super::playwright_mcp::PLAYWRIGHT_MCP_PROVIDER_ID;
use super::playwright_setup::{
    execute_playwright_setup_plan_with_runner, plan_playwright_setup, PlaywrightSetupAction,
    PlaywrightSetupExecutionReport, SystemPlaywrightSetupCommandRunner,
};
use super::runtime_control_center::BrowserRuntimeControlCenterReport;
use super::runtime_pack::{
    diagnose_runtime_pack, execute_runtime_pack_plan_dry_run,
    execute_runtime_pack_plan_with_runner, inspect_runtime_pack_status,
    plan_runtime_pack_operation, probe_runtime_pack_filesystem, BrowserRuntimePackAction,
    BrowserRuntimePackExecutionMode, BrowserRuntimePackExecutionReport,
    BrowserRuntimePackExecutionStatus, BrowserRuntimePackExecutorPolicy,
    BrowserRuntimePackFilesystemProbeOptions, BrowserRuntimePackManifest,
    BrowserRuntimePackNetworkState, BrowserRuntimePackOperation,
    BrowserRuntimePackOperationRequest, BrowserRuntimePackPaths, BrowserRuntimePackPlanTrigger,
    BrowserRuntimePackStatusReport, BrowserRuntimePackStatusRequest,
};
use super::runtime_pack_runner::{
    BrowserRuntimePackLocalStepRunner, BrowserRuntimePackPostInstallProbe,
    BrowserRuntimePackRealPostInstallProbe,
};
use super::runtime_pack_source::{
    BrowserRuntimePackSourceResolution, BrowserRuntimePackSourceResolutionStatus,
    BrowserRuntimePackSourceResolver,
};
use super::runtime_provider_probe::{
    append_probe_history, probe_provider_from_status, BrowserRuntimeProviderProbeClock,
    BrowserRuntimeProviderProbeSummary,
};
use super::runtime_status::BrowserRuntimeStatusReport;

#[tauri::command]
pub async fn get_browser_runtime_status(
    state: State<'_, AppState>,
) -> Result<BrowserRuntimeStatusReport, Error> {
    let provider_config = {
        let settings = state.settings.read().await;
        settings.browser_runtime_provider_config.clone()
    };
    state
        .browser_runtime_status_service
        .inspect_with_provider_config(provider_config)
        .await
}

#[tauri::command]
pub async fn get_browser_runtime_control_center(
    state: State<'_, AppState>,
) -> Result<BrowserRuntimeControlCenterReport, Error> {
    let status = get_browser_runtime_status(state).await?;
    Ok(status.control_center)
}

#[tauri::command]
pub async fn set_browser_runtime_provider_enabled(
    state: State<'_, AppState>,
    provider_id: String,
    enabled: bool,
) -> Result<BrowserRuntimeControlCenterReport, Error> {
    {
        let mut settings = state.settings.write().await;
        settings
            .browser_runtime_provider_config
            .set_enabled(&provider_id, enabled)?;
        settings.save(&state.config_path)?;
    }

    get_browser_runtime_control_center(state).await
}

#[tauri::command]
pub async fn set_browser_runtime_provider_priority(
    state: State<'_, AppState>,
    provider_ids: Vec<String>,
) -> Result<BrowserRuntimeControlCenterReport, Error> {
    {
        let mut settings = state.settings.write().await;
        settings
            .browser_runtime_provider_config
            .set_priority(provider_ids)?;
        settings.save(&state.config_path)?;
    }

    get_browser_runtime_control_center(state).await
}

#[tauri::command]
pub async fn set_browser_runtime_mcp_raw_tools_exposed(
    state: State<'_, AppState>,
    exposed: bool,
) -> Result<BrowserRuntimeControlCenterReport, Error> {
    {
        let mut settings = state.settings.write().await;
        settings
            .browser_runtime_provider_config
            .set_playwright_mcp_raw_tools_exposed(exposed);
        settings.save(&state.config_path)?;
    }

    {
        let mut manager = state.mcp_manager.write().await;
        manager
            .set_playwright_mcp_raw_tools_exposed(exposed)
            .map_err(Error::Internal)?;
    }

    get_browser_runtime_control_center(state).await
}

#[tauri::command]
pub async fn run_browser_runtime_provider_probe(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<BrowserRuntimeProviderProbeSummary, Error> {
    let runtime_status = get_browser_runtime_status(state.clone()).await?;
    let provider_ready = provider_ready_for_probe(&runtime_status, &provider_id);
    let summary = probe_provider_from_status(
        &provider_id,
        provider_ready,
        BrowserRuntimeProviderProbeClock::utc_now(),
    );

    {
        let mut settings = state.settings.write().await;
        settings
            .browser_runtime_provider_config
            .provider_probe_cache
            .insert(provider_id.clone(), summary.clone());
        let history = settings
            .browser_runtime_provider_config
            .provider_probe_history
            .remove(&provider_id)
            .unwrap_or_default();
        settings
            .browser_runtime_provider_config
            .provider_probe_history
            .insert(provider_id, append_probe_history(history, summary.clone()));
        settings.save(&state.config_path)?;
    }

    Ok(summary)
}

fn provider_ready_for_probe(status: &BrowserRuntimeStatusReport, provider_id: &str) -> bool {
    match provider_id {
        PLAYWRIGHT_CLI_PROVIDER_ID => status.provider_readiness.playwright_cli.ready,
        PLAYWRIGHT_MCP_PROVIDER_ID => status.provider_readiness.playwright_mcp.ready,
        _ => true,
    }
}

#[tauri::command]
pub async fn run_playwright_setup(
    action: PlaywrightSetupAction,
) -> Result<PlaywrightSetupExecutionReport, Error> {
    tokio::task::spawn_blocking(move || {
        let discovery = inspect_playwright_system();
        let plan = plan_playwright_setup(&discovery, action);
        let mut runner = SystemPlaywrightSetupCommandRunner;
        execute_playwright_setup_plan_with_runner(&plan, &mut runner)
    })
    .await
    .map_err(|e| Error::Internal(format!("playwright setup worker failed: {e}")))
}

#[tauri::command]
pub async fn dry_run_browser_runtime_action(
    action: BrowserRuntimePackAction,
) -> Result<BrowserRuntimePackExecutionReport, Error> {
    dry_run_default_browser_runtime_action(action)
}

#[tauri::command]
pub async fn execute_browser_runtime_action(
    app_handle: AppHandle,
    action: BrowserRuntimePackAction,
    confirmed: bool,
) -> Result<BrowserRuntimePackExecutionReport, Error> {
    let manifest = BrowserRuntimePackManifest::v1_default();
    let bundle_resource_dir = bundle_runtime_pack_source_dir(&app_handle, &manifest);
    execute_default_browser_runtime_action(action, confirmed, bundle_resource_dir)
}

pub fn inspect_default_browser_runtime_status() -> Result<BrowserRuntimePackStatusReport, Error> {
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_uclaw_home(&manifest)?;
    Ok(inspect_browser_runtime_status(&manifest, &paths))
}

pub fn dry_run_default_browser_runtime_action(
    action: BrowserRuntimePackAction,
) -> Result<BrowserRuntimePackExecutionReport, Error> {
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_uclaw_home(&manifest)?;
    Ok(dry_run_browser_runtime_action_for_paths(
        &manifest, &paths, action,
    ))
}

pub fn execute_default_browser_runtime_action(
    action: BrowserRuntimePackAction,
    confirmed: bool,
    bundle_resource_dir: Option<PathBuf>,
) -> Result<BrowserRuntimePackExecutionReport, Error> {
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_uclaw_home(&manifest)?;
    let resolver = BrowserRuntimePackSourceResolver::from_runtime_context(bundle_resource_dir);
    execute_browser_runtime_action_for_paths(
        &manifest,
        &paths,
        action,
        confirmed,
        resolver,
        BrowserRuntimePackRealPostInstallProbe,
    )
}

fn inspect_browser_runtime_status(
    manifest: &BrowserRuntimePackManifest,
    paths: &BrowserRuntimePackPaths,
) -> BrowserRuntimePackStatusReport {
    inspect_runtime_pack_status(
        manifest,
        paths,
        BrowserRuntimePackFilesystemProbeOptions {
            worker_startup_ok: true,
            real_page_probe_ok: true,
            ..BrowserRuntimePackFilesystemProbeOptions::default()
        },
        BrowserRuntimePackStatusRequest {
            trigger: BrowserRuntimePackPlanTrigger::Settings,
            network_state: BrowserRuntimePackNetworkState::Online,
            auto_prepare_enabled: true,
            user_confirmed: false,
        },
    )
}

fn dry_run_browser_runtime_action_for_paths(
    manifest: &BrowserRuntimePackManifest,
    paths: &BrowserRuntimePackPaths,
    action: BrowserRuntimePackAction,
) -> BrowserRuntimePackExecutionReport {
    let filesystem = probe_runtime_pack_filesystem(
        manifest,
        paths,
        BrowserRuntimePackFilesystemProbeOptions::default(),
    );
    let doctor = diagnose_runtime_pack(manifest, &filesystem.probe);
    let operation = BrowserRuntimePackOperation::from_action(action);
    let plan = plan_runtime_pack_operation(
        manifest,
        paths,
        &doctor,
        BrowserRuntimePackOperationRequest {
            operation,
            trigger: BrowserRuntimePackPlanTrigger::Settings,
            network_state: BrowserRuntimePackNetworkState::Online,
            auto_prepare_enabled: true,
            user_confirmed: false,
            active_tasks: doctor.active_tasks,
        },
    );

    deprecated_runtime_pack_report(execute_runtime_pack_plan_dry_run(&plan))
}

fn execute_browser_runtime_action_for_paths<P>(
    manifest: &BrowserRuntimePackManifest,
    paths: &BrowserRuntimePackPaths,
    action: BrowserRuntimePackAction,
    confirmed: bool,
    resolver: BrowserRuntimePackSourceResolver,
    post_install_probe: P,
) -> Result<BrowserRuntimePackExecutionReport, Error>
where
    P: BrowserRuntimePackPostInstallProbe + 'static,
{
    let operation = BrowserRuntimePackOperation::from_action(action);
    if !matches!(
        operation,
        BrowserRuntimePackOperation::Prepare
            | BrowserRuntimePackOperation::Repair
            | BrowserRuntimePackOperation::KeepCurrent
    ) {
        return Ok(blocked_report(
            dry_run_browser_runtime_action_for_paths(manifest, paths, action),
            "Real Browser runtime execution currently supports prepare, repair, and keep_current only.",
        ));
    }

    if !confirmed
        && matches!(
            operation,
            BrowserRuntimePackOperation::Prepare | BrowserRuntimePackOperation::Repair
        )
    {
        return Ok(confirmation_required_report(
            dry_run_browser_runtime_action_for_paths(manifest, paths, action),
        ));
    }

    let filesystem = probe_runtime_pack_filesystem(
        manifest,
        paths,
        BrowserRuntimePackFilesystemProbeOptions::default(),
    );
    let doctor = diagnose_runtime_pack(manifest, &filesystem.probe);
    let plan = plan_runtime_pack_operation(
        manifest,
        paths,
        &doctor,
        BrowserRuntimePackOperationRequest {
            operation,
            trigger: BrowserRuntimePackPlanTrigger::Settings,
            network_state: BrowserRuntimePackNetworkState::Online,
            auto_prepare_enabled: true,
            user_confirmed: confirmed,
            active_tasks: doctor.active_tasks,
        },
    );

    if operation == BrowserRuntimePackOperation::KeepCurrent {
        let mut runner = BrowserRuntimePackLocalStepRunner::new(manifest.clone(), paths.clone())
            .with_post_install_probe(post_install_probe);
        return Ok(deprecated_runtime_pack_report(
            execute_runtime_pack_plan_with_runner(
                &plan,
                BrowserRuntimePackExecutorPolicy {
                    allow_network: false,
                    allow_destructive: false,
                },
                &mut runner,
            ),
        ));
    }

    let source_resolution = resolver.resolve(manifest);
    if source_resolution.status != BrowserRuntimePackSourceResolutionStatus::Found {
        return Ok(deprecated_runtime_pack_report(
            source_resolution_failed_report(
                execute_runtime_pack_plan_dry_run(&plan),
                &source_resolution,
            ),
        ));
    }
    let Some(source_dir) = source_resolution.source_dir.clone() else {
        return Err(Error::Validation(
            "Runtime pack source resolver returned Found without a source_dir.".to_string(),
        ));
    };

    let mut runner = BrowserRuntimePackLocalStepRunner::new(manifest.clone(), paths.clone())
        .with_staging_source_dir(&source_dir)
        .with_post_install_probe(post_install_probe);
    let report = execute_runtime_pack_plan_with_runner(
        &plan,
        BrowserRuntimePackExecutorPolicy {
            allow_network: true,
            allow_destructive: false,
        },
        &mut runner,
    );

    Ok(deprecated_runtime_pack_report(attach_source_evidence(
        report,
        &source_resolution,
    )))
}

fn bundle_runtime_pack_source_dir(
    app_handle: &AppHandle,
    manifest: &BrowserRuntimePackManifest,
) -> Option<PathBuf> {
    app_handle.path().resource_dir().ok().map(|dir| {
        dir.join("browser-runtime-pack")
            .join(&manifest.pack_version)
    })
}

fn confirmation_required_report(
    mut report: BrowserRuntimePackExecutionReport,
) -> BrowserRuntimePackExecutionReport {
    report.mode = BrowserRuntimePackExecutionMode::Managed;
    report.status = BrowserRuntimePackExecutionStatus::RequiresConfirmation;
    report.summary =
        "Managed execution requires explicit confirmation before writing runtime files."
            .to_string();
    report.requires_confirmation = true;
    report.step_reports.clear();
    report
        .event_names
        .push("browser.runtime.execution.confirmation_required".to_string());
    report
}

fn deprecated_runtime_pack_report(
    mut report: BrowserRuntimePackExecutionReport,
) -> BrowserRuntimePackExecutionReport {
    if !report
        .event_names
        .iter()
        .any(|event| event == "browser.runtime_pack.deprecated")
    {
        report
            .event_names
            .push("browser.runtime_pack.deprecated".to_string());
    }
    report
}

fn blocked_report(
    mut report: BrowserRuntimePackExecutionReport,
    summary: impl Into<String>,
) -> BrowserRuntimePackExecutionReport {
    report.mode = BrowserRuntimePackExecutionMode::Managed;
    report.status = BrowserRuntimePackExecutionStatus::Blocked;
    report.summary = summary.into();
    report.step_reports.clear();
    report
        .event_names
        .push("browser.runtime.execution.blocked".to_string());
    report
}

fn source_resolution_failed_report(
    mut report: BrowserRuntimePackExecutionReport,
    resolution: &BrowserRuntimePackSourceResolution,
) -> BrowserRuntimePackExecutionReport {
    report.mode = BrowserRuntimePackExecutionMode::Managed;
    report.status = BrowserRuntimePackExecutionStatus::Failed;
    report.summary = if resolution.validation_errors.is_empty() {
        "Runtime pack source resolution failed.".to_string()
    } else {
        resolution.validation_errors.join("\n")
    };
    report.step_reports.clear();
    attach_source_evidence(report, resolution)
}

fn attach_source_evidence(
    mut report: BrowserRuntimePackExecutionReport,
    resolution: &BrowserRuntimePackSourceResolution,
) -> BrowserRuntimePackExecutionReport {
    report.source_kind = resolution.source_kind.map(|kind| kind.as_str().to_string());
    report.source_dir = resolution.source_dir.clone();
    report
        .event_names
        .push("browser.runtime.source.resolved".to_string());
    report
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::runtime_control_center::BrowserRuntimeProviderConfig;
    use super::super::runtime_pack::BrowserRuntimePackFilesystemProbeOptions;
    use super::super::runtime_pack::{
        BrowserRuntimePackDoctorStatus, BrowserRuntimePackExecutionMode,
        BrowserRuntimePackExecutionStatus, BrowserRuntimePackOperation,
        BrowserRuntimePackPlanStatus,
    };
    use super::super::runtime_pack_runner::BrowserRuntimePackPostInstallProbe;
    use super::super::runtime_pack_source::BrowserRuntimePackSourceResolver;
    use super::super::runtime_provider_probe::BrowserRuntimeProviderProbeState;
    use super::super::runtime_status::compose_browser_runtime_status_with_config;
    use super::*;

    #[test]
    fn status_query_reports_missing_pack_without_creating_runtime_files() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths =
            BrowserRuntimePackPaths::from_root(temp_dir.path().join("browser-runtime"), &manifest);

        let report = inspect_browser_runtime_status(&manifest, &paths);

        assert_eq!(report.manifest_pack_version, manifest.pack_version);
        assert_eq!(report.current_pack_dir, paths.current_pack_dir);
        assert_eq!(report.primary_action, BrowserRuntimePackAction::Prepare);
        assert_eq!(
            report.doctor.status,
            BrowserRuntimePackDoctorStatus::NeedsPrepare
        );
        assert_eq!(
            report.operation_plan.operation,
            BrowserRuntimePackOperation::Prepare
        );
        assert_eq!(
            report.operation_plan.status,
            BrowserRuntimePackPlanStatus::Planned
        );
        assert!(report.operation_plan.uses_network);
        assert!(!report.operation_plan.destructive);
        assert!(!paths.runtime_root.exists());
    }

    #[test]
    fn provider_probe_gate_uses_provider_readiness_not_runtime_pack_readiness() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths =
            BrowserRuntimePackPaths::from_root(temp_dir.path().join("browser-runtime"), &manifest);
        let runtime_pack = inspect_browser_runtime_status(&manifest, &paths);
        let mut config = BrowserRuntimeProviderConfig::default();
        config.playwright_cli_enabled = true;
        config.playwright_mcp_enabled = true;

        let status =
            compose_browser_runtime_status_with_config(runtime_pack, Vec::new(), config, true);

        assert!(!status.runtime_pack.ready);
        assert!(provider_ready_for_probe(
            &status,
            PLAYWRIGHT_CLI_PROVIDER_ID
        ));
        assert!(provider_ready_for_probe(
            &status,
            PLAYWRIGHT_MCP_PROVIDER_ID
        ));

        let cli_probe = probe_provider_from_status(
            PLAYWRIGHT_CLI_PROVIDER_ID,
            provider_ready_for_probe(&status, PLAYWRIGHT_CLI_PROVIDER_ID),
            BrowserRuntimeProviderProbeClock::fixed(1_770_000_000_000),
        );
        let mcp_probe = probe_provider_from_status(
            PLAYWRIGHT_MCP_PROVIDER_ID,
            provider_ready_for_probe(&status, PLAYWRIGHT_MCP_PROVIDER_ID),
            BrowserRuntimeProviderProbeClock::fixed(1_770_000_000_000),
        );

        assert_eq!(cli_probe.state, BrowserRuntimeProviderProbeState::Passed);
        assert_eq!(mcp_probe.state, BrowserRuntimeProviderProbeState::Passed);
    }

    /// Manual diagnostic: dumps this machine's real runtime-pack status, then
    /// panics on purpose so the report is shown. Run with `--ignored`.
    #[test]
    #[ignore = "manual diagnostic: reads the real ~/.uclaw-pi runtime pack and always panics to show output"]
    fn print_real_status_diagnostics() {
        let report = inspect_default_browser_runtime_status().unwrap();
        println!("REAL_BROWSER_STATUS_REPORT_START");
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        println!("REAL_BROWSER_STATUS_REPORT_END");
        panic!("force display stdout");
    }

    #[test]
    fn status_query_serializes_as_frontend_runtime_status_report() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths =
            BrowserRuntimePackPaths::from_root(temp_dir.path().join("browser-runtime"), &manifest);
        let report = inspect_browser_runtime_status(&manifest, &paths);

        let value = serde_json::to_value(&report).expect("serialize report");

        assert_eq!(value["manifestPackVersion"], manifest.pack_version);
        assert_eq!(value["primaryAction"], "prepare");
        assert_eq!(value["doctor"]["status"], "needs_prepare");
        assert_eq!(value["operationPlan"]["status"], "planned");
        assert_eq!(value["eventNames"][0], "browser.runtime.manifest.checked",);
    }

    #[test]
    fn dry_run_action_reports_plan_without_creating_runtime_files() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths =
            BrowserRuntimePackPaths::from_root(temp_dir.path().join("browser-runtime"), &manifest);

        let report = dry_run_browser_runtime_action_for_paths(
            &manifest,
            &paths,
            BrowserRuntimePackAction::Prepare,
        );

        assert_eq!(report.operation, BrowserRuntimePackOperation::Prepare);
        assert_eq!(report.mode, BrowserRuntimePackExecutionMode::DryRun);
        assert_eq!(report.status, BrowserRuntimePackExecutionStatus::Succeeded);
        assert!(report.uses_network);
        assert!(!report.destructive);
        assert!(!paths.runtime_root.exists());
        assert!(report
            .event_names
            .iter()
            .any(|name| name == "browser.runtime.prepare.dry_run_succeeded"));
    }

    #[test]
    fn dry_run_action_serializes_frontend_execution_report() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths =
            BrowserRuntimePackPaths::from_root(temp_dir.path().join("browser-runtime"), &manifest);

        let report = dry_run_browser_runtime_action_for_paths(
            &manifest,
            &paths,
            BrowserRuntimePackAction::Prepare,
        );
        let value = serde_json::to_value(&report).expect("serialize execution report");

        assert_eq!(value["operation"], "prepare");
        assert_eq!(value["mode"], "dry_run");
        assert_eq!(value["destructive"], false);
        assert_eq!(value["stepReports"][0]["status"], "would_run");
        assert!(value["artifactId"]
            .as_str()
            .expect("artifact id")
            .contains("prepare"));
        assert!(value["artifactId"]
            .as_str()
            .expect("artifact id")
            .contains("succeeded"));
    }

    fn write_source_fixture(source_dir: &std::path::Path, manifest: &BrowserRuntimePackManifest) {
        fs::create_dir_all(source_dir.join("node/bin")).expect("node parent");
        fs::write(source_dir.join("node/bin/node"), "node").expect("node");
        fs::create_dir_all(source_dir.join("node_modules/playwright")).expect("playwright");
        fs::create_dir_all(source_dir.join("node_modules/@playwright/mcp")).expect("mcp");
        fs::create_dir_all(source_dir.join("worker")).expect("worker parent");
        fs::write(
            source_dir.join("worker/uclaw-playwright-worker.mjs"),
            "worker",
        )
        .expect("worker");
        fs::create_dir_all(
            source_dir.join("ms-playwright/chromium-1178/chrome-mac/Chromium.app/Contents/MacOS"),
        )
        .expect("chromium parent");
        fs::write(
            source_dir.join(
                "ms-playwright/chromium-1178/chrome-mac/Chromium.app/Contents/MacOS/Chromium",
            ),
            "chromium",
        )
        .expect("chromium");
        fs::write(
            source_dir.join("runtime-pack.manifest.json"),
            serde_json::to_string_pretty(manifest).expect("manifest json"),
        )
        .expect("manifest");
    }

    #[derive(Clone)]
    struct ReadyProbe;

    impl BrowserRuntimePackPostInstallProbe for ReadyProbe {
        fn probe(
            &self,
            _manifest: &BrowserRuntimePackManifest,
            _paths: &BrowserRuntimePackPaths,
        ) -> BrowserRuntimePackFilesystemProbeOptions {
            BrowserRuntimePackFilesystemProbeOptions {
                worker_startup_ok: true,
                real_page_probe_ok: true,
                ..BrowserRuntimePackFilesystemProbeOptions::default()
            }
        }
    }

    #[test]
    fn execute_prepare_requires_confirmation_before_writing_files() {
        let temp = tempfile::tempdir().expect("temp");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let runtime_paths =
            BrowserRuntimePackPaths::from_root(temp.path().join("runtime"), &manifest);
        let source_dir = temp.path().join("source");
        write_source_fixture(&source_dir, &manifest);
        let resolver = BrowserRuntimePackSourceResolver::for_test(Some(source_dir), None, None);

        let report = execute_browser_runtime_action_for_paths(
            &manifest,
            &runtime_paths,
            BrowserRuntimePackAction::Prepare,
            false,
            resolver,
            ReadyProbe,
        )
        .expect("confirmation report");

        assert_eq!(
            report.status,
            BrowserRuntimePackExecutionStatus::RequiresConfirmation
        );
        assert!(!runtime_paths.current_pack_dir.exists());
    }

    #[test]
    fn execute_prepare_installs_from_resolved_source_after_confirmation() {
        let temp = tempfile::tempdir().expect("temp");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let runtime_paths =
            BrowserRuntimePackPaths::from_root(temp.path().join("runtime"), &manifest);
        let source_dir = temp.path().join("source");
        write_source_fixture(&source_dir, &manifest);
        let resolver =
            BrowserRuntimePackSourceResolver::for_test(Some(source_dir.clone()), None, None);

        let report = execute_browser_runtime_action_for_paths(
            &manifest,
            &runtime_paths,
            BrowserRuntimePackAction::Prepare,
            true,
            resolver,
            ReadyProbe,
        )
        .expect("execute report");

        assert_eq!(report.status, BrowserRuntimePackExecutionStatus::Succeeded);
        assert_eq!(report.source_kind.as_deref(), Some("env_override"));
        assert_eq!(report.source_dir.as_deref(), Some(source_dir.as_path()));
        assert!(runtime_paths.manifest_path.exists());
        assert!(runtime_paths.node_binary_path.exists());
    }

    #[test]
    fn execute_prepare_reports_missing_source_without_writing_files() {
        let temp = tempfile::tempdir().expect("temp");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let runtime_paths =
            BrowserRuntimePackPaths::from_root(temp.path().join("runtime"), &manifest);
        let resolver = BrowserRuntimePackSourceResolver::for_test(None, None, None);

        let report = execute_browser_runtime_action_for_paths(
            &manifest,
            &runtime_paths,
            BrowserRuntimePackAction::Prepare,
            true,
            resolver,
            ReadyProbe,
        )
        .expect("execute report");

        assert_eq!(report.status, BrowserRuntimePackExecutionStatus::Failed);
        assert!(report.summary.contains("Runtime pack source not found"));
        assert!(!runtime_paths.current_pack_dir.exists());
    }

    #[test]
    fn execute_reinstall_is_blocked_in_first_real_installer() {
        let temp = tempfile::tempdir().expect("temp");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let runtime_paths =
            BrowserRuntimePackPaths::from_root(temp.path().join("runtime"), &manifest);
        let resolver = BrowserRuntimePackSourceResolver::for_test(None, None, None);

        let report = execute_browser_runtime_action_for_paths(
            &manifest,
            &runtime_paths,
            BrowserRuntimePackAction::Reinstall,
            true,
            resolver,
            ReadyProbe,
        )
        .expect("execute report");

        assert_eq!(report.status, BrowserRuntimePackExecutionStatus::Blocked);
        assert!(!runtime_paths.current_pack_dir.exists());
    }
}
