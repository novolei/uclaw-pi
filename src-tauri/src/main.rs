// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use std::sync::Mutex;
use std::collections::HashMap;
use tauri::Manager;
use tauri::Emitter;
use uclaw_core::app::AppState;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

/// 全局快捷键当前绑定注册表。
/// 存储 shortcut_id -> 当前绑定的 Tauri 格式组合键字符串（如 "Super+Shift+C"）。
pub struct GlobalShortcutRegistry {
    pub bindings: Mutex<HashMap<String, String>>,
}

fn main() {
    // _guard flushes the non-blocking file writer on Drop. Must outlive
    // the rest of main, hence the underscore-prefixed binding here.
    let _guard = uclaw_core::observability::init();
    uclaw_core::observability::install_panic_hook();

    // Bundle 27-C — Unclean shutdown detection. Logs structured WARN
    // if the previous instance died without removing its process.lock
    // (SIGKILL / OOM-kill / panic / forced quit). If another live
    // uClaw instance is detected, log error and abort early.
    let shutdown_lock_path = uclaw_core::observability::shutdown::default_lock_path();
    let previous_shutdown = match uclaw_core::observability::shutdown::check_and_install(
        &shutdown_lock_path,
    ) {
        Ok(state) => state,
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %shutdown_lock_path.display(),
                "[Bundle 27-C] could not install process lock — continuing without unclean-shutdown detection"
            );
            uclaw_core::observability::shutdown::PreviousShutdown::Clean
        }
    };
    if let uclaw_core::observability::shutdown::PreviousShutdown::AnotherInstanceAlive(p) =
        &previous_shutdown
    {
        eprintln!(
            "[uclaw] another live instance detected (pid={}, started_at={}). Refusing to start.",
            p.pid, p.started_at
        );
        std::process::exit(1);
    }
    // RAII guard — Drop on normal exit (and on panic-unwind, when
    // tauri::Builder::run returns or main() ends) removes the lock
    // file so the next boot sees Clean. Only SIGKILL / abort skips
    // Drop — which is exactly the case Bundle 27-C wants detected.
    let _shutdown_guard =
        uclaw_core::observability::shutdown::CleanExitGuard::new(shutdown_lock_path.clone());
    // Bundle 27-A — clone the Unclean lock for recovery payload PID
    // cross-check inside the setup closure. Move out of `_previous`
    // first so we don't accidentally consume `previous_shutdown`
    // here in a way that breaks the AnotherInstance branch above.
    let prev_unclean_lock: Option<uclaw_core::observability::shutdown::ProcessLock> =
        match &previous_shutdown {
            uclaw_core::observability::shutdown::PreviousShutdown::Unclean(p) => {
                Some(p.clone())
            }
            _ => None,
        };
    let _previous_shutdown = previous_shutdown;

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            let app_state = AppState::new(app.handle())?;

            // Bundle 27-A — if Bundle 27-C reported an unclean shutdown,
            // check whether the prior run left a `last_active_run.json`
            // flight record with buffered assistant text. If so, emit
            // `agent:interrupted-recovered` so the UI can render the
            // partial reply as an `[interrupted-recovered]` message —
            // delivering on "真实中断的话也需要把 Agent 的回复补齐".
            //
            // Spawn on the tauri async runtime so we don't block setup.
            // The UI registers the listener on mount, which races with
            // this emit; a small initial delay covers the gap.
            if let Some(prev_lock) = prev_unclean_lock.clone() {
                let app_handle = app.handle().clone();
                let flight_path =
                    uclaw_core::agent::heartbeat::default_flight_path();
                let recovery_store = app_state.pending_recovery.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500))
                        .await;
                    match uclaw_core::agent::recovery::recover_unclean_shutdown(
                        &app_handle,
                        &flight_path,
                        Some(&prev_lock),
                        Some(recovery_store),
                    ) {
                        Ok(report) => {
                            if report.recovered {
                                tracing::warn!(
                                    conversation_id = ?report.conversation_id,
                                    iteration = ?report.iteration,
                                    stage = ?report.stage,
                                    partial_chars = report.partial_chars,
                                    dead_pid = ?report.dead_pid,
                                    "[Bundle 27-A] recovered interrupted assistant reply",
                                );
                            }
                        }
                        Err(e) => tracing::warn!(
                            error = %e,
                            "[Bundle 27-A] flight recovery failed"
                        ),
                    }
                });
            }

            // Attach AppHandle to MemUClient so consolidation events can emit
            if let Some(memu_client) = &app_state.memu_client {
                let memu_client = memu_client.clone();
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    memu_client.set_app_handle(app_handle).await;
                });
            }

            // System tray icon
            let tray = tauri::tray::TrayIconBuilder::new()
                .tooltip("uClaw — AI Coworker")
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left, ..
                    } = event {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;
            let _ = tray; // keep alive — owned by the app

            // Spawn HTTP server for remote access
            let session_mgr = app_state.session_manager.clone();
            let data_dir = app_state.data_dir.clone();
            let jwt_secret = uclaw_core::api::auth::generate_secret();

            let ws_manager = uclaw_core::api::ws::WsConnectionManager::new();

            let http_state = uclaw_core::api::auth::HttpServerState {
                session_manager: session_mgr,
                jwt_secret,
                data_dir,
                ws_manager: ws_manager.clone(),
            };

            let router = uclaw_core::api::router::build_router(http_state);

            // [pi-lightweight] The local HTTP API (memory / `/v1/embeddings` /
            // external access) is an OPTIONAL layer beside the pi kernel, not core —
            // chat & agent run entirely over Tauri IPC. Opt-in via `UCLAW_HTTP_API`,
            // default OFF, so the default startup stays lightweight and a port clash
            // (e.g. the original uClaw) can never crash the app. gbrain does not
            // auto-depend on it; point gbrain's llama-server at :19528/v1 only if you
            // enable this. Bind/serve failures degrade gracefully (log, never panic).
            // Enabled by either the env override OR the persisted `http_api_enabled`
            // setting (toggled in Settings → 系统诊断). Read once at startup; the UI
            // toggle takes effect on the next restart.
            let http_api_enabled = std::env::var_os("UCLAW_HTTP_API").is_some()
                || app_state.db.lock().ok().is_some_and(|conn| {
                    use uclaw_core::services::settings_service::SettingsService as _;
                    uclaw_core::services::settings_service::DbSettings.http_api_enabled(&conn)
                });
            if http_api_enabled {
                std::thread::spawn(move || {
                    let rt = match tokio::runtime::Runtime::new() {
                        Ok(rt) => rt,
                        Err(e) => {
                            tracing::warn!("HTTP API server not started: runtime init failed: {e}");
                            return;
                        }
                    };
                    rt.block_on(async move {
                        // Spawn stale WebSocket connection reaper
                        uclaw_core::api::ws::spawn_stale_connection_reaper(ws_manager);

                        let listener = match tokio::net::TcpListener::bind("127.0.0.1:19528").await {
                            Ok(l) => l,
                            Err(e) => {
                                tracing::warn!(
                                    "HTTP API server not started: bind 127.0.0.1:19528 failed \
                                     (port in use?): {e}"
                                );
                                return;
                            }
                        };
                        tracing::info!(
                            "uClaw HTTP API server listening on http://127.0.0.1:19528 (UCLAW_HTTP_API)"
                        );
                        if let Err(e) = axum::serve(listener, router).await {
                            tracing::warn!("HTTP API server stopped: {e}");
                        }
                    });
                });
            } else {
                tracing::info!(
                    "HTTP API server disabled (optional; set UCLAW_HTTP_API=1 to enable the local \
                     API / embeddings endpoint)"
                );
            }

            app.manage(app_state);

            // ─── [R1 接线] PiEngine — the new pi-based agent backend ──────────
            // pi runs STATELESS (no_session=true): it pulls no sqlmodel-sqlite, so
            // it coexists with uClaw's rusqlite (no F2 data migration — see
            // docs/MIGRATION_GOALS.md §P0). The engine owns a dedicated asupersync
            // thread; its chat:stream-* events reach the frontend via TauriEventSink.
            {
                let sink = uclaw_core::engine_sink::TauriEventSink::new(app.handle().clone());
                // [R4 IO 桥] Pass the uClaw tool-request seam. The stub declares no
                // IO tools yet (built-ins + ExitPlanTool only); the real tokio
                // executor (mcp.rs/browser/skill dispatch → EngineCmd::ToolResult)
                // is the integration that replaces it.
                let tool_request_sink: std::sync::Arc<dyn uclaw_pi_engine::ToolRequestSink> =
                    std::sync::Arc::new(uclaw_core::engine_sink::StubToolRequestSink);
                let pi_engine = uclaw_pi_engine::PiEngine::spawn(
                    sink,
                    Some(tool_request_sink),
                    uclaw_pi_engine::EngineConfig {
                        no_session: true,
                        ..Default::default()
                    },
                );
                app.manage(std::sync::Arc::new(pi_engine));
                tracing::info!("[R1] PiEngine spawned (stateless) and managed");
            }

            // ─── Debug 菜单（仅 debug 模式可见） ────────────────────
            #[cfg(debug_assertions)]
            {
                use tauri::menu::SubmenuBuilder;

                let debug_submenu = SubmenuBuilder::new(app, "Debug")
                    .text("emit-memory-recall", "发射 Memory Recall 测试事件")
                    .text("emit-proactive-learning", "发射 Proactive Learning 测试事件")
                    .text("emit-self-eval", "发射 Self-Eval 测试数据（含 SkillLearned 事件）")
                    .separator()
                    .text("generate-default-config", "生成默认配置文件 (memubot_config.json)")
                    .build()?;

                let menu = app.menu().expect("menu must exist");
                menu.append(&debug_submenu)?;
                tracing::info!("[Debug] Debug menu registered (debug mode only)");
            }

            // ─── Stage 3 & 4：注册后台服务并启动 ──────────────────────
            {
                let state: tauri::State<'_, AppState> = app.state();
                let service_manager = state.service_manager.clone();
                let infra_service = state.infra_service.clone();
                let memubot_config_arc = state.memubot_config.clone();
                let data_dir = state.data_dir.clone();
                let memu_client = state.memu_client.clone();
                let provider_service = state.provider_service.clone();
                let memory_graph_store = state.memory_graph_store.clone();
                let db = state.db.clone();
                let app_handle = app.handle().clone();
                let files_rail_service = state.files_rail_service.clone();
                // M1-T7 — capture llm_config for the in-Stage-3 prewarm spawn.
                let llm_config_arc = state.llm_config.clone();
                let mcp_manager = state.mcp_manager.clone();

                // 在后台异步执行服务注册和启动
                tauri::async_runtime::spawn(async move {
                    // Stage 3: 注册后台服务
                    tracing::info!("[Stage 3] Registering background services...");
                    // Snapshot config at boot — services read their flags once at startup.
                    let memubot_config = memubot_config_arc.read().await.clone();

                    // PowerService
                    if memubot_config.power.prevent_sleep {
                        let power_svc = Arc::new(
                            uclaw_core::services::PowerService::new()
                        );
                        service_manager.register(power_svc).await;
                        tracing::info!("[Stage 3] PowerService registered");
                    }

                    // MemorizationService
                    if memubot_config.memorization.enabled {
                        let mem_db_path = data_dir.join("memorization.db");
                        match uclaw_core::memorization::MemorizationStorage::new(&mem_db_path) {
                            Ok(storage) => {
                                let mem_svc = Arc::new(
                                    uclaw_core::memorization::MemorizationService::new(
                                        memubot_config.memorization.clone(),
                                        Arc::new(storage),
                                        infra_service.clone(),
                                    )
                                );
                                // 注入 mcp_manager
                                mem_svc.set_mcp_manager(Some(mcp_manager.clone())).await;

                                // 注入 MemoryOsLlmClient
                                let os_llm = Arc::new(uclaw_core::memory_graph::memory_os_llm::MemoryOsLlmClient::new(
                                    provider_service.clone(),
                                    db.clone(),
                                ));
                                mem_svc.set_llm_client(Some(os_llm)).await;

                                // 注入 memU 客户端
                                if let Some(ref client) = memu_client {
                                    mem_svc.set_memu_client(Some(client.clone())).await;
                                }
                                // 注入 MemoryGraphStore
                                mem_svc.set_graph_store(memory_graph_store.clone()).await;
                                service_manager.register(mem_svc).await;
                                tracing::info!("[Stage 3] MemorizationService registered");
                            }
                            Err(e) => {
                                tracing::warn!("[Stage 3] MemorizationStorage init failed: {}, skipping", e);
                            }
                        }
                    }

                    // ProactiveService
                    if memubot_config.proactive.enabled {
                        // 构建 ScenarioManager 并注册三个场景
                        let mut scenario_manager = uclaw_core::proactive::scenarios::ScenarioManager::new();

                        // Scenario 1: Always-Learning Assistant
                        if memubot_config.scenarios.conversation_learning.enabled {
                            let scenario = Arc::new(
                                uclaw_core::proactive::scenarios::conversation_learning::ConversationLearningScenario::new(
                                    memubot_config.scenarios.conversation_learning.clone(),
                                )
                            );
                            scenario_manager.register(scenario);
                            tracing::info!("[Stage 3] ConversationLearningScenario registered");
                        }

                        // Scenario 2: Self-Improving Agent (Skill Extraction)
                        if memubot_config.scenarios.skill_extraction.enabled {
                            let scenario = Arc::new(
                                uclaw_core::proactive::scenarios::skill_extraction::SkillExtractionScenario::new(
                                    memubot_config.scenarios.skill_extraction.clone(),
                                )
                            );
                            scenario_manager.register(scenario);
                            tracing::info!("[Stage 3] SkillExtractionScenario registered");
                        }

                        // Scenario 2b: GEP Gene Evolution
                        if memubot_config.gene_evolution.enabled {
                            let scenario = Arc::new(
                                uclaw_core::proactive::scenarios::gene_evolution::GeneEvolutionScenario::new(
                                    memubot_config.gene_evolution.clone(),
                                )
                            );
                            scenario_manager.register(scenario);
                            tracing::info!("[Stage 3] GeneEvolutionScenario registered");
                        }

                        // Scenario 3: Multimodal Context Builder
                        if memubot_config.scenarios.multimodal_context.enabled {
                            let scenario = Arc::new(
                                uclaw_core::proactive::scenarios::multimodal_context::MultimodalContextScenario::new(
                                    memubot_config.scenarios.multimodal_context.clone(),
                                )
                            );
                            scenario_manager.register(scenario);
                            tracing::info!("[Stage 3] MultimodalContextScenario registered");
                        }

                        // Scenario 4: Plan-mode calibration (no LLM; runs DB-only calibration)
                        {
                            let scenario = Arc::new(
                                uclaw_core::proactive::scenarios::plan_mode_calibration::PlanModeCalibrationScenario::new(
                                    db.clone(),
                                )
                            );
                            scenario_manager.register(scenario);
                            tracing::info!("[Stage 3] PlanModeCalibrationScenario registered");
                        }

                        tracing::info!("[Stage 3] {} proactive scenarios registered", scenario_manager.scenario_count());

                        let pro_db_path = data_dir.join("proactive.db");
                        let gep_path = data_dir.join("gep");
                        let gene_repo = match uclaw_core::agent::gep::repository::GeneRepository::new(gep_path) {
                            Ok(repo) => Arc::new(std::sync::Mutex::new(repo)),
                            Err(e) => {
                                tracing::warn!("[Stage 3] GeneRepository init failed: {}, gene evolution disabled", e);
                                // Create a fallback with temp dir to avoid crash
                                Arc::new(std::sync::Mutex::new(
                                    uclaw_core::agent::gep::repository::GeneRepository::new(
                                        std::env::temp_dir().join("uclaw_gep_fallback")
                                    ).unwrap_or_else(|_| panic!("Cannot create GeneRepository fallback"))
                                ))
                            }
                        };
                        match uclaw_core::proactive::ProactiveStorage::new(&pro_db_path) {
                            Ok(storage) => {
                                let pro_svc = Arc::new(
                                    uclaw_core::proactive::ProactiveService::new(
                                        memubot_config.proactive.clone(),
                                        infra_service.clone(),
                                        Arc::new(storage),
                                        Arc::new(scenario_manager),
                                        Arc::new(uclaw_core::proactive::execution_log::ExecutionLogCollector::new()),
                                        Arc::new(uclaw_core::proactive::multimodal::MultimodalQueue::new()),
                                        provider_service.clone(),
                                        memu_client.clone(),
                                        memory_graph_store.clone(),
                                        Some(app_handle.clone()),
                                        db.clone(),
                                        gene_repo,
                                        memubot_config.gene_evolution.clone(),
                                        // Phase 3/4/5 runtime knobs bundled into
                                        // MemoryOsRuntimeConfig — replaces what used
                                        // to be five trailing positional args.
                                        // Future Memory OS phases just add fields
                                        // to the struct without touching this call
                                        // site.
                                        {
                                            let state_ref: tauri::State<'_, AppState> = app_handle.state();
                                            let mut cfg = uclaw_core::proactive::MemoryOsRuntimeConfig::from_memubot_config(
                                                &memubot_config.memory_os,
                                                state_ref.lint_analyzer.clone(),
                                            );
                                            // Sprint 1.10 — wire learning pipeline handles in
                                            // here so ProactiveService tick can drive the
                                            // 30-min stability rebuild + FacetCache refresh.
                                            cfg.learning_scheduler =
                                                Some(state_ref.learning_scheduler.clone());
                                            cfg.facet_cache = Some(state_ref.facet_cache.clone());
                                            // Sprint 2.1a — resolve PROFILE.md disk path
                                            // (lives under the default brain root so it
                                            // ships next to entity pages + can be hand-
                                            // edited / version-controlled by the user).
                                            // `None` when there's no Documents dir (CI,
                                            // unusual setups); tick block skips the disk
                                            // write but the cache + system prompt still
                                            // work normally.
                                            cfg.profile_md_path = uclaw_core::memory_graph::brain_io::BrainExportConfig::default_brain_root()
                                                .map(|root| root.join("PROFILE.md"));
                                            cfg
                                        },
                                        // Bundle 22 — pass uClaw data dir so
                                        // skill_extraction can persist
                                        // SKILL.md under <data_dir>/skills/
                                        // _auto_extracted/.
                                        {
                                            let state_ref: tauri::State<'_, AppState> = app_handle.state();
                                            state_ref.data_dir.clone()
                                        },
                                        // Bundle 23 — pass SkillsRegistry handle so the
                                        // proactive service can trigger an
                                        // immediate disk-tier rescan after auto-persistence.
                                        {
                                            let state_ref: tauri::State<'_, AppState> = app_handle.state();
                                            state_ref.skills_registry.clone()
                                        },
                                    )
                                );
                                // Inject into AppState for tauri_commands access
                                {
                                    let state_ref: tauri::State<'_, AppState> = app_handle.state();
                                    *state_ref.proactive_service.write().await = Some(pro_svc.clone());
                                }
                                service_manager.register(pro_svc).await;
                                tracing::info!("[Stage 3] ProactiveService registered");
                            }
                            Err(e) => {
                                tracing::warn!("[Stage 3] ProactiveStorage init failed: {}, skipping", e);
                            }
                        }
                    }

                    // LocalApiService — also surfaces `/v1/embeddings`
                    // (OpenAI-compatible) backed by memU's FastEmbed when
                    // memU is available, so external tools like gbrain
                    // can reuse the bundled embedding stack without a
                    // separate API key.
                    if memubot_config.local_api.enabled {
                        let local_api_svc = Arc::new(
                            uclaw_core::local_api::LocalApiService::new(
                                memubot_config.local_api.clone(),
                                memu_client.clone(),
                            )
                        );
                        service_manager.register(local_api_svc).await;
                        tracing::info!("[Stage 3] LocalApiService registered");
                    }

                    // FilesRailService — always registered (not gated on memubot_config)
                    service_manager.register(files_rail_service).await;
                    tracing::info!("[Stage 3] FilesRailService registered");

                    // AppRuntimeService — already constructed in AppState::new(); we just
                    // register the shared Arc into ServiceManager here so it gets started.
                    {
                        let state_ref: tauri::State<'_, AppState> = app_handle.state();
                        let app_runtime_svc = state_ref.runtime_service.clone();
                        service_manager.register(app_runtime_svc).await;
                        tracing::info!("[Stage 3] AppRuntimeService registered");
                    }

                    // [R5] SymphonyService 启动块已删（symphony_graph 旧后端删除）。

                    // Start ImChannelManager (load DB instances + start notify senders)
                    {
                        let state_ref: tauri::State<'_, AppState> = app_handle.state();
                        let im_mgr = state_ref.im_channel_manager.clone();
                        let im_reg = state_ref.im_session_registry.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = im_reg.load_from_db().await {
                                tracing::warn!("[Stage 3] ImSessionRegistry load_from_db failed: {}", e);
                            }
                            if let Err(e) = im_mgr.start_all().await {
                                tracing::warn!("[Stage 3] ImChannelManager start_all failed: {}", e);
                            }
                            tracing::info!("[Stage 3] ImChannelManager started");
                        });
                    }

                    // MCP PR-1 — auto-connect every server marked enabled
                    // in mcp_servers.json. Without this, every app launch
                    // requires the user to manually click "重启" on each
                    // server in the Integrations module before any agent
                    // can use MCP tools.
                    //
                    // PR-3 — also start the per-server health/reconnect
                    // background loop for each successful connect.
                    //
                    // PR-4 — wire the notification channel BEFORE the
                    // connect pass so any tools/list_changed fired during
                    // the initial handshake is captured. Consumer task is
                    // spawned alongside; it dispatches by method and
                    // emits Tauri events the frontend listens for.
                    {
                        let state_ref: tauri::State<'_, AppState> = app_handle.state();
                        let mcp_mgr = state_ref.mcp_manager.clone();
                        // PR-5 — pull the DB handle out here so the
                        // spawned task doesn't need to re-acquire State<AppState>
                        // (which would require app_handle to outlive the
                        // task — easier to just clone the Arc once).
                        let db_for_mcp = state_ref.db.clone();
                        let settings_for_mcp = state_ref.settings.clone();
                        let gbrain_mcp_id_slot = state_ref.gbrain_mcp_id.clone();
                        // Sprint 2.2.5b — diagnostics slot for init outcome.
                        let gbrain_init_status_slot = state_ref.gbrain_init_status.clone();
                        // gbrain Sprint 2.1 init-fix — resolve bundled
                        // artifacts. Returning Option lets the seed below
                        // skip cleanly if the bundle is missing (fresh
                        // checkout without scripts/setup-bun-runtime.sh +
                        // scripts/setup-gbrain-source.sh).
                        let resource_dir_for_mcp: Option<std::path::PathBuf> =
                            app_handle.path().resource_dir().ok();
                        let bun_path = AppState::find_bun_path(resource_dir_for_mcp.as_deref());
                        let gbrain_entry =
                            AppState::find_gbrain_entry(resource_dir_for_mcp.as_deref());
                        let gbrain_home = state_ref.data_dir.join("gbrain");
                        let playwright_mcp_workspace_root =
                            state_ref.active_workspace_root_or_default();
                        let app_for_consumer = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            let (tx, mut rx) =
                                tokio::sync::mpsc::unbounded_channel::<
                                    uclaw_core::mcp::McpNotificationEvent,
                                >();
                            // Consumer task — drains the channel for the
                            // lifetime of the app. tools/list_changed
                            // refreshes the manifest + emits a Tauri
                            // event the Integrations module listens for.
                            let consumer_mgr = mcp_mgr.clone();
                            let consumer_app = app_for_consumer.clone();
                            tauri::async_runtime::spawn(async move {
                                while let Some(event) = rx.recv().await {
                                    if event.method
                                        == uclaw_core::mcp::NOTIFY_TOOLS_LIST_CHANGED
                                    {
                                        let id = event.server_id.clone();
                                        let refresh_result = {
                                            let mut m = consumer_mgr.write().await;
                                            m.refresh_tools(&id).await
                                        };
                                        match refresh_result {
                                            Ok(tools) => {
                                                use tauri::Emitter;
                                                let _ = consumer_app.emit(
                                                    "mcp:tools-changed",
                                                    serde_json::json!({
                                                        "serverId": id,
                                                        "toolCount": tools.len(),
                                                    }),
                                                );
                                                tracing::info!(
                                                    "[mcp-notif] {} tools/list_changed → {} tools",
                                                    id,
                                                    tools.len()
                                                );
                                            }
                                            Err(e) => {
                                                tracing::warn!(
                                                    "[mcp-notif] {} refresh failed: {}",
                                                    id,
                                                    e
                                                );
                                            }
                                        }
                                    } else {
                                        tracing::debug!(
                                            "[mcp-notif] {} {}",
                                            event.server_id,
                                            event.method
                                        );
                                    }
                                }
                                tracing::debug!("[mcp-notif] consumer task ended");
                            });

                            let shared = mcp_mgr.clone();
                            let mut guard = mcp_mgr.write().await;
                            guard.set_notification_tx(tx);
                            guard.set_runtime_working_dir(
                                "playwright",
                                Some(playwright_mcp_workspace_root.clone()),
                            );
                            // PR-5 — install the DB handle pulled out
                            // before spawning so the audit-log writes
                            // share the same SQLite connection AppState
                            // owns.
                            guard.set_db_handle(db_for_mcp);
                            // gbrain Sprint 2.1 init-fix — initialize the
                            // bundled brain BEFORE seeding the MCP entry.
                            // Without this, gbrain serve exits immediately
                            // on every connect with "No brain configured".
                            // Synchronous + blocking: first launch runs
                            // ~63 migrations (~30-60s); subsequent
                            // launches short-circuit via the PG_VERSION
                            // probe.
                            if let (Some(bun), Some(entry)) =
                                (bun_path.as_ref(), gbrain_entry.as_ref())
                            {
                                // Sprint 2.2 — write self-describing launcher
                                // files BEFORE init. Best-effort: a write
                                // failure shouldn't abort the gbrain seed.
                                if let Err(e) = uclaw_core::app::AppState::write_gbrain_launcher_files(
                                    &gbrain_home, bun, entry,
                                ) {
                                    tracing::warn!(
                                        error = %e,
                                        "[Stage 3] failed to write ~/.uclaw/gbrain/{{run.sh,paths.json}}"
                                    );
                                }
                                // Sprint 2.2.5b — flip status to InProgress
                                // before the spawn so the diagnostics tab can
                                // show "running" if the user opens Settings
                                // during the 30-60s first-launch window.
                                let init_started = std::time::Instant::now();
                                *gbrain_init_status_slot.lock().unwrap() =
                                    uclaw_core::mcp::GbrainInitStatus::InProgress;
                                let init_outcome = uclaw_core::mcp::ensure_bundled_gbrain_initialized(
                                    bun, entry, &gbrain_home,
                                )
                                .await;
                                let now_ms = chrono::Utc::now().timestamp_millis();
                                match &init_outcome {
                                    Ok(true) => {
                                        let duration_ms =
                                            init_started.elapsed().as_millis() as u64;
                                        tracing::info!(
                                            duration_ms,
                                            "[Stage 3] gbrain brain initialized (first launch)"
                                        );
                                        *gbrain_init_status_slot.lock().unwrap() =
                                            uclaw_core::mcp::GbrainInitStatus::Succeeded {
                                                duration_ms,
                                                at_ms: now_ms,
                                            };
                                    }
                                    Ok(false) => {
                                        tracing::debug!(
                                            "[Stage 3] gbrain brain already initialized"
                                        );
                                        *gbrain_init_status_slot.lock().unwrap() =
                                            uclaw_core::mcp::GbrainInitStatus::SkippedAlreadyInitialized {
                                                at_ms: now_ms,
                                            };
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            error = %e,
                                            "[Stage 3] gbrain init failed — seeding the MCP entry anyway so it surfaces in the UI for debugging; gbrain will fail to connect until init succeeds (re-run scripts/init-gbrain.sh or remove ~/.uclaw/gbrain/ to retry)"
                                        );
                                        // Extract stderr-tail from the
                                        // error message when present
                                        // (ensure_bundled_gbrain_initialized
                                        // emits a "stderr (last 20 lines):\n…"
                                        // section on subprocess failures).
                                        let stderr_tail = e
                                            .split_once("stderr (last 20 lines):\n")
                                            .map(|(_, t)| t.to_string());
                                        let error_summary = e
                                            .lines()
                                            .next()
                                            .unwrap_or("gbrain init failed")
                                            .to_string();
                                        *gbrain_init_status_slot.lock().unwrap() =
                                            uclaw_core::mcp::GbrainInitStatus::Failed {
                                                error: error_summary,
                                                stderr_tail,
                                                at_ms: now_ms,
                                            };
                                        // Intentional fall-through: still
                                        // run the seed match below so the
                                        // entry appears in the Integrations
                                        // UI. The auto-connect pass will
                                        // then surface a connect failure
                                        // (more actionable than a silently
                                        // missing entry).
                                    }
                                }
                                match guard.seed_bundled_gbrain(bun, entry, &gbrain_home) {
                                    Ok(seeded) => {
                                        if seeded {
                                            tracing::info!(
                                                "[Stage 3] gbrain MCP entry seeded (first launch)"
                                            );
                                        } else {
                                            tracing::debug!(
                                                "[Stage 3] gbrain MCP entry already present, skipping seed"
                                            );
                                        }
                                        // Store the server ID so diagnostics + restart commands can find it.
                                        *gbrain_mcp_id_slot.lock().unwrap() =
                                            Some("gbrain".to_string());
                                    }
                                    Err(e) => tracing::warn!(
                                        error = %e,
                                        "[Stage 3] gbrain MCP seed failed (continuing without bundled gbrain)"
                                    ),
                                }
                            } else {
                                tracing::info!(
                                    "[Stage 3] gbrain MCP seed skipped (bundle artifacts missing — run setup-bun-runtime.sh + setup-gbrain-source.sh)"
                                );
                                // Sprint 2.2.5b — mark the init slot so the
                                // diagnostics tab can tell users explicitly
                                // "the bundle is missing, run setup scripts"
                                // instead of just "NotAttempted" which sounds
                                // like a transient state.
                                *gbrain_init_status_slot.lock().unwrap() =
                                    uclaw_core::mcp::GbrainInitStatus::BundleMissing;
                            }

                            match guard.seed_builtin_playwright_mcp() {
                                Ok(true) => tracing::info!(
                                    "[Stage 3] Playwright MCP built-in entry seeded or refreshed"
                                ),
                                Ok(false) => tracing::debug!(
                                    "[Stage 3] Playwright MCP built-in entry already current"
                                ),
                                Err(e) => tracing::warn!(
                                    error = %e,
                                    "[Stage 3] Playwright MCP built-in seed failed"
                                ),
                            }
                            let expose_raw_playwright_mcp = settings_for_mcp
                                .read()
                                .await
                                .browser_runtime_provider_config
                                .playwright_mcp_raw_tools_exposed;
                            match guard
                                .set_playwright_mcp_raw_tools_exposed(expose_raw_playwright_mcp)
                            {
                                Ok(true) => tracing::info!(
                                    expose_raw_playwright_mcp,
                                    "[Stage 3] Playwright MCP raw tool exposure updated"
                                ),
                                Ok(false) => tracing::debug!(
                                    expose_raw_playwright_mcp,
                                    "[Stage 3] Playwright MCP raw tool exposure already current"
                                ),
                                Err(e) => tracing::warn!(
                                    error = %e,
                                    "[Stage 3] Playwright MCP raw tool exposure update failed"
                                ),
                            }
                            // Drop the write lock before the slow
                            // connect pass so readers aren't blocked.
                            drop(guard);
                            uclaw_core::mcp::connect_all_enabled(&shared).await;
                            let mut guard = shared.write().await;
                            let ids = guard.list_enabled_ids();
                            for id in &ids {
                                guard.start_health_loop(shared.clone(), id);
                            }
                            tracing::info!(
                                "[Stage 3] MCP servers auto-connect pass complete ({} enabled health/reconnect loops spawned)",
                                ids.len()
                            );
                        });
                    }

                    // M1-T7 — Prewarm HTTP/2 + TLS to the active LLM provider.
                    // Fire-and-forget: returns immediately so boot continues;
                    // the connection lands in the reqwest client pool ~200-500ms
                    // later, ready for the user's first chat message.
                    {
                        let llm_cfg = llm_config_arc.read().await.clone();
                        if !llm_cfg.provider.is_empty() {
                            tracing::info!(
                                provider = %llm_cfg.provider,
                                "[Stage 3] Spawning LLM prewarm"
                            );
                            uclaw_core::llm::prewarm::spawn_prewarm(
                                llm_cfg.provider.clone(),
                                llm_cfg.base_url.clone(),
                            );
                        }
                    }

                    // Stage 4: 启动所有已注册服务
                    tracing::info!("[Stage 4] Starting all registered services...");
                    let results = service_manager.start_all().await;
                    for (name, result) in &results {
                        match result {
                            Ok(()) => tracing::info!("[Stage 4] Service '{}' started OK", name),
                            Err(e) => tracing::error!("[Stage 4] Service '{}' failed to start: {}", name, e),
                        }
                    }
                    tracing::info!("[Stage 4] All services started ({} total)", results.len());

                    // Stage 5: 启动间隔复习调度器 & 每日摘要服务
                    tracing::info!("[Stage 5] Starting review scheduler & daily summary...");

                    let review_scheduler = uclaw_core::proactive::review_scheduler::ReviewScheduler::new(
                        app_handle.clone(),
                        db.clone(),
                    );
                    review_scheduler.start();
                    tracing::info!("[Stage 5] ReviewScheduler started");

                    let daily_summary = uclaw_core::proactive::daily_summary::DailySummaryService::new(
                        app_handle.clone(),
                        db.clone(),
                        9, // 默认每天 9 点生成昨日摘要
                    );
                    daily_summary.start();
                    tracing::info!("[Stage 5] DailySummaryService started");
                });
            }

            // ─── 全局快捷键注册 ──────────────────────────────────────────
            {
                let voice_combo = if cfg!(target_os = "macos") {
                    "Super+Shift+M"
                } else {
                    "Control+Shift+M"
                };
                let clip_combo = if cfg!(target_os = "macos") {
                    "Super+Shift+C"
                } else {
                    "Control+Shift+C"
                };

                // Register voice memory shortcut
                let voice_shortcut = Shortcut::new(
                    Some(if cfg!(target_os = "macos") { Modifiers::SUPER | Modifiers::SHIFT } else { Modifiers::CONTROL | Modifiers::SHIFT }),
                    Code::KeyM,
                );
                let voice_handle = app.handle().clone();
                app.global_shortcut().on_shortcut(voice_shortcut, move |_app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        dispatch_global_shortcut_action(&voice_handle, "quick-memory-voice");
                    }
                })?;
                tracing::info!("[GlobalShortcut] {} registered for voice memory", voice_combo);

                // Register clipboard capture shortcut
                let clip_shortcut = Shortcut::new(
                    Some(if cfg!(target_os = "macos") { Modifiers::SUPER | Modifiers::SHIFT } else { Modifiers::CONTROL | Modifiers::SHIFT }),
                    Code::KeyC,
                );
                let clip_handle = app.handle().clone();
                app.global_shortcut().on_shortcut(clip_shortcut, move |_app, _shortcut, event| {
                    if event.state != ShortcutState::Pressed { return; }
                    dispatch_global_shortcut_action(&clip_handle, "clipboard-capture-silent");
                })?;
                tracing::info!("[GlobalShortcut] {} registered for clipboard capture", clip_combo);

                // 将初始绑定存入 GlobalShortcutRegistry State
                let mut initial_bindings = HashMap::new();
                initial_bindings.insert("quick-memory-voice".to_string(), voice_combo.to_string());
                initial_bindings.insert("clipboard-capture-silent".to_string(), clip_combo.to_string());
                app.manage(GlobalShortcutRegistry {
                    bindings: Mutex::new(initial_bindings),
                });
                tracing::info!("[GlobalShortcut] Registry state initialized");
            }

            tracing::info!("uClaw started successfully");
            Ok(())
        })
        // ─── 优雅关闭钩子 ──────────────────────────────────────────────
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let app_handle = window.app_handle();
                if let Some(state) = app_handle.try_state::<AppState>() {
                    let service_manager = state.service_manager.clone();
                    let memu_client = state.memu_client.clone();
                    tracing::info!("[Shutdown] Window destroyed, stopping all services...");
                    // 同步阻塞停止所有服务（窗口关闭时在进程退出前执行）
                    let rt = tokio::runtime::Runtime::new().ok();
                    if let Some(rt) = rt {
                        rt.block_on(async {
                            let results = service_manager.stop_all().await;
                            for (name, result) in &results {
                                match result {
                                    Ok(()) => tracing::info!("[Shutdown] Service '{}' stopped", name),
                                    Err(e) => tracing::error!("[Shutdown] Service '{}' stop error: {}", name, e),
                                }
                            }
                            // 优雅关闭 memU client（停止 Python bridge 子进程）
                            if let Some(client) = &memu_client {
                                if let Err(e) = client.shutdown().await {
                                    tracing::warn!("[Shutdown] memU client shutdown error: {}", e);
                                }
                            }
                        });
                    }
                    tracing::info!("[Shutdown] All services stopped");
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Bootstrap
            uclaw_core::tauri_commands::get_settings,
            uclaw_core::commands::settings::get_http_api_enabled,
            uclaw_core::commands::settings::set_http_api_enabled,
            uclaw_core::tauri_commands::patch_settings,
            uclaw_core::browser::runtime_pack_ipc::get_browser_runtime_status,
            uclaw_core::browser::runtime_pack_ipc::get_browser_runtime_control_center,
            uclaw_core::browser::runtime_pack_ipc::set_browser_runtime_provider_enabled,
            uclaw_core::browser::runtime_pack_ipc::set_browser_runtime_provider_priority,
            uclaw_core::browser::runtime_pack_ipc::set_browser_runtime_mcp_raw_tools_exposed,
            uclaw_core::browser::runtime_pack_ipc::run_browser_runtime_provider_probe,
            uclaw_core::browser::runtime_pack_ipc::run_playwright_setup,
            uclaw_core::browser::runtime_pack_ipc::dry_run_browser_runtime_action,
            uclaw_core::browser::runtime_pack_ipc::execute_browser_runtime_action,
            uclaw_core::browser::identity_ipc::list_browser_identities,
            uclaw_core::browser::identity_ipc::complete_browser_identity_authorization_from_tab,
            uclaw_core::browser::identity_ipc::complete_browser_identity_authorization_from_webview,
            uclaw_core::browser::identity_ipc::revoke_browser_identity,
            uclaw_core::tauri_commands::get_platform,
            uclaw_core::tauri_commands::get_version,
            uclaw_core::tauri_commands::get_bootstrap_status,
            // Chat
            uclaw_core::tauri_commands::send_message,
            uclaw_core::commands::conversation::create_conversation,
            uclaw_core::commands::conversation::list_conversations,
            uclaw_core::commands::conversation::list_recent_threads,
            uclaw_core::commands::cost::get_daily_costs,
            uclaw_core::commands::cost::get_model_costs,
            uclaw_core::commands::cost::get_session_costs,
            uclaw_core::commands::cost::list_workspace_cost_rollup,
            uclaw_core::commands::cost::get_month_cost_total,
            uclaw_core::commands::conversation::get_messages,
            uclaw_core::commands::conversation::delete_conversation,
            uclaw_core::commands::conversation::toggle_star_conversation,
            // Spaces
            uclaw_core::commands::space::create_space,
            uclaw_core::commands::space::list_spaces,
            uclaw_core::commands::space::delete_space,
            // LLM Config
            uclaw_core::commands::llm_config::get_llm_config,
            uclaw_core::commands::llm_config::update_llm_config,
            // Artifacts
            uclaw_core::commands::artifact::list_artifacts,
            uclaw_core::commands::artifact::read_artifact,
            uclaw_core::commands::artifact::write_artifact,
            uclaw_core::commands::artifact::delete_artifact,
            // Enhanced Artifact Tree
            uclaw_core::commands::artifact::list_artifacts_tree,
            uclaw_core::commands::artifact::load_artifact_children,
            // Extended Artifact Commands
            uclaw_core::commands::artifact::create_artifact,
            uclaw_core::commands::artifact::rename_artifact,
            uclaw_core::commands::artifact::move_artifact,
            uclaw_core::commands::artifact::delete_artifact_recursive,
            uclaw_core::commands::artifact::detect_file_type,
            // Search
            uclaw_core::commands::search::search_workspace,
            uclaw_core::commands::search::search_conversations,
            uclaw_core::commands::search::search_all,
            // Notifications
            uclaw_core::commands::notification::get_notifications,
            uclaw_core::commands::notification::clear_notifications,
            // Background tasks
            uclaw_core::commands::background_task::get_background_tasks,
            // Memory
            uclaw_core::commands::memory::memory_set,
            uclaw_core::commands::memory::memory_get,
            uclaw_core::commands::memory::memory_delete,
            uclaw_core::commands::memory::memory_search,
            uclaw_core::commands::memory::memory_list,
            uclaw_core::commands::memory::memory_clear_namespace,
            uclaw_core::commands::memory::memory_prune_expired,
            uclaw_core::commands::memory::memory_bulk_import,
            uclaw_core::commands::memory::memory_export,
            uclaw_core::commands::memory::memory_list_namespaces,
            // Memory adapter unified IPC (PR4 of 阶段 4)
            uclaw_core::tauri_commands::memory_unified_record,
            uclaw_core::tauri_commands::memory_unified_recall,
            uclaw_core::tauri_commands::memory_unified_get,
            uclaw_core::tauri_commands::memory_unified_list,
            uclaw_core::tauri_commands::memory_unified_delete,
            uclaw_core::tauri_commands::memory_unified_clear_namespace,
            uclaw_core::tauri_commands::memory_unified_namespace_summaries,
            uclaw_core::tauri_commands::memory_unified_list_backends,
            uclaw_core::tauri_commands::memory_unified_set_default_backend,
            // MCP
            uclaw_core::commands::mcp::list_mcp_servers,
            uclaw_core::commands::mcp::add_mcp_server,
            uclaw_core::commands::mcp::update_mcp_server,
            uclaw_core::commands::mcp::remove_mcp_server,
            uclaw_core::commands::mcp::toggle_mcp_server,
            uclaw_core::commands::mcp::connect_mcp_server,
            uclaw_core::commands::mcp::disconnect_mcp_server,
            uclaw_core::commands::mcp::restart_mcp_server,
            uclaw_core::commands::mcp::list_mcp_tools,
            uclaw_core::commands::mcp::refresh_mcp_tools,
            uclaw_core::commands::mcp::ping_mcp_server,
            uclaw_core::commands::mcp::list_mcp_audit,
            // Skills
            uclaw_core::commands::skills::list_skills,
            uclaw_core::commands::skills::get_workspace_capabilities,
            uclaw_core::commands::skills::toggle_skill,
            uclaw_core::commands::skills::discover_skills,
            uclaw_core::commands::skills::reload_skills,
            uclaw_core::commands::skills::fork_skill_to_user,
            uclaw_core::commands::skills::list_active_manifest_skills,
            uclaw_core::commands::skills::get_skill_detail,
            uclaw_core::commands::skills::match_skills,
            // Channels
            uclaw_core::commands::channel::list_channels,
            uclaw_core::commands::channel::add_channel,
            uclaw_core::commands::channel::remove_channel,
            uclaw_core::commands::channel::toggle_channel,
            // IM Channel Instance CRUD
            uclaw_core::commands::channel::list_im_channels,
            uclaw_core::commands::channel::get_im_channel_statuses,
            uclaw_core::commands::channel::create_im_channel,
            uclaw_core::commands::channel::update_im_channel,
            uclaw_core::commands::channel::delete_im_channel,
            uclaw_core::commands::channel::toggle_im_channel,
            uclaw_core::commands::channel::request_wechat_ilink_qrcode,
            uclaw_core::commands::channel::poll_wechat_ilink_qrcode_status,
            uclaw_core::commands::channel::save_wechat_ilink_token,
            uclaw_core::commands::channel::disconnect_wechat_ilink,
            uclaw_core::commands::channel::list_spec_channel_bindings,
            uclaw_core::commands::channel::update_spec_channel_bindings,
            uclaw_core::commands::channel::update_spec_im_settings,
            // Providers
            uclaw_core::commands::provider::list_providers,
            uclaw_core::commands::provider::list_configured_providers,
            uclaw_core::commands::provider::get_provider_config,
            uclaw_core::commands::provider::configure_provider,
            uclaw_core::commands::provider::configure_provider_with_models,
            uclaw_core::commands::provider::remove_provider_config,
            uclaw_core::commands::provider::test_provider_connection,
            uclaw_core::commands::provider::list_provider_models,
            uclaw_core::commands::provider::get_configured_models,
            uclaw_core::commands::provider::get_all_configured_models,
            uclaw_core::commands::provider::get_active_model,
            uclaw_core::commands::provider::set_active_model,
            uclaw_core::commands::provider::get_role_models,
            uclaw_core::commands::provider::set_role_model,
            // Persona
            uclaw_core::commands::persona::create_persona_journal_entry,
            uclaw_core::commands::persona::delete_persona_journal_entry,
            uclaw_core::commands::persona::get_persona_config,
            uclaw_core::commands::persona::get_persona_relationship_timeline,
            uclaw_core::commands::persona::promote_persona_journal_entry,
            uclaw_core::commands::persona::propose_persona_keepsake,
            uclaw_core::commands::persona::record_persona_event,
            uclaw_core::commands::persona::update_persona_badge_visibility,
            uclaw_core::commands::persona::update_persona_bond_profile,
            uclaw_core::commands::persona::update_persona_keepsake_status,
            uclaw_core::commands::persona::update_persona_relationship_settings,
            uclaw_core::commands::persona::update_persona_voice_profile,
            // Safety
            uclaw_core::commands::safety::get_safety_policy,
            uclaw_core::commands::safety::set_safety_mode,
            uclaw_core::commands::safety::set_tool_safety_override,
            uclaw_core::commands::safety::remove_tool_safety_override,
            uclaw_core::commands::safety::add_auto_approved_tool,
            uclaw_core::commands::safety::remove_auto_approved_tool,
            uclaw_core::commands::safety::block_tool,
            uclaw_core::commands::safety::unblock_tool,
            uclaw_core::commands::safety::assess_command_risk,
            // System Prompts
            uclaw_core::commands::system_prompt::get_system_prompt_config,
            uclaw_core::commands::system_prompt::create_system_prompt,
            uclaw_core::commands::system_prompt::delete_system_prompt,
            uclaw_core::commands::system_prompt::update_system_prompt,
            uclaw_core::commands::system_prompt::set_default_prompt,
            uclaw_core::commands::system_prompt::get_system_prompt_versions,
            uclaw_core::commands::system_prompt::update_append_setting,
            // Tool Approval
            uclaw_core::commands::tool_approval::approve_tool_call,
            uclaw_core::tauri_commands::respond_ask_user,
            uclaw_core::tauri_commands::respond_exit_plan_mode,
            uclaw_core::tauri_commands::respond_plan_mode_suggest,
            uclaw_core::commands::tool_approval::list_permission_rules,
            uclaw_core::commands::tool_approval::create_permission_rule,
            uclaw_core::commands::tool_approval::delete_permission_rule,
            uclaw_core::commands::tool_approval::list_permission_audit,
            // Memory Graph
            uclaw_core::tauri_commands::memory_graph_search,
            uclaw_core::tauri_commands::memory_graph_get_node,
            uclaw_core::tauri_commands::memory_graph_list_boot,
            uclaw_core::tauri_commands::memory_graph_manage_boot,
            uclaw_core::tauri_commands::memory_graph_list_timeline,
            uclaw_core::tauri_commands::memory_graph_explain_recall,
            uclaw_core::tauri_commands::get_memory_recall_config,
            uclaw_core::tauri_commands::patch_memory_recall_config,
            uclaw_core::tauri_commands::memory_graph_get_full_graph,
            uclaw_core::tauri_commands::memory_graph_create_node,
            uclaw_core::tauri_commands::memory_graph_quick_capture,
            uclaw_core::tauri_commands::memory_graph_update_node,
            uclaw_core::tauri_commands::memory_graph_delete_node,
            // EntityPage (Memory OS Foundation Phase 1)
            uclaw_core::tauri_commands::memory_entity_page_create,
            uclaw_core::tauri_commands::memory_entity_page_get,
            uclaw_core::tauri_commands::memory_entity_page_find_by_slug,
            uclaw_core::tauri_commands::memory_entity_page_list,
            uclaw_core::tauri_commands::memory_entity_page_append_timeline,
            // Wiki artifacts (Memory OS Foundation Phase 3)
            uclaw_core::tauri_commands::memory_wiki_get_overview,
            uclaw_core::tauri_commands::memory_wiki_get_index,
            uclaw_core::tauri_commands::memory_wiki_regenerate,
            // Health findings (Memory OS Foundation Phase 4)
            uclaw_core::tauri_commands::memory_health_list_findings,
            uclaw_core::tauri_commands::memory_health_dismiss_finding,
            uclaw_core::tauri_commands::memory_health_run_now,
            // Lint scan (Memory OS Foundation Phase 5)
            uclaw_core::tauri_commands::memory_lint_run_now,
            // Drift Detection + Importance Decay (Memory OS L3)
            uclaw_core::tauri_commands::memory_drift_list_events,
            uclaw_core::tauri_commands::memory_drift_resolve_event,
            uclaw_core::tauri_commands::memory_importance_list_candidates,
            // EntityPage synthesis (Memory OS Foundation Phase 6.2/6.3)
            uclaw_core::tauri_commands::memory_entity_page_synthesize_now,
            // Markdown export (Memory OS Foundation Phase 7.1)
            uclaw_core::tauri_commands::memory_wiki_export,
            // Markdown sync-from-disk (Memory OS Foundation Phase 7.2)
            uclaw_core::tauri_commands::memory_wiki_sync_from_disk,
            // Learning pipeline (Memory OS Sprint 1.10 + Sprint 2.3)
            uclaw_core::tauri_commands::memory_learning_rebuild_now,
            uclaw_core::tauri_commands::memory_learning_list_facets,
            uclaw_core::tauri_commands::memory_learning_dismiss_facet,
            uclaw_core::tauri_commands::memory_learning_promote_facet,
            uclaw_core::tauri_commands::memory_learning_demote_facet,
            // Fragment / Daily Summary
            uclaw_core::tauri_commands::memory_graph_list_fragments,
            uclaw_core::tauri_commands::search_fragments,
            uclaw_core::tauri_commands::list_daily_summaries,
            // Learned Skills
            uclaw_core::tauri_commands::list_learned_skills,
            uclaw_core::tauri_commands::get_learned_skill,
            uclaw_core::tauri_commands::toggle_learned_skill,
            uclaw_core::tauri_commands::delete_learned_skill,
            uclaw_core::tauri_commands::update_learned_skill,
            uclaw_core::tauri_commands::record_skill_cited,
            uclaw_core::tauri_commands::set_skill_lifecycle,
            uclaw_core::tauri_commands::list_invocable_skills,
            uclaw_core::tauri_commands::get_skill_versions,
            // GEP Gene Evolution
            uclaw_core::tauri_commands::list_genes,
            uclaw_core::tauri_commands::get_gene_detail,
            uclaw_core::tauri_commands::get_gene_evolution_tree,
            uclaw_core::tauri_commands::retire_gene,
            uclaw_core::tauri_commands::reactivate_gene,
            // [R5] symphony 命令注册已删。
            uclaw_core::tauri_commands::backfill_skill_keywords,
            uclaw_core::tauri_commands::propose_skill_consolidation,
            uclaw_core::tauri_commands::cancel_skill_consolidation,
            uclaw_core::tauri_commands::apply_skill_consolidation,
            // System Diagnostics
            uclaw_core::tauri_commands::get_system_diagnostics,
            // [R5] eval 命令注册已删。
            uclaw_core::tauri_commands::restart_memu_bridge,
            uclaw_core::tauri_commands::get_embedding_config,
            uclaw_core::tauri_commands::set_embedding_config,
            uclaw_core::tauri_commands::test_embedding_endpoint,
            uclaw_core::tauri_commands::run_setup_script,
            uclaw_core::tauri_commands::restart_gbrain_mcp,
            uclaw_core::tauri_commands::gbrain_list_pages,
            uclaw_core::tauri_commands::gbrain_get_page,
            uclaw_core::tauri_commands::gbrain_search,
            uclaw_core::tauri_commands::gbrain_get_backlinks,
            uclaw_core::tauri_commands::gbrain_traverse_graph,
            uclaw_core::tauri_commands::gbrain_get_versions,
            uclaw_core::tauri_commands::gbrain_revert_version,
            uclaw_core::tauri_commands::gbrain_put_page,
            uclaw_core::tauri_commands::gbrain_get_stats,
            uclaw_core::tauri_commands::gbrain_find_orphans,
            uclaw_core::tauri_commands::gbrain_full_graph,
            uclaw_core::tauri_commands::gbrain_serve_smoke,
            // Knowledge Ingestion
            uclaw_core::tauri_commands::ingest_files,
            uclaw_core::tauri_commands::ingest_url,
            uclaw_core::tauri_commands::ingest_job_status,
            uclaw_core::tauri_commands::ingest_list_jobs,
            uclaw_core::tauri_commands::reset_ai_engine,
            uclaw_core::tauri_commands::restart_app,
            // MEMUBOT Services
            uclaw_core::tauri_commands::services_health,
            uclaw_core::tauri_commands::memorization_status,
            uclaw_core::tauri_commands::proactive_status,
            uclaw_core::tauri_commands::proactive_start,
            uclaw_core::tauri_commands::proactive_stop,
            uclaw_core::tauri_commands::metrics_summary,
            uclaw_core::tauri_commands::memubot_config_get,
            uclaw_core::tauri_commands::get_plan_mode_suggest_enabled,
            uclaw_core::tauri_commands::set_plan_mode_suggest_enabled,
            uclaw_core::tauri_commands::get_stream_idle_timeout_secs,
            uclaw_core::tauri_commands::set_stream_idle_timeout_secs,
            // Bundle 17-B — /compact fold-delta threshold
            uclaw_core::tauri_commands::get_fold_delta_threshold,
            uclaw_core::tauri_commands::set_fold_delta_threshold,
            uclaw_core::tauri_commands::get_skill_prune_min_unused_days,
            uclaw_core::tauri_commands::set_skill_prune_min_unused_days,
            uclaw_core::tauri_commands::get_skill_promote_min_returned_count,
            uclaw_core::tauri_commands::set_skill_promote_min_returned_count,
            // Dev / Testing
            uclaw_core::tauri_commands::trigger_proactive_scenario,
            // Agent Session Control
            uclaw_core::tauri_commands::stop_agent_session,
            uclaw_core::tauri_commands::create_agent_session,
            uclaw_core::tauri_commands::delete_agent_session,
            uclaw_core::tauri_commands::toggle_pin_agent_session,
            uclaw_core::tauri_commands::toggle_archive_agent_session,
            uclaw_core::tauri_commands::toggle_archive_conversation,
            uclaw_core::tauri_commands::list_agent_sessions,
            uclaw_core::tauri_commands::list_chat_sessions_for_spec,
            uclaw_core::tauri_commands::estimate_session_context,
            uclaw_core::tauri_commands::get_agent_session_messages,
            uclaw_core::tauri_commands::send_agent_message,
            uclaw_core::tauri_commands::move_agent_session_to_workspace,
            uclaw_core::tauri_commands::stop_agent,
            uclaw_core::tauri_commands::cancel_conversation,
            uclaw_core::tauri_commands::interrupt_current_agent_run,
            uclaw_core::tauri_commands::consume_pending_recovery,
            uclaw_core::tauri_commands::dismiss_pending_recovery,
            uclaw_core::tauri_commands::queue_agent_message,
            uclaw_core::tauri_commands::agent_steer,
            uclaw_core::tauri_commands::agent_follow_up,
            uclaw_core::tauri_commands::fork_agent_session,
            uclaw_core::tauri_commands::rewind_session,
            // Browser Commands (Phase 3)
            uclaw_core::tauri_commands::browser_get_state,
            uclaw_core::tauri_commands::browser_launch,
            uclaw_core::tauri_commands::browser_shutdown,
            uclaw_core::tauri_commands::browser_take_screenshot,
            uclaw_core::tauri_commands::browser_list_sessions,
            uclaw_core::tauri_commands::browser_destroy_session,
            uclaw_core::tauri_commands::browser_start_screencast,
            uclaw_core::tauri_commands::browser_capture_screenshot,
            uclaw_core::tauri_commands::browser_stop_screencast,
            uclaw_core::tauri_commands::browser_get_dom_state,
            uclaw_core::tauri_commands::browser_ui_navigate,
            uclaw_core::tauri_commands::browser_ui_go_back,
            uclaw_core::tauri_commands::browser_ui_go_forward,
            uclaw_core::tauri_commands::browser_ui_switch_tab,
            uclaw_core::tauri_commands::browser_ui_reload,
            uclaw_core::tauri_commands::browser_ui_close_tab,
            uclaw_core::tauri_commands::browser_ui_click,
            uclaw_core::tauri_commands::browser_ui_mouse_event,
            uclaw_core::tauri_commands::browser_ui_complete_login,
            uclaw_core::tauri_commands::browser_webview_complete_login,
            // System Tray / Badge Commands (Phase 3)
            uclaw_core::tauri_commands::update_badge_count,
            // Automation Commands (Phase 3)
            uclaw_core::tauri_commands::list_automations,
            uclaw_core::tauri_commands::trigger_automation_manual,
            uclaw_core::tauri_commands::stop_automation_runs,
            uclaw_core::tauri_commands::get_automation_activity,
            uclaw_core::tauri_commands::get_or_create_spec_home_thread,
            // Humane Automation Commands (Phase 1 spec § 7.3)
            uclaw_core::tauri_commands::install_humane_spec,
            uclaw_core::tauri_commands::import_humane_spec_file,
            uclaw_core::tauri_commands::get_automation_spec,
            uclaw_core::tauri_commands::update_user_config,
            uclaw_core::tauri_commands::set_automation_permission,
            uclaw_core::tauri_commands::set_automation_enabled,
            uclaw_core::tauri_commands::uninstall_automation,
            uclaw_core::tauri_commands::resolve_escalation,
            uclaw_core::tauri_commands::list_pending_escalations,
            uclaw_core::tauri_commands::read_automation_memory,
            uclaw_core::tauri_commands::compact_automation_memory,
            uclaw_core::tauri_commands::list_installed_marketplace_automations,
            uclaw_core::tauri_commands::list_marketplace_humans,
            uclaw_core::tauri_commands::install_marketplace_human,
            uclaw_core::tauri_commands::uninstall_marketplace_human,
            uclaw_core::tauri_commands::list_standalone_installs,
            uclaw_core::tauri_commands::query_marketplace,
            uclaw_core::tauri_commands::marketplace_category_counts,
            uclaw_core::tauri_commands::get_marketplace_detail,
            uclaw_core::tauri_commands::check_marketplace_updates,
            uclaw_core::tauri_commands::refresh_marketplace,
            // Files Rail Commands
            uclaw_core::files_rail::commands::files_rail_list_mounts,
            uclaw_core::files_rail::commands::files_rail_read_dir,
            uclaw_core::files_rail::commands::files_rail_watch_start,
            uclaw_core::files_rail::commands::files_rail_watch_stop,
            // Preview Commands
            uclaw_core::preview::commands::preview_read_bytes,
            uclaw_core::preview::commands::preview_resolve_chips,
            uclaw_core::preview::commands::preview_write_text,
            uclaw_core::preview::commands::approve_preview_write,
            // ─── Git Commands ───
            uclaw_core::tauri_commands_git::gh_available,
            uclaw_core::tauri_commands_git::gh_create_issue,
            uclaw_core::tauri_commands_git::gh_create_pr,
            uclaw_core::tauri_commands_git::git_branches,
            uclaw_core::tauri_commands_git::git_checkout_branch,
            uclaw_core::tauri_commands_git::git_commit,
            uclaw_core::tauri_commands_git::git_commit_push_pr,
            uclaw_core::tauri_commands_git::git_create_branch,
            uclaw_core::tauri_commands_git::git_current_branch,
            uclaw_core::tauri_commands_git::git_default_branch,
            uclaw_core::tauri_commands_git::git_diff,
            uclaw_core::tauri_commands_git::git_init_repo,
            uclaw_core::tauri_commands_git::git_is_repo,
            uclaw_core::tauri_commands_git::git_status,
            // Workspace Commands
            uclaw_core::tauri_commands::get_active_workspace_id,
            uclaw_core::tauri_commands::set_active_workspace_id,
            uclaw_core::tauri_commands::create_workspace,
            uclaw_core::tauri_commands::update_workspace,
            uclaw_core::tauri_commands::get_workspace_skill_tags,
            uclaw_core::tauri_commands::set_workspace_skill_tags,
            uclaw_core::tauri_commands::reorder_workspaces,
            uclaw_core::tauri_commands::get_workspace_directories,
            uclaw_core::tauri_commands::attach_workspace_directory,
            uclaw_core::tauri_commands::detach_workspace_directory,
            uclaw_core::tauri_commands::list_session_directories,
            uclaw_core::tauri_commands::attach_session_directory,
            uclaw_core::tauri_commands::detach_session_directory,
            uclaw_core::tauri_commands::rename_attached_file,
            uclaw_core::tauri_commands::move_attached_file,
            uclaw_core::tauri_commands::read_attached_file,
            uclaw_core::tauri_commands::delete_workspace,
            uclaw_core::tauri_commands::list_directory_entries,
            uclaw_core::tauri_commands::search_workspace_files_for_mention,
            uclaw_core::tauri_commands::upload_workspace_file,
            uclaw_core::tauri_commands::copy_file_into_workspace,
            uclaw_core::tauri_commands::list_always_allowed_paths,
            uclaw_core::tauri_commands::add_always_allowed_path,
            uclaw_core::tauri_commands::remove_always_allowed_path,
            uclaw_core::tauri_commands::list_session_allowed_paths,
            uclaw_core::tauri_commands::promote_session_path_to_global,
            uclaw_core::tauri_commands::path_is_directory,
            uclaw_core::tauri_commands::delete_workspace_file,
            uclaw_core::tauri_commands::read_workspace_uclaw_md,
            uclaw_core::tauri_commands::write_workspace_uclaw_md,
            uclaw_core::tauri_commands::read_default_prompts,
            uclaw_core::tauri_commands::open_workspace_uclaw_md_externally,
            uclaw_core::tauri_commands::reveal_path_in_file_manager,
            // Trajectory
            uclaw_core::tauri_commands::get_session_trajectory,
            uclaw_core::tauri_commands::search_trajectories,
            // Session Title
            uclaw_core::tauri_commands::generate_session_title,
            // Agent Teams
            uclaw_core::tauri_commands::start_agent_teams,
            uclaw_core::tauri_commands::get_team_channel,
            uclaw_core::tauri_commands::stop_agent_teams,
            // STT (SenseVoice ONNX, local)
            uclaw_core::stt::commands::stt_transcribe,
            uclaw_core::stt::commands::stt_model_status,
            uclaw_core::stt::commands::stt_download_model,
            uclaw_core::stt::commands::stt_get_settings,
            uclaw_core::stt::commands::stt_save_settings,
            uclaw_core::stt::commands::stt_list_microphones,
            // Connection health (Bottom Dock)
            uclaw_core::tauri_commands::get_app_health,
            uclaw_core::tauri_commands::get_memu_status,
            uclaw_core::tauri_commands::memu_embed_text,
            // Global Shortcut
            update_global_shortcut,
            // Slice 1 — Agent OS v2 introspection (M2-A baseline + M2-J telemetry)
            uclaw_core::tauri_commands::inspect_baseline_blocks,
            uclaw_core::tauri_commands::inspect_rendered_baseline,
            uclaw_core::tauri_commands::get_latest_token_budget,
            uclaw_core::tauri_commands::list_token_budget_task_ids,
            // C2-Dirac-B2 — ContextManager compose stats for the M2-J UI
            uclaw_core::tauri_commands::get_compose_stats,
            // Pi Sprint 2 — Bash streaming overflow log reader
            uclaw_core::tauri_commands::read_bash_log,
            // Slice 1b — automation approval chokepoint
            uclaw_core::tauri_commands::list_pending_automation_approvals,
            uclaw_core::tauri_commands::resolve_automation_approval,
        ]);

    // ─── Debug 菜单事件处理器（仅 debug 模式） ──────────────────────
    #[cfg(debug_assertions)]
    {
        builder = builder.on_menu_event(|app_handle, event| {
            use tauri::Emitter;
            tracing::info!("[Debug] Menu event received, id={}", event.id().as_ref());
            let state = app_handle.state::<AppState>();
            let data_dir = state.data_dir.clone();
            match event.id().as_ref() {
                "emit-memory-recall" => {
                    let _ = app_handle.emit("agent:memory-recall", serde_json::json!({
                        "totalCandidates": 8,
                        "skillsCount": 2,
                        "bootCount": 3,
                        "triggeredCount": 2,
                        "relevantCount": 2,
                        "expandedCount": 1,
                        "recentCount": 2,
                        "items": [
                            {"nodeId": "debug-1", "title": "Git 提交规范 (示例)", "kind": "procedure", "source": "boot"},
                            {"nodeId": "debug-2", "title": "React 组件拆分最佳实践 (示例)", "kind": "procedure", "source": "boot"},
                            {"nodeId": "debug-3", "title": "用户偏好：浅色主题 (示例)", "kind": "userProfile", "source": "trigger"},
                        ],
                        "conversationId": null,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }));
                    tracing::info!("[Debug] Emitted test memory-recall event");
                }
                "emit-proactive-learning" => {
                    let _ = app_handle.emit("agent:proactive-learning", serde_json::json!({
                        "scenario": "skill_extraction",
                        "items_extracted": 2,
                        "categories": ["procedure"],
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "summary": "[Debug] 测试技能提取 — 从最近对话中识别到 2 个可复用操作模式",
                        "sessionId": null,
                    }));
                    tracing::info!("[Debug] Emitted test proactive-learning event");
                }
                "emit-self-eval" => {
                    // 模拟 SelfEvalTool.execute() 的完整流程：
                    // 1. INSERT session_evals
                    // 2. 对每个 learning 分类并 publish SkillLearned（非 Noise 类）
                    // 3. Emit session:eval-complete 给前端
                    // 注意：publish_skill_learned 是 async，需要在 spawn 中执行
                    let session_id = format!("debug-self-eval-{}", uuid::Uuid::new_v4());
                    let score: f32 = 0.72;
                    let reasoning = "[Debug] 手动注入测试数据 — 模拟 Agent 完成任务后调用 self_eval";
                    let learnings: Vec<String> = vec![
                        "先检查文件是否存在再写入，避免覆盖用户已有数据".to_string(),
                        "工具调用失败时先重试一次，重试前检查网络连接".to_string(),
                        "大文件应分批读取，每批不超过10MB避免内存溢出".to_string(),
                        "路径拼接使用 PathBuf 而非字符串拼接，跨平台更安全".to_string(),
                        "错误信息应区分网络错误和权限错误，分别给出不同建议".to_string(),
                        "在完成多步任务后应主动总结已完成步骤供用户确认".to_string(),
                    ];
                    let learnings_json = serde_json::to_string(&learnings).unwrap_or_default();
                    let now_ms = chrono::Utc::now().timestamp_millis();

                    // 1. INSERT into session_evals（同步）
                    {
                        let db = state.db.lock().unwrap();
                        if let Err(e) = db.execute(
                            "INSERT INTO session_evals (id, session_id, score, reasoning, learnings, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                            rusqlite::params![uuid::Uuid::new_v4().to_string(), session_id, score, reasoning, learnings_json, now_ms],
                        ) {
                            tracing::error!("[Debug] self-eval DB insert failed: {}", e);
                        } else {
                            tracing::info!("[Debug] self-eval INSERT into session_evals OK (score={:.2}, {} learnings)", score, learnings.len());
                        }
                    }

                    // 2. Publish SkillLearned events via InfraService（异步：spawn）
                    let infra_clone = state.infra_service.clone();
                    let session_id_clone = session_id.clone();
                    let learnings_clone = learnings.clone();
                    tauri::async_runtime::spawn(async move {
                        for learning in &learnings_clone {
                            let card = uclaw_core::agent::tools::builtin::self_eval::classify_learning(
                                learning, score, &session_id_clone, None,
                            );
                            if card.card_type == uclaw_core::agent::gep::types::LearningCardType::Noise {
                                continue;
                            }
                            infra_clone.publish_skill_learned(
                                "self_eval",
                                learning,
                                serde_json::json!({
                                    "session_id": session_id_clone,
                                    "score": score,
                                    "source": "self_eval",
                                    "learning_card": {
                                        "card_type": card.card_type,
                                        "failure_signal": card.failure_signal,
                                        "tool_name": card.tool_name,
                                        "strategy_hint": {
                                            "condition": card.strategy_hint.condition,
                                            "action": card.strategy_hint.action,
                                            "reason": card.strategy_hint.reason,
                                        },
                                    },
                                }),
                            ).await;
                        }
                        tracing::info!("[Debug] Published {} SkillLearned events via InfraService", learnings_clone.len());
                    });

                    // 3. Emit session:eval-complete to frontend（同步）
                    let _ = app_handle.emit("session:eval-complete", serde_json::json!({
                        "sessionId": session_id,
                        "score": score,
                        "reasoning": reasoning,
                        "learnings": learnings,
                    }));
                    tracing::info!("[Debug] Emitted test self-eval data ({} learnings, score={:.2})", learnings.len(), score);
                }
                "generate-default-config" => {
                    let config_path = data_dir.join("memubot_config.json");
                    if config_path.exists() {
                        tracing::info!("[Debug] Config already exists at {:?}, skipping", config_path);
                    } else {
                        let config = uclaw_core::memubot_config::MemubotConfig::default();
                        match config.save(&data_dir) {
                            Ok(()) => tracing::info!("[Debug] Default config saved to {:?}", config_path),
                            Err(e) => tracing::error!("[Debug] Failed to save config: {:?}", e),
                        }
                    }
                }
                other => {
                    tracing::info!("[Debug] Unhandled menu event: {}", other);
                }
            }
        });
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running uClaw");
}

/// 根据 shortcut_id 分派全局快捷键操作
pub fn dispatch_global_shortcut_action(handle: &tauri::AppHandle, shortcut_id: &str) {
    match shortcut_id {
        "quick-memory-voice" => {
            // 显示主窗口 + 触发前端语音记忆录制
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            let _ = handle.emit("memory-voice-global-trigger", ());
            tracing::info!("[GlobalShortcut] memory-voice-global-trigger emitted");
        }
        "clipboard-capture-silent" => {
            let h = handle.clone();
            std::thread::spawn(move || {
                clipboard_capture_action(&h);
            });
        }
        _ => {
            tracing::warn!("[GlobalShortcut] Unknown shortcut_id: {}", shortcut_id);
        }
    }
}

/// 将前端快捷键格式转换为 Tauri 全局快捷键格式
/// 前端: "Cmd+Shift+C" -> Tauri: "Super+Shift+C" (macOS)
/// 前端: "Ctrl+Shift+C" -> Tauri: "Control+Shift+C" (Windows/Linux)
fn convert_frontend_to_tauri_shortcut(combo: &str) -> String {
    combo
        .replace("Cmd", "Super")
        .replace("Ctrl", "Control")
}

/// IPC 命令：动态更新全局快捷键绑定
#[tauri::command]
fn update_global_shortcut(
    app: tauri::AppHandle,
    registry: tauri::State<'_, GlobalShortcutRegistry>,
    shortcut_id: String,
    new_combo: String,
) -> Result<(), String> {
    let mut bindings = registry.bindings.lock().map_err(|e| e.to_string())?;

    // 1. 获取旧绑定并解除
    if let Some(old_combo) = bindings.get(&shortcut_id).cloned() {
        if let Err(e) = app.global_shortcut().unregister(old_combo.as_str()) {
            tracing::warn!("[GlobalShortcut] Failed to unregister '{}': {}", old_combo, e);
            // 继续执行，不要因为旧快捷键解除失败而中断
        }
    }

    // 2. 如果 new_combo 为空，只移除不重新注册
    if new_combo.is_empty() {
        bindings.remove(&shortcut_id);
        tracing::info!("[GlobalShortcut] Removed binding for '{}'", shortcut_id);
        return Ok(());
    }

    // 3. 转换前端格式到 Tauri 格式
    let tauri_combo = convert_frontend_to_tauri_shortcut(&new_combo);

    // 4. 注册新的全局快捷键
    let shortcut_id_clone = shortcut_id.clone();
    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(tauri_combo.as_str(), move |_app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            dispatch_global_shortcut_action(&handle, &shortcut_id_clone);
        })
        .map_err(|e| format!("Failed to register shortcut '{}': {}", tauri_combo, e))?;

    // 5. 更新 State
    tracing::info!(
        "[GlobalShortcut] Updated '{}': -> '{}'",
        shortcut_id,
        tauri_combo
    );
    bindings.insert(shortcut_id, tauri_combo);

    Ok(())
}

/// 执行剪贴板捕获动作：读取剪贴板 -> 保存到记忆 -> 音效 + 通知
fn clipboard_capture_action(handle: &tauri::AppHandle) {
    // 1. 读取剪贴板
    let text = match arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
        Ok(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => {
            play_system_sound(false);
            let _ = handle.notification()
                .builder()
                .title("记忆拾取")
                .body("剪贴板为空，无内容可保存")
                .show();
            return;
        }
    };

    // 2. 保存到记忆图谱
    let save_result = save_clipboard_to_memory(handle, &text);

    // 3. 音效 + OS 通知
    match save_result {
        Ok(_) => {
            play_system_sound(true);
            let preview: String = text.chars().take(30).collect();
            let body = if text.chars().count() > 30 {
                format!("{}...", preview)
            } else {
                preview
            };
            let _ = handle.notification()
                .builder()
                .title("记忆已保存")
                .body(&body)
                .show();
        }
        Err(e) => {
            play_system_sound(false);
            let _ = handle.notification()
                .builder()
                .title("记忆保存失败")
                .body(&e)
                .show();
        }
    }
}

/// 从 app state 获取 MemoryGraphStore 并调用 quick_capture_core
fn save_clipboard_to_memory(handle: &tauri::AppHandle, text: &str) -> Result<(), String> {
    let state = handle.state::<AppState>();
    let store = &state.memory_graph_store;
    uclaw_core::tauri_commands::quick_capture_core(store, text, "clipboard", None, None)?;
    Ok(())
}

fn play_system_sound(success: bool) {
    #[cfg(target_os = "macos")]
    {
        let sound = if success {
            "/System/Library/Sounds/Glass.aiff"
        } else {
            "/System/Library/Sounds/Basso.aiff"
        };
        let _ = std::process::Command::new("afplay").arg(sound).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let sound = if success { "Asterisk" } else { "Hand" };
        let _ = std::process::Command::new("powershell")
            .args(["-c", &format!("[System.Media.SystemSounds]::{}::Play()", sound)])
            .spawn();
    }
}
