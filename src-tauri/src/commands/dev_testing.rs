//! Dev / testing-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), this domain has **no service**:
//! the single command drives the proactive end-to-end path by delegating to the
//! memU client held in [`crate::app::AppState`] (`state.memu_client`) and emitting
//! an `agent:proactive-learning` event — there is **no inline `state.db` SQL to
//! lift**, so the JUDGMENT RULE resolves to a thin move.
//!
//! Relocated verbatim from the legacy `tauri_commands.rs` god file (the
//! `// ─── Dev / Testing Commands` section): the 1 `#[tauri::command]`. The
//! `stop_agent_session` command that follows in the god file belongs to the
//! deferred Agent Session domain and was left behind.

use tauri::{Emitter, State};

use crate::app::AppState;

/// 手动触发指定的 Proactive 场景（跳过定时器和阈值条件）
///
/// 用于端到端验证完整链路：场景 → memorize → IPC 事件。
/// 生产环境也可调用，日志会标注为手动触发。
#[tauri::command]
pub async fn trigger_proactive_scenario(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    scenario_name: String,
) -> Result<serde_json::Value, String> {
    let valid_scenarios = ["conversation_learning", "skill_extraction", "multimodal_context"];
    if !valid_scenarios.contains(&scenario_name.as_str()) {
        return Err(format!(
            "Unknown scenario: {}. Valid: {:?}",
            scenario_name, valid_scenarios
        ));
    }

    tracing::info!(
        "[DevTrigger] Manually triggering proactive scenario: {}",
        scenario_name
    );

    // 尝试通过 memU client 执行真实的 memorize
    let mut items_extracted: usize = 0;
    let mut categories: Vec<String> = vec![];

    if let Some(ref memu) = state.memu_client {
        let (memory_types, source_type): (Vec<&str>, &str) = match scenario_name.as_str() {
            "conversation_learning" => (
                vec!["profile", "behavior"],
                "proactive_test_conversation",
            ),
            "skill_extraction" => (
                vec!["skill", "tool"],
                "proactive_test_skill",
            ),
            _ => (
                vec!["knowledge"],
                "proactive_test_multimodal",
            ),
        };

        let test_content = format!(
            "[Dev Test] Triggered {} scenario manually at {}",
            scenario_name,
            chrono::Utc::now().to_rfc3339()
        );

        match memu
            .memorize_with_config(&test_content, &memory_types, None, source_type)
            .await
        {
            Ok(result) => {
                items_extracted = result.items_extracted;
                categories = result.categories_updated;
                tracing::info!(
                    "[DevTrigger] memorize_with_config OK: items={}, categories={:?}",
                    items_extracted,
                    categories
                );
            }
            Err(e) => {
                tracing::warn!("[DevTrigger] memorize_with_config failed: {}", e);
            }
        }
    } else {
        tracing::warn!("[DevTrigger] memu_client is None, skipping memorize");
    }

    // Emit IPC 事件到前端
    let summary = format!("[Dev Test] {} 场景手动触发成功", scenario_name);
    let _ = app_handle.emit(
        "agent:proactive-learning",
        serde_json::json!({
            "scenario": scenario_name,
            "items_extracted": items_extracted,
            "categories": categories,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "summary": summary,
            "dev_trigger": true,
        }),
    );

    Ok(serde_json::json!({
        "success": true,
        "scenario": scenario_name,
        "items_extracted": items_extracted,
        "categories": categories,
        "dev_trigger": true,
    }))
}
