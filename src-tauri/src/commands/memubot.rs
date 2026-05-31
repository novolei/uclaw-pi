//! MEMUBOT-service-control-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), this domain has **no service**:
//! every command delegates to an async manager held in
//! [`crate::app::AppState`] — the [`crate::services::ServiceManager`]
//! (`state.service_manager`, health/start/stop of the background `memorization`
//! / `proactive` services), the metrics service (`state.metrics_service`), and
//! the hot-reloadable [`crate::memubot_config::MemubotConfig`]
//! (`state.memubot_config`, an `RwLock` persisted to `memubot_config.json` via
//! `MemubotConfig::save`). Those managers *are* the logic holders — there is
//! **no inline `state.db` SQL to lift** — so the JUDGMENT RULE resolves to a
//! thin move for the whole section.
//!
//! Relocated verbatim from the legacy `tauri_commands.rs` god file: the 17
//! `#[tauri::command]`s that sat under the `// ===== MEMUBOT 服务控制命令 =====`
//! banner. The trailing `memory_graph_delete_node` command (Memory Graph
//! domain) that shared the banner was left behind — it belongs to a deferred
//! cluster.

use tauri::State;

use crate::app::AppState;

/// 获取所有服务的健康状态
#[tauri::command]
pub async fn services_health(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let summary = state.service_manager.get_all_health().await;
    serde_json::to_value(summary).map_err(|e| e.to_string())
}

/// 获取记忆提取服务状态
#[tauri::command]
pub async fn memorization_status(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let health = state.service_manager.get_health("memorization").await;
    match health {
        Some(h) => serde_json::to_value(h).map_err(|e| e.to_string()),
        None => Ok(serde_json::json!({"status": "not_registered"})),
    }
}

/// 获取主动服务状态
#[tauri::command]
pub async fn proactive_status(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let health = state.service_manager.get_health("proactive").await;
    match health {
        Some(h) => serde_json::to_value(h).map_err(|e| e.to_string()),
        None => Ok(serde_json::json!({"status": "not_registered", "enabled": false})),
    }
}

/// 启动主动服务
#[tauri::command]
pub async fn proactive_start(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.service_manager.restart_service("proactive").await
        .map_err(|e| e.to_string())
}

/// 停止主动服务
#[tauri::command]
pub async fn proactive_stop(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.service_manager.stop_service("proactive").await
        .map_err(|e| e.to_string())
}

/// 获取可观测性指标
#[tauri::command]
pub async fn metrics_summary(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let summary = state.metrics_service.get_summary().await;
    serde_json::to_value(summary).map_err(|e| e.to_string())
}

/// 获取 MEMUBOT 配置
#[tauri::command]
pub async fn memubot_config_get(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let cfg = state.memubot_config.read().await;
    serde_json::to_value(&*cfg).map_err(|e| e.to_string())
}

/// 读取 Plan 模式自动建议开关
#[tauri::command]
pub async fn get_plan_mode_suggest_enabled(
    state: State<'_, AppState>,
) -> Result<bool, String> {
    Ok(state.memubot_config.read().await.plan_mode_suggest_enabled)
}

/// 设置 Plan 模式自动建议开关，并持久化到 memubot_config.json
#[tauri::command]
pub async fn set_plan_mode_suggest_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    {
        let mut cfg = state.memubot_config.write().await;
        cfg.plan_mode_suggest_enabled = enabled;
        cfg.save(&state.data_dir).map_err(|e| e.to_string())?;
    }
    tracing::info!(enabled, "plan_mode_suggest_enabled updated and persisted");
    Ok(())
}

/// Bundle 27-B — 读取 LLM 流式响应空闲超时（秒）
#[tauri::command]
pub async fn get_stream_idle_timeout_secs(
    state: State<'_, AppState>,
) -> Result<u64, String> {
    Ok(state.memubot_config.read().await.stream_idle_timeout_secs)
}

/// Bundle 17-B — read `/compact` delta-path threshold.
#[tauri::command]
pub async fn get_fold_delta_threshold(state: State<'_, AppState>) -> Result<u32, String> {
    Ok(state.memubot_config.read().await.context.fold_delta_threshold)
}

/// Bundle 17-B — set `/compact` delta-path threshold and persist.
///
/// Clamped to `[FOLD_DELTA_THRESHOLD_MIN, FOLD_DELTA_THRESHOLD_MAX]` =
/// `[1, 50]` per `memubot_config.rs`. Below 1 would disable the delta
/// path entirely (every compact re-renders); above 50 would let
/// nearly-fresh folds slip through as deltas and defeat the cache
/// stability benefit.
///
/// The dispatcher reads `cfg.context.fold_delta_threshold` afresh on
/// each `/compact`, so changes take effect on the next user-triggered
/// compaction without restart.
#[tauri::command]
pub async fn set_fold_delta_threshold(
    state: State<'_, AppState>,
    threshold: u32,
) -> Result<(), String> {
    let clamped = threshold.clamp(
        crate::memubot_config::FOLD_DELTA_THRESHOLD_MIN,
        crate::memubot_config::FOLD_DELTA_THRESHOLD_MAX,
    );
    {
        let mut cfg = state.memubot_config.write().await;
        cfg.context.fold_delta_threshold = clamped;
        cfg.save(&state.data_dir).map_err(|e| e.to_string())?;
    }
    tracing::info!(
        requested = threshold,
        applied = clamped,
        "[Bundle 17-B] fold_delta_threshold updated and persisted"
    );
    Ok(())
}

/// Bundle 27-B — 设置 LLM 流式响应空闲超时（秒），持久化到
/// memubot_config.json。变更对**下一次**消息立即生效（dispatcher /
/// headless 的 call_llm 在每次调用前重新从 MemubotConfig 读取该值）。
/// 不需要重启进程或主动服务。
#[tauri::command]
pub async fn set_stream_idle_timeout_secs(
    state: State<'_, AppState>,
    secs: u64,
) -> Result<(), String> {
    // Floor at 5s — anything lower would false-positive on legitimate
    // slow streams (reasoning models routinely have 5-30s inter-chunk
    // gaps mid-response). Cap at 600s — beyond that the user is
    // effectively asking us never to time out.
    let clamped = secs.clamp(5, 600);
    {
        let mut cfg = state.memubot_config.write().await;
        cfg.stream_idle_timeout_secs = clamped;
        cfg.save(&state.data_dir).map_err(|e| e.to_string())?;
    }
    tracing::info!(
        requested = secs,
        applied = clamped,
        "stream_idle_timeout_secs updated and persisted"
    );
    Ok(())
}

/// Bundle 26-B — 读取技能归档闲置天数阈值
#[tauri::command]
pub async fn get_skill_prune_min_unused_days(
    state: State<'_, AppState>,
) -> Result<u32, String> {
    Ok(state.memubot_config.read().await.memory_os.skill_prune_min_unused_days)
}

/// Bundle 26-B — 设置技能归档闲置天数阈值。`MemoryOsRuntimeConfig`
/// 是 proactive 服务启动时的 snapshot，因此持久化后做一次静默
/// `restart_service("proactive")` 让新值在下一个 tick 生效，匹配
/// `set_embedding_config` 同样的「保存后由后端重启子服务」模式。
#[tauri::command]
pub async fn set_skill_prune_min_unused_days(
    state: State<'_, AppState>,
    days: u32,
) -> Result<(), String> {
    // Floor at 1 day (anything lower archives skills before the user
    // has a chance to use them). Cap at 365 days (a year of cold
    // storage is plenty for any sane library).
    let clamped = days.clamp(1, 365);
    {
        let mut cfg = state.memubot_config.write().await;
        cfg.memory_os.skill_prune_min_unused_days = clamped;
        cfg.save(&state.data_dir).map_err(|e| e.to_string())?;
    }
    // Silent restart so the new threshold lands in the proactive
    // tick's `MemoryOsRuntimeConfig` snapshot. Best-effort: a restart
    // failure logs but doesn't fail the IPC — config is already
    // persisted and will be picked up at next app launch regardless.
    if let Err(e) = state.service_manager.restart_service("proactive").await {
        tracing::warn!(
            error = %e,
            "proactive restart after skill_prune threshold change failed — value persisted, will apply at next launch"
        );
    }
    tracing::info!(
        requested = days,
        applied = clamped,
        "skill_prune_min_unused_days updated, proactive restarted"
    );
    Ok(())
}

/// Bundle 26-D — 读取技能升级为基因的最小返回次数阈值
#[tauri::command]
pub async fn get_skill_promote_min_returned_count(
    state: State<'_, AppState>,
) -> Result<u32, String> {
    Ok(state.memubot_config.read().await.memory_os.skill_promote_min_returned_count)
}

/// Bundle 26-D — 设置技能升级为基因的最小返回次数阈值。同
/// `set_skill_prune_min_unused_days` 一样在保存后静默重启
/// proactive 服务。
#[tauri::command]
pub async fn set_skill_promote_min_returned_count(
    state: State<'_, AppState>,
    count: u32,
) -> Result<(), String> {
    // Floor at 1 (anything lower is the extraction-event itself —
    // promoting a never-actually-used skill is the noise we're
    // trying to avoid). Cap at 100 (beyond that effectively
    // disables promotion).
    let clamped = count.clamp(1, 100);
    {
        let mut cfg = state.memubot_config.write().await;
        cfg.memory_os.skill_promote_min_returned_count = clamped;
        cfg.save(&state.data_dir).map_err(|e| e.to_string())?;
    }
    if let Err(e) = state.service_manager.restart_service("proactive").await {
        tracing::warn!(
            error = %e,
            "proactive restart after skill_promote threshold change failed — value persisted, will apply at next launch"
        );
    }
    tracing::info!(
        requested = count,
        applied = clamped,
        "skill_promote_min_returned_count updated, proactive restarted"
    );
    Ok(())
}
