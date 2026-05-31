//! Learned-Skills-domain Tauri commands — thin wrappers.
//!
//! Per the code-organization ADR (2026-05-31), this domain has **no service**:
//! every command delegates to the in-memory
//! [`crate::memory_graph::store::MemoryGraphStore`] held in
//! `state.memory_graph_store` (an `Arc<MemoryGraphStore>`). That store *is* the
//! logic holder — it owns its own SQLite access behind `list_nodes_by_kind` /
//! `get_node` / `update_node` / `get_active_version` / `create_version` /
//! `get_keywords_for_node` / `create_keyword` / `delete_node` / … — so there is
//! **no `&Connection` / `state.db` SQL to lift**, and the JUDGMENT RULE resolves
//! to a thin move (same shape as `commands::memory`, which wraps
//! `crate::memory::MemoryStore`).
//!
//! The two consolidation commands (`propose_skill_consolidation` /
//! `apply_skill_consolidation`) additionally orchestrate an LLM call via
//! `state.provider_service`, emit `skill-consolidation:progress` events through
//! the `AppHandle`, and coordinate an in-flight cancel via a process-level
//! [`CONSOLIDATION_CANCELLED`] atomic flag (flipped by
//! `cancel_skill_consolidation`). That orchestration is manager/async-bound,
//! not `&Connection`-extractable, so it stays here in full — along with every
//! consolidation-only helper/type/static, all of which were private to this
//! domain and have no other caller in the god file:
//!   - [`CONSOLIDATION_SYSTEM_PROMPT`], [`CONSOLIDATION_CANCELLED`]
//!   - [`emit_consolidation_progress`], [`consolidation_max_tokens`]
//!   - [`precluster_by_title`], [`call_consolidation_llm`] (+
//!     [`ConsolidationLlmOutput`]), [`parse_json_array_tolerant`]
//!
//! The editable-field input shape [`UpdateLearnedSkillInput`] and the version
//! read shape [`SkillVersionInfo`] are likewise Learned-Skills-only and move
//! here with their commands.
//!
//! Relocated verbatim from the legacy `tauri_commands.rs` god file.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Emitter, State};

use crate::agent::types::ChatMessage;
use crate::app::AppState;

// ─── Learned Skills Commands ─────────────────────────────────────────────────

/// 列出所有学到的技能（Procedure 节点 + metadata.skill_type == "learned"）
#[tauri::command]
pub async fn list_learned_skills(
    state: State<'_, AppState>,
    space_id: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    use crate::memory_graph::models::MemoryNodeKind;

    let store = &state.memory_graph_store;
    let sid = space_id.unwrap_or_else(|| "default".into());

    let nodes = store.list_nodes_by_kind(&sid, MemoryNodeKind::Procedure, 500)
        .map_err(|e| format!("Failed to list procedure nodes: {}", e))?;

    let mut results = Vec::new();
    for node in nodes {
        if let Some(ref meta) = node.metadata {
            if meta.get("skill_type").and_then(|v| v.as_str()) == Some("learned") {
                results.push(serde_json::json!({
                    "id": node.id,
                    "name": node.title,
                    "context": meta.get("context").cloned().unwrap_or(serde_json::Value::Null),
                    "principles": meta.get("principles").cloned().unwrap_or(serde_json::Value::Null),
                    "steps": meta.get("steps").cloned().unwrap_or(serde_json::Value::Null),
                    "pitfalls": meta.get("pitfalls").cloned().unwrap_or(serde_json::Value::Null),
                    "enabled": meta.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                    "usageCount": meta.get("usage_count").and_then(|v| v.as_u64()).unwrap_or(0),
                    "citedCount": meta.get("cited_count").and_then(|v| v.as_u64()).unwrap_or(0),
                    "lastCitedAt": meta.get("last_cited_at").cloned().unwrap_or(serde_json::Value::Null),
                    "lifecycle": meta.get("lifecycle").and_then(|v| v.as_str()).unwrap_or("promoted"),
                    "category": meta.get("category").cloned().unwrap_or(serde_json::Value::Null),
                    "tags": meta.get("tags").cloned().unwrap_or(serde_json::Value::Null),
                    "validationHint": meta.get("validation_hint").cloned().unwrap_or(serde_json::Value::Null),
                    "createdAt": node.created_at,
                }));
            }
        }
    }
    Ok(results)
}

/// 获取单个学到的技能详情（含 version content）
#[tauri::command]
pub async fn get_learned_skill(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<serde_json::Value, String> {
    let store = &state.memory_graph_store;

    let node = store.get_node(&skill_id)
        .map_err(|e| format!("Failed to get node: {}", e))?
        .ok_or_else(|| format!("Skill not found: {}", skill_id))?;

    let meta = node.metadata.as_ref().cloned().unwrap_or(serde_json::json!({}));
    let active_version = store.get_active_version(&skill_id)
        .map_err(|e| format!("Failed to get active version: {}", e))?;

    let content = active_version.map(|v| v.content).unwrap_or_default();

    Ok(serde_json::json!({
        "id": node.id,
        "name": node.title,
        "context": meta.get("context").cloned().unwrap_or(serde_json::Value::Null),
        "principles": meta.get("principles").cloned().unwrap_or(serde_json::Value::Null),
        "steps": meta.get("steps").cloned().unwrap_or(serde_json::Value::Null),
        "pitfalls": meta.get("pitfalls").cloned().unwrap_or(serde_json::Value::Null),
        "enabled": meta.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        "usageCount": meta.get("usage_count").and_then(|v| v.as_u64()).unwrap_or(0),
        "citedCount": meta.get("cited_count").and_then(|v| v.as_u64()).unwrap_or(0),
        "lifecycle": meta.get("lifecycle").and_then(|v| v.as_str()).unwrap_or("promoted"),
        "createdAt": node.created_at,
        "content": content,
    }))
}

/// 切换学到的技能的启用/禁用状态
#[tauri::command]
pub async fn toggle_learned_skill(
    state: State<'_, AppState>,
    skill_id: String,
    enabled: bool,
) -> Result<(), String> {
    let store = &state.memory_graph_store;

    let node = store.get_node(&skill_id)
        .map_err(|e| format!("Failed to get node: {}", e))?
        .ok_or_else(|| format!("Skill not found: {}", skill_id))?;

    let mut meta = node.metadata.unwrap_or(serde_json::json!({}));
    if let Some(obj) = meta.as_object_mut() {
        obj.insert("enabled".to_string(), serde_json::Value::Bool(enabled));
    }

    store.update_node(&skill_id, None, None, Some(&meta))
        .map_err(|e| format!("Failed to update node: {}", e))?;
    Ok(())
}

/// 删除学到的技能
#[tauri::command]
pub async fn delete_learned_skill(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<(), String> {
    let store = &state.memory_graph_store;
    store.delete_node(&skill_id)
        .map_err(|e| format!("Failed to delete skill: {}", e))?;
    Ok(())
}

/// 记录一个技能被 LLM 引用 (E2 → E3 桥接)
///
/// 由前端在解析到 `> 应用技能：X — Y` citation 块后调用一次。
/// 在 metadata 里 bump 一个独立的 `cited_count` 字段（与
/// `usage_count` 分开 — 后者只代表"进入 system prompt"，前者代表
/// "LLM 真的应用了"）。E3 之后会让 boot 排序优先看 cited_count。
///
/// 返回匹配到的 skill_id（或 null 如果 LLM cite 了一个不存在的标题）。
/// 软失败：写入错误只 log，不抛给前端 — UI 不应因为这点小事报错。
#[tauri::command]
pub async fn record_skill_cited(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    space_id: Option<String>,
    title: String,
) -> Result<Option<String>, String> {
    let store = &state.memory_graph_store;
    let sid = space_id.unwrap_or_else(|| "default".into());

    // Normalize the same way skill_parser does so capitalization /
    // trailing punctuation differences don't break the lookup.
    let normalized = crate::skills::normalize_skill_title(&title);
    if normalized.is_empty() {
        return Ok(None);
    }

    let node = store
        .find_learned_skill_by_normalized_title(&sid, &normalized)
        .map_err(|e| format!("lookup failed: {}", e))?;

    let Some(node) = node else {
        tracing::info!(
            cited_title = %title,
            normalized,
            "record_skill_cited: LLM cited a title that doesn't exist in the skill DB"
        );
        return Ok(None);
    };

    // Bump cited_count via json_set (mirrors bump_skill_usage shape).
    let mut meta = node.metadata.clone().unwrap_or(serde_json::json!({}));
    let new_cited_count: u64;
    if let Some(obj) = meta.as_object_mut() {
        let prev = obj
            .get("cited_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        new_cited_count = prev + 1;
        obj.insert(
            "cited_count".to_string(),
            serde_json::Value::Number(serde_json::Number::from(new_cited_count)),
        );
        obj.insert(
            "last_cited_at".to_string(),
            serde_json::Value::String(chrono::Utc::now().to_rfc3339()),
        );
    } else {
        new_cited_count = 1;
    }

    // Check for auto-promotion: draft skills with cited_count >= 3 get promoted.
    let lifecycle = meta
        .as_object()
        .and_then(|obj| obj.get("lifecycle"))
        .and_then(|v| v.as_str())
        .unwrap_or("promoted");

    let should_promote = lifecycle == "draft" && new_cited_count >= 3;
    if should_promote {
        if let Some(obj) = meta.as_object_mut() {
            obj.insert(
                "lifecycle".to_string(),
                serde_json::Value::String("promoted".to_string()),
            );
        }
    }

    if let Err(e) = store.update_node(&node.id, None, None, Some(&meta)) {
        tracing::warn!(
            node_id = %node.id,
            err = %e,
            "record_skill_cited: bump cited_count failed (non-fatal)"
        );
    } else {
        tracing::info!(
            node_id = %node.id,
            title = %node.title,
            cited_count = new_cited_count,
            "record_skill_cited: bumped cited_count"
        );

        // Emit lifecycle-changed event if auto-promoted
        if should_promote {
            tracing::info!(
                node_id = %node.id,
                title = %node.title,
                "record_skill_cited: auto-promoted draft skill (cited_count >= 3)"
            );
            let _ = app_handle.emit("skill:lifecycle-changed", serde_json::json!({
                "nodeId": node.id,
                "oldLifecycle": "draft",
                "newLifecycle": "promoted",
                "reason": "auto_promotion_3_citations"
            }));
        }
    }

    Ok(Some(node.id))
}

/// Manually set a learned skill's lifecycle stage.
///
/// PR-mattpocock-3 introduces three stages — "draft" (just extracted, not
/// yet validated by usage), "promoted" (cited ≥ 3 times OR manually
/// promoted), "deprecated" (manually retired). The manifest only includes
/// "promoted" skills; skill_search includes all stages but flags non-promoted
/// ones in the result's `warnings[]`.
///
/// Used by Settings → 已学技能 → ⋯ overflow menu.
#[tauri::command]
pub async fn set_skill_lifecycle(
    state: State<'_, AppState>,
    node_id: String,
    lifecycle: String,
) -> Result<(), String> {
    if !matches!(lifecycle.as_str(), "draft" | "promoted" | "deprecated") {
        return Err(format!(
            "invalid lifecycle '{}' — expected one of: draft, promoted, deprecated",
            lifecycle
        ));
    }
    let store = &state.memory_graph_store;
    let node = store
        .get_node(&node_id)
        .map_err(|e| format!("lookup failed: {}", e))?
        .ok_or_else(|| format!("skill node '{}' not found", node_id))?;

    let mut meta = node.metadata.clone().unwrap_or(serde_json::json!({}));
    if let Some(obj) = meta.as_object_mut() {
        obj.insert(
            "lifecycle".to_string(),
            serde_json::Value::String(lifecycle.clone()),
        );
    }
    store
        .update_node(&node_id, None, None, Some(&meta))
        .map_err(|e| format!("update failed: {}", e))?;

    tracing::info!(
        node_id = %node_id,
        title = %node.title,
        new_lifecycle = %lifecycle,
        "set_skill_lifecycle: changed"
    );
    Ok(())
}

/// Update editable fields of a learned skill.
///
/// Accepts a `node_id` and optional fields. Non-None fields are written
/// into the node's metadata JSON. After metadata is updated, a new active
/// version is created (deprecating the old one) with regenerated content
/// so that future skill_search embeddings stay in sync.
///
/// Used by the SkillDetail edit mode (Phase 4 G8).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLearnedSkillInput {
    pub node_id: String,
    pub context: Option<String>,
    pub principles: Option<String>,
    pub steps: Option<String>,
    pub pitfalls: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub validation_hint: Option<String>,
}

#[tauri::command]
pub async fn update_learned_skill(
    state: State<'_, AppState>,
    input: UpdateLearnedSkillInput,
) -> Result<(), String> {
    let store = &state.memory_graph_store;
    let node = store
        .get_node(&input.node_id)
        .map_err(|e| format!("lookup failed: {}", e))?
        .ok_or_else(|| format!("skill node '{}' not found", input.node_id))?;

    let mut meta = node.metadata.clone().unwrap_or(serde_json::json!({}));
    {
        let obj = meta.as_object_mut().ok_or("metadata is not an object")?;

        // Patch each field if provided
        if let Some(v) = &input.context {
            obj.insert("context".into(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &input.principles {
            obj.insert("principles".into(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &input.steps {
            obj.insert("steps".into(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &input.pitfalls {
            obj.insert("pitfalls".into(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &input.category {
            obj.insert("category".into(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = &input.tags {
            obj.insert(
                "tags".into(),
                serde_json::Value::Array(v.iter().map(|t| serde_json::Value::String(t.clone())).collect()),
            );
        }
        if let Some(v) = &input.validation_hint {
            obj.insert("validation_hint".into(), serde_json::Value::String(v.clone()));
        }
    } // drop obj

    // Persist metadata
    store
        .update_node(&input.node_id, None, None, Some(&meta))
        .map_err(|e| format!("metadata update failed: {}", e))?;

    // Rebuild active version content from the (now-updated) metadata
    let name = node.title.clone();
    let context = meta.get("context").and_then(|v| v.as_str()).unwrap_or("");
    let principles = meta.get("principles").and_then(|v| v.as_str()).unwrap_or("");
    let steps = meta.get("steps").and_then(|v| v.as_str()).unwrap_or("");
    let pitfalls = meta.get("pitfalls").and_then(|v| v.as_str()).unwrap_or("");
    let anti_patterns = meta.get("anti_patterns").and_then(|v| v.as_str());

    let mut new_content = format!(
        "# {}\n\n## 适用场景\n{}\n\n## 核心原则\n{}\n\n## 实现步骤\n{}",
        name, context, principles, steps
    );
    if let Some(ap) = anti_patterns {
        new_content.push_str("\n\n## 反模式\n");
        new_content.push_str(ap);
    }
    if !pitfalls.is_empty() {
        new_content.push_str("\n\n## 常见陷阱\n");
        new_content.push_str(pitfalls);
    }

    // Deprecate old active version & create new one
    if let Ok(Some(old_ver)) = store.get_active_version(&input.node_id) {
        let _ = store.deprecate_version(&old_ver.id);
        let new_ver = crate::memory_graph::models::MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: input.node_id.clone(),
            supersedes_version_id: Some(old_ver.id),
            status: crate::memory_graph::models::MemoryVersionStatus::Active,
            content: new_content,
            metadata: None,
            embedding_json: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        store
            .create_version(&new_ver)
            .map_err(|e| format!("version creation failed: {}", e))?;
    }

    tracing::info!(
        node_id = %input.node_id,
        title = %node.title,
        "update_learned_skill: fields updated, new version created"
    );
    Ok(())
}

/// Return all version records for a skill node, newest-first.
///
/// Used by the "演化历史" tab in Settings → 已学技能 to render a side-by-side
/// diff of the active version vs the most-recent superseded one.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillVersionInfo {
    pub id: String,
    pub status: String,
    pub content: String,
    pub created_at: String,
}

#[tauri::command]
pub async fn get_skill_versions(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<Vec<SkillVersionInfo>, String> {
    let store = &state.memory_graph_store;
    let versions = store
        .get_versions(&node_id)
        .map_err(|e| format!("Failed to get versions: {}", e))?;
    Ok(versions
        .into_iter()
        .map(|v| SkillVersionInfo {
            id: v.id,
            status: v.status.as_str().to_string(),
            content: v.content,
            created_at: v.created_at,
        })
        .collect())
}

/// Backfill `memory_keywords` rows for learned skills that are missing them.
///
/// Background: PR #58 added keyword writing to `store_skill_as_procedure`,
/// but the ~35 skills extracted before that PR have no keyword index rows.
/// L2 keyword recall therefore misses them entirely. This dev/maintenance
/// command rebuilds the index by running `extract_keywords` on every
/// learned skill that has no current keyword rows and inserting the
/// resulting tokens.
///
/// Idempotent: skills that already have any keyword rows are skipped.
/// Re-running the command is safe and a no-op once the index is full.
///
/// Returns counts so the UI can show "回填了 N 条 skill 共 K 个关键词".
#[tauri::command]
pub async fn backfill_skill_keywords(
    state: State<'_, AppState>,
    space_id: Option<String>,
) -> Result<serde_json::Value, String> {
    use crate::memory_graph::models::{MemoryKeyword, MemoryNodeKind};

    let store = &state.memory_graph_store;
    let sid = space_id.unwrap_or_else(|| "default".into());

    let nodes = store
        .list_nodes_by_kind(&sid, MemoryNodeKind::Procedure, 1000)
        .map_err(|e| format!("Failed to list skills: {}", e))?;

    let mut total_learned = 0usize;
    let mut already_indexed = 0usize;
    let mut backfilled_skills = 0usize;
    let mut total_keywords_inserted = 0usize;
    let now = chrono::Utc::now().to_rfc3339();

    for node in nodes {
        let meta = match node.metadata.as_ref() {
            Some(m) => m,
            None => continue,
        };
        if meta.get("skill_type").and_then(|v| v.as_str()) != Some("learned") {
            continue;
        }
        total_learned += 1;

        let existing = store
            .get_keywords_for_node(&node.id)
            .map_err(|e| format!("get_keywords_for_node failed: {}", e))?;
        if !existing.is_empty() {
            already_indexed += 1;
            continue;
        }

        let context = meta
            .get("context")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let keywords = crate::proactive::skill_parser::extract_keywords(&node.title, context);
        if keywords.is_empty() {
            // Title + context produced no usable tokens (very short / pure
            // punctuation). Skip — re-running won't change anything.
            continue;
        }

        let mut inserted_for_node = 0usize;
        for kw in keywords {
            let row = MemoryKeyword {
                id: uuid::Uuid::new_v4().to_string(),
                space_id: node.space_id.clone(),
                node_id: node.id.clone(),
                keyword: kw,
                created_at: now.clone(),
            };
            // Best-effort: a unique-constraint failure or any other DB
            // error here just means one keyword didn't land. Log + keep
            // going so a single bad row doesn't poison the whole backfill.
            match store.create_keyword(&row) {
                Ok(()) => {
                    inserted_for_node += 1;
                    total_keywords_inserted += 1;
                }
                Err(e) => {
                    tracing::warn!(
                        node_id = %node.id,
                        keyword = %row.keyword,
                        err = %e,
                        "backfill_skill_keywords: insert failed (continuing)"
                    );
                }
            }
        }
        if inserted_for_node > 0 {
            backfilled_skills += 1;
        }
    }

    tracing::info!(
        total_learned,
        already_indexed,
        backfilled_skills,
        total_keywords_inserted,
        "backfill_skill_keywords: done"
    );
    Ok(serde_json::json!({
        "totalLearnedSkills": total_learned,
        "alreadyIndexed": already_indexed,
        "backfilledSkills": backfilled_skills,
        "keywordsInserted": total_keywords_inserted,
    }))
}

// ─── D3: LLM-driven skill consolidation ───────────────────────────────

// Two-stage consolidation:
//   Stage 1 (propose): LLM only identifies which skills are duplicates.
//                      Just outputs cluster mapping — no merged content.
//                      Token cost: ~50 tokens per cluster instead of 500.
//   Stage 2 (apply):   Backend keeps canonical's existing content; deletes
//                      duplicates. User can manually edit canonical later.
//
// Why this split:
//   - First version asked LLM to also generate merged content per cluster
//     (4 markdown sections × 5 clusters × 35 skills → token starvation
//     with deepseek-v4-flash, returned empty response, parse failure)
//   - Identification is the hard semantic task; merging is mechanical and
//     can be done deterministically (or punted to manual edit)
//   - Reliable failure mode: if LLM emits empty / garbage, just show
//     "no clusters found" instead of crashing the dialog
//
// Robustness improvements (P0+P1+P2):
//   P0-1: Dynamic max_tokens = max(2048, skill_count * 200).min(8192)
//         Prevents token starvation with DeepSeek models that count
//         reasoning_content against the output budget.
//   P0-2: Truncation salvage — when finish_reason == "length", attempt
//         parse_json_array_tolerant before giving up (partial clusters may
//         be complete).
//   P1-1: Pre-clustering — when >30 skills, group by title word overlap
//         (Jaccard > 0.4) into independent batches to keep per-call token
//         budget bounded.
//   P1-2: Auto-retry — on length failure, retry once with 2× max_tokens.
//   P2-1: Progress events — emit "skill-consolidation:progress" to the
//         frontend for real-time stage/percentage feedback.
//   P2-2: Cancellation — cancel_skill_consolidation Tauri command + atomic
//         flag checked between batches.

const CONSOLIDATION_SYSTEM_PROMPT: &str = "你是技能去重助手。\
用户给你一个 JSON 数组，每条是已学技能：{id, title, context}。\
你的唯一任务：把**概念上重复**的技能聚合成 cluster。\n\n\
输出**纯 JSON 数组**（不要 markdown 代码块、不要任何解释文字），shape：\n\
[\n  {\
\n    \"cluster\": [\"id1\", \"id2\"],   // 同概念的 id（必须 ≥ 2 个）\
\n    \"canonical_id\": \"id1\",         // 选最完整 / 最准确的那条作为保留\
\n    \"reason\": \"简短说明为什么是同一概念\"\
\n  }\
\n]\n\n规则：\
\n1. cluster 至少 2 个 id；单条独立的不要输出。\
\n2. 不同概念绝不混在一个 cluster — 宁可保留独立条目。\
\n3. 如果完全没有可合并的，直接返回 []。\
\n4. 输出必须以 [ 开头，以 ] 结尾。不要任何前缀 / 后缀文字。\
\n5. canonical_id 必须是 cluster 里的某一个 id。";

// ─── P2-2: Cancel flag for in-flight consolidation ────────────────────

static CONSOLIDATION_CANCELLED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn cancel_skill_consolidation() {
    CONSOLIDATION_CANCELLED.store(true, Ordering::SeqCst);
    tracing::info!("cancel_skill_consolidation: cancellation requested");
}

// ─── P2-1: Progress event helper ──────────────────────────────────────

fn emit_consolidation_progress(
    app_handle: &tauri::AppHandle,
    stage: &str,
    current: usize,
    total: usize,
    detail: &str,
) {
    let _ = app_handle.emit(
        "skill-consolidation:progress",
        serde_json::json!({
            "stage": stage,
            "current": current,
            "total": total,
            "detail": detail,
        }),
    );
}

// ─── P0-1: Dynamic max_tokens ─────────────────────────────────────────

fn consolidation_max_tokens(skill_count: usize) -> u32 {
    (skill_count as u32 * 200).max(2048).min(8192)
}

// ─── P1-1: Lightweight title-based pre-clustering ─────────────────────

/// Groups skills by title word overlap (Jaccard similarity > 0.4).
/// Returns batches of up to ~20 skills each. Used when total > 30.
fn precluster_by_title(skills: &[serde_json::Value]) -> Vec<Vec<serde_json::Value>> {
    let n = skills.len();
    if n <= 30 {
        return vec![skills.to_vec()];
    }

    let titles: Vec<&str> = skills
        .iter()
        .filter_map(|s| s.get("title").and_then(|v| v.as_str()))
        .collect();

    // Union-Find
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }
    fn union(parent: &mut [usize], x: usize, y: usize) {
        let rx = find(parent, x);
        let ry = find(parent, y);
        if rx != ry {
            parent[rx] = ry;
        }
    }

    for i in 0..n {
        for j in (i + 1)..n {
            let words_i: std::collections::HashSet<&str> = titles[i]
                .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
                .filter(|w| w.len() >= 2)
                .collect();
            let words_j: std::collections::HashSet<&str> = titles[j]
                .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
                .filter(|w| w.len() >= 2)
                .collect();
            if words_i.is_empty() || words_j.is_empty() {
                continue;
            }
            let intersection = words_i.intersection(&words_j).count();
            let union_size = words_i.union(&words_j).count();
            if union_size > 0 {
                let similarity = intersection as f64 / union_size as f64;
                if similarity > 0.4 {
                    union(&mut parent, i, j);
                }
            }
        }
    }

    // Group by root
    let mut groups: std::collections::HashMap<usize, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    for (i, skill) in skills.iter().enumerate() {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(skill.clone());
    }

    let batches: Vec<Vec<serde_json::Value>> = groups.into_values().collect();
    // Split large batches (> 25) further
    let mut result: Vec<Vec<serde_json::Value>> = Vec::new();
    for batch in batches {
        if batch.len() > 25 {
            for chunk in batch.chunks(25) {
                result.push(chunk.to_vec());
            }
        } else {
            result.push(batch);
        }
    }
    result
}

// ─── Core LLM call (extracted for retry — P1-2) ───────────────────────

struct ConsolidationLlmOutput {
    text: String,
    finish_reason: Option<String>,
}

async fn call_consolidation_llm(
    state: &AppState,
    skills_input: &[serde_json::Value],
    max_tokens: u32,
    model_override: Option<String>,
) -> Result<ConsolidationLlmOutput, String> {
    let user_content =
        serde_json::to_string_pretty(skills_input)
            .map_err(|e| format!("Failed to serialize skills: {}", e))?;

    let llm_cfg = if let Some((provider_id, model, api_key, base_url, _)) =
        state.provider_service.get_chat_llm_config().await
    {
        crate::llm::llm_config_from_provider(
            &provider_id,
            model_override.as_deref().unwrap_or(&model),
            &api_key,
            &base_url,
            max_tokens,
            0.1,
            None, // secondary call site — out of scope (Task 2)
        )
    } else if let Some((provider_id, model, api_key, base_url, _api)) =
        state.provider_service.get_active_llm_config().await
    {
        crate::llm::llm_config_from_provider(
            &provider_id,
            model_override.as_deref().unwrap_or(&model),
            &api_key,
            &base_url,
            max_tokens,
            0.1,
            None, // secondary call site — out of scope (Task 2)
        )
    } else {
        return Err("未配置可用的 LLM provider".into());
    };

    tracing::info!(
        model = %llm_cfg.model,
        skill_count = skills_input.len(),
        max_tokens,
        "propose_skill_consolidation: calling LLM"
    );

    let provider = crate::llm::create_provider(&llm_cfg)
        .map_err(|e| format!("Failed to create LLM provider: {}", e))?;
    let messages = vec![
        ChatMessage::system(CONSOLIDATION_SYSTEM_PROMPT),
        ChatMessage::user(&user_content),
    ];
    let cfg = crate::llm::CompletionConfig {
        model: llm_cfg.model.clone(),
        max_tokens,
        temperature: 0.1,
        thinking_enabled: false,
    };
    let output = provider
        .complete(messages, vec![], &cfg)
        .await
        .map_err(|e| format!("LLM call failed: {}", e))?;

    let (text, finish_reason) = match output {
        crate::agent::types::RespondOutput::Text {
            text, metadata, ..
        } => (text, metadata.finish_reason),
        crate::agent::types::RespondOutput::ToolCalls {
            text, metadata, ..
        } => (text.unwrap_or_default(), metadata.finish_reason),
    };

    let preview: String = text.chars().take(500).collect();
    tracing::info!(
        finish_reason = ?finish_reason,
        text_len = text.len(),
        preview = %preview,
        "propose_skill_consolidation: LLM response received"
    );

    Ok(ConsolidationLlmOutput {
        text,
        finish_reason,
    })
}

// ─── Main command ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn propose_skill_consolidation(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    space_id: Option<String>,
) -> Result<serde_json::Value, String> {
    use crate::memory_graph::models::MemoryNodeKind;

    // Reset cancel flag at start of new consolidation
    CONSOLIDATION_CANCELLED.store(false, Ordering::SeqCst);

    emit_consolidation_progress(&app_handle, "loading", 0, 0, "正在加载学得技能…");

    let store = &state.memory_graph_store;
    let sid = space_id.unwrap_or_else(|| "default".into());

    // Load all learned skills.
    let nodes = store
        .list_nodes_by_kind(&sid, MemoryNodeKind::Procedure, 500)
        .map_err(|e| format!("Failed to list skills: {}", e))?;
    let learned: Vec<_> = nodes
        .into_iter()
        .filter(|n| {
            n.metadata
                .as_ref()
                .and_then(|m| m.get("skill_type"))
                .and_then(|v| v.as_str())
                == Some("learned")
        })
        .collect();

    if learned.len() < 2 {
        emit_consolidation_progress(&app_handle, "done", learned.len(), learned.len(), "技能不足");
        return Ok(serde_json::json!({
            "clusters": [],
            "total_skills": learned.len(),
            "proposed_canonical_count": learned.len(),
        }));
    }

    // Build LLM input — id + title + truncated context
    let skills_input: Vec<serde_json::Value> = learned
        .iter()
        .map(|n| {
            let meta = n.metadata.as_ref();
            let context = meta
                .and_then(|m| m.get("context"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let truncated_ctx = if context.chars().count() > 120 {
                let cut: String = context.chars().take(120).collect();
                format!("{}…", cut)
            } else {
                context.to_string()
            };
            serde_json::json!({
                "id": n.id,
                "title": n.title,
                "context": truncated_ctx,
            })
        })
        .collect();

    let total_skills = learned.len();

    // P1-1: Pre-cluster into batches when > 30 skills
    let batches = precluster_by_title(&skills_input);
    let batch_count = batches.len();
    if batch_count > 1 {
        emit_consolidation_progress(
            &app_handle,
            "preclustering",
            0,
            batch_count,
            &format!(
                "技能数 {} 超过阈值，已按标题相似度预分为 {} 组",
                total_skills, batch_count
            ),
        );
    }

    let mut all_clusters_raw: Vec<serde_json::Value> = Vec::new();

    // Process each batch
    for (batch_idx, batch) in batches.iter().enumerate() {
        // P2-2: Check cancellation between batches
        if CONSOLIDATION_CANCELLED.load(Ordering::SeqCst) {
            emit_consolidation_progress(&app_handle, "cancelled", batch_idx, batch_count, "操作已取消");
            return Err("操作已取消".into());
        }

        let batch_size = batch.len();
        let batch_label = if batch_count > 1 {
            format!("第 {}/{} 组 ({} 条技能)", batch_idx + 1, batch_count, batch_size)
        } else {
            format!("共 {} 条技能", batch_size)
        };
        emit_consolidation_progress(&app_handle, "analyzing", batch_idx + 1, batch_count, &batch_label);

        // P0-1: Dynamic max_tokens
        let base_max_tokens = consolidation_max_tokens(batch_size);

        // First attempt
        let llm_output = call_consolidation_llm(&state, batch, base_max_tokens, None).await?;

        // P0-2 + P1-2: Handle truncation — try salvage, then retry with 2x tokens
        let (text_to_parse, was_retried) =
            if llm_output.finish_reason.as_deref() == Some("length") {
                // Try to salvage partial JSON first
                if !llm_output.text.trim().is_empty() {
                    if let Some(partial) = parse_json_array_tolerant(&llm_output.text) {
                        tracing::warn!(
                            batch = batch_idx,
                            salvaged = partial.len(),
                            "propose_skill_consolidation: truncated response, salvaged partial clusters"
                        );
                        // Use salvaged clusters — don't retry
                        all_clusters_raw.extend(partial);
                        continue;
                    }
                }

                // P1-2: Retry with 2× max_tokens
                let retry_tokens = (base_max_tokens * 2).min(8192);
                if retry_tokens > base_max_tokens {
                    emit_consolidation_progress(
                        &app_handle,
                        "retrying",
                        batch_idx + 1,
                        batch_count,
                        &format!("响应被截断，以 {} tokens 重试…", retry_tokens),
                    );
                    tracing::warn!(
                        batch = batch_idx,
                        base_max_tokens,
                        retry_tokens,
                        "propose_skill_consolidation: retrying with increased max_tokens"
                    );
                    let retry_output =
                        call_consolidation_llm(&state, batch, retry_tokens, None).await?;
                    (retry_output.text, true)
                } else {
                    return Err(format!(
                        "LLM 响应被截断且已达最大 token 预算（{}）。\
                         当前批次 {} 条技能，建议减少技能数或切换模型。",
                        base_max_tokens, batch_size
                    ));
                }
            } else {
                (llm_output.text, false)
            };

        // Handle empty response
        if text_to_parse.trim().is_empty() {
            let msg = if was_retried {
                format!(
                    "LLM 重试后仍返回空响应（model: 当前模型, batch: {} 条技能, max_tokens: {}）。\
                     试试切换到其他模型，或检查模型是否支持 system prompt。",
                    batch_size, base_max_tokens
                )
            } else {
                format!(
                    "LLM 返回了空响应。试试切换到其他模型，\
                     或检查模型是否支持 system prompt。"
                )
            };
            return Err(msg);
        }

        // Parse JSON
        let clusters_raw: Vec<serde_json::Value> =
            parse_json_array_tolerant(&text_to_parse).ok_or_else(|| {
                let preview: String = text_to_parse.chars().take(500).collect();
                format!(
                    "LLM 输出无法解析为 JSON 数组。原始响应（截断）：\n{}",
                    preview
                )
            })?;

        tracing::info!(
            batch = batch_idx,
            clusters = clusters_raw.len(),
            was_retried,
            "propose_skill_consolidation: batch parsed"
        );

        all_clusters_raw.extend(clusters_raw);
    }

    // P2-1: Validation phase
    emit_consolidation_progress(
        &app_handle,
        "validating",
        all_clusters_raw.len(),
        all_clusters_raw.len(),
        "正在验证整合方案…",
    );

    // Validate clusters against the actual id set
    let id_to_node: std::collections::HashMap<
        String,
        &crate::memory_graph::models::MemoryNode,
    > = learned.iter().map(|n| (n.id.clone(), n)).collect();

    let mut clusters_out: Vec<serde_json::Value> = Vec::new();
    for c in all_clusters_raw {
        let cluster_ids: Vec<String> = c
            .get("cluster")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .filter(|id| id_to_node.contains_key(id))
                    .collect()
            })
            .unwrap_or_default();
        if cluster_ids.len() < 2 {
            continue;
        }
        let canonical_id = c
            .get("canonical_id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .filter(|id| cluster_ids.contains(id))
            .unwrap_or_else(|| cluster_ids[0].clone());

        let canonical = id_to_node.get(&canonical_id).cloned();
        let canonical_title = canonical
            .as_ref()
            .map(|n| n.title.clone())
            .unwrap_or_default();
        let canonical_meta = canonical.and_then(|n| n.metadata.as_ref());
        let pull = |key: &str| -> String {
            canonical_meta
                .and_then(|m| m.get(key))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };

        let duplicate_ids: Vec<String> = cluster_ids
            .iter()
            .filter(|id| **id != canonical_id)
            .cloned()
            .collect();
        let duplicate_titles: Vec<String> = duplicate_ids
            .iter()
            .filter_map(|id| id_to_node.get(id).map(|n| n.title.clone()))
            .collect();

        clusters_out.push(serde_json::json!({
            "canonical_id": canonical_id,
            "canonical_title": canonical_title.clone(),
            "merged_title": canonical_title,
            "merged_context": pull("context"),
            "merged_principles": pull("principles"),
            "merged_steps": pull("steps"),
            "merged_pitfalls": pull("pitfalls"),
            "duplicate_ids": duplicate_ids,
            "duplicate_titles": duplicate_titles,
            "reason": c.get("reason").and_then(|v| v.as_str()).unwrap_or(""),
        }));
    }

    let consolidated: usize = clusters_out
        .iter()
        .map(|c| {
            c.get("duplicate_ids")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0)
        })
        .sum();
    let total = learned.len();
    let proposed = total.saturating_sub(consolidated);

    emit_consolidation_progress(
        &app_handle,
        "done",
        total,
        total,
        &format!("分析完成：{} 条技能 → {} 组可合并", total, clusters_out.len()),
    );

    Ok(serde_json::json!({
        "clusters": clusters_out,
        "total_skills": total,
        "proposed_canonical_count": proposed,
    }))
}

#[tauri::command]
pub async fn apply_skill_consolidation(
    state: State<'_, AppState>,
    plan: serde_json::Value,
) -> Result<serde_json::Value, String> {
    use crate::memory_graph::models::{MemoryVersion, MemoryVersionStatus};

    let store = &state.memory_graph_store;
    let clusters = plan
        .get("clusters")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut applied = 0u32;
    let mut deprecated = 0u32;
    let mut updated = 0u32;
    let now = chrono::Utc::now().to_rfc3339();

    for c in clusters {
        let canonical_id = c
            .get("canonical_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if canonical_id.is_empty() {
            continue;
        }

        let merged_title = c
            .get("merged_title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let merged_context = c
            .get("merged_context")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let merged_principles = c
            .get("merged_principles")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let merged_steps = c
            .get("merged_steps")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let merged_pitfalls = c
            .get("merged_pitfalls")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let Ok(Some(node)) = store.get_node(&canonical_id) else {
            tracing::warn!(canonical_id, "consolidation: canonical node not found, skipping cluster");
            continue;
        };

        // Update canonical metadata.
        let mut meta = node.metadata.clone().unwrap_or(serde_json::json!({}));
        if let Some(obj) = meta.as_object_mut() {
            if !merged_context.is_empty() {
                obj.insert("context".into(), serde_json::Value::String(merged_context.clone()));
            }
            if !merged_principles.is_empty() {
                obj.insert("principles".into(), serde_json::Value::String(merged_principles.clone()));
            }
            if !merged_steps.is_empty() {
                obj.insert("steps".into(), serde_json::Value::String(merged_steps.clone()));
            }
            if !merged_pitfalls.is_empty() {
                obj.insert("pitfalls".into(), serde_json::Value::String(merged_pitfalls.clone()));
            }
            obj.insert("consolidated_at".into(), serde_json::Value::String(now.clone()));
        }

        let new_title = if merged_title.trim().is_empty() {
            node.title.clone()
        } else {
            merged_title
        };
        if let Err(e) = store.update_node(&canonical_id, Some(&new_title), None, Some(&meta)) {
            tracing::warn!(canonical_id, err = %e, "consolidation: update_node failed");
            continue;
        }

        // Deprecate old active version, create new with merged content.
        if let Ok(Some(active)) = store.get_active_version(&canonical_id) {
            let _ = store.deprecate_version(&active.id);
        }
        let new_content = format!(
            "# {}\n\n## 适用场景\n{}\n\n## 核心原则\n{}\n\n## 实现步骤\n{}\n\n## 常见陷阱\n{}",
            new_title, merged_context, merged_principles, merged_steps, merged_pitfalls
        );
        let _ = store.create_version(&MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: canonical_id.clone(),
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: new_content,
            metadata: None,
            embedding_json: None,
            created_at: now.clone(),
        });
        updated += 1;

        // Hard-delete duplicates (cascades to versions / keywords / edges
        // / FTS). Hard delete chosen over soft: the user explicitly asked
        // for these to merge; leaving deprecated nodes around would just
        // re-pollute boot ranking and recall.
        let duplicate_ids = c
            .get("duplicate_ids")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for did in duplicate_ids {
            if let Some(d) = did.as_str() {
                if d == canonical_id {
                    continue;
                }
                if store.delete_node(d).is_ok() {
                    deprecated += 1;
                } else {
                    tracing::warn!(duplicate_id = d, "consolidation: delete_node failed");
                }
            }
        }

        applied += 1;
    }

    tracing::info!(
        applied,
        deprecated,
        updated,
        "skill consolidation completed"
    );
    Ok(serde_json::json!({
        "applied_clusters": applied,
        "deprecated_skills": deprecated,
        "updated_skills": updated,
    }))
}

/// Parse an LLM response that should contain a JSON array. Tolerates
/// markdown code fences (```json ... ```), leading prose, and trailing
/// whitespace. Returns None if no parseable array is found.
fn parse_json_array_tolerant(text: &str) -> Option<Vec<serde_json::Value>> {
    // Try direct parse first.
    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(text.trim()) {
        return Some(arr);
    }
    // Strip ```json ... ``` fences if present.
    let stripped = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```JSON")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(stripped) {
        return Some(arr);
    }
    // Last resort: find first '[' and last ']'.
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}
