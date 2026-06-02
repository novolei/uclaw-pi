//! 主动召回增强
//!
//! 在合适的时机主动提供记忆，包括：
//! - 任务类型匹配 → 查找相似历史任务
//! - 错误模式匹配 → 查找相关失败经验
//! - 工具使用 → 查找工具使用模式
//! - 文件操作 → 查找文件相关记忆
//!
//! ## 设计
//! ```text
//! 用户输入 + 上下文 → proactive_recall()
//!     ├─ 分析上下文特征
//!     │   ├─ 任务类型
//!     │   ├─ 涉及的工具/文件
//!     │   └─ 错误信号
//!     ├─ 多维度触发条件检查
//!     ├─ 执行混合检索
//!     └─ 按相关性排序返回
//! ```

use std::sync::Arc;

use crate::error::Error;
use crate::memory_graph::recall::{MemoryRecallCandidate, MemoryRecallEngine};
use crate::memory_graph::store::MemoryGraphStore;

use super::failure_memory::FailureMemoryManager;
use super::task_memory::{TaskMemoryManager, TaskType};
use super::tool_memory::ToolUsageMemoryManager;

// ─── 主动召回上下文 ───────────────────────────────────────────────────

/// 主动召回上下文
#[derive(Debug, Clone)]
pub struct ProactiveRecallContext {
    /// 任务类型（可选，自动推断）
    pub task_type: Option<TaskType>,
    /// 涉及的文件路径
    pub files_involved: Vec<String>,
    /// 工具使用建议
    pub tools_suggested: Vec<String>,
    /// 错误信号
    pub error_signals: Vec<String>,
    /// 用户查询文本
    pub user_query: Option<String>,
    /// 工作区 ID
    pub space_id: String,
}

// ─── 背景知识上下文 ───────────────────────────────────────────────────

/// 准备注入 system prompt 的背景知识上下文
#[derive(Debug, Clone)]
pub struct BackgroundContext {
    /// 人格画像摘要
    pub personality_summary: Option<String>,
    /// 最近任务上下文
    pub recent_tasks: Vec<String>,
    /// 相关失败经验警告
    pub failure_warnings: Vec<String>,
    /// 工具使用建议
    pub tool_suggestions: Vec<String>,
    /// 相关记忆候选项
    pub related_memories: Vec<MemoryRecallCandidate>,
}

// ─── 主动召回服务 ─────────────────────────────────────────────────────

/// 主动召回服务
///
/// 编排多维度触发条件和智能召回。
pub struct ProactiveRecallService {
    store: Arc<MemoryGraphStore>,
    task_memory: Arc<TaskMemoryManager>,
    tool_memory: Arc<ToolUsageMemoryManager>,
    failure_memory: Arc<FailureMemoryManager>,
}

impl ProactiveRecallService {
    pub fn new(
        store: Arc<MemoryGraphStore>,
        task_memory: Arc<TaskMemoryManager>,
        tool_memory: Arc<ToolUsageMemoryManager>,
        failure_memory: Arc<FailureMemoryManager>,
    ) -> Self {
        Self {
            store,
            task_memory,
            tool_memory,
            failure_memory,
        }
    }

    // ─── 主动召回 ────────────────────────────────────────────────

    /// 智能召回：在合适的时机主动提供记忆。
    ///
    /// 多维度触发条件：
    /// 1. 任务类型匹配 → 查找相似历史任务
    /// 2. 错误模式匹配 → 查找相关失败经验
    /// 3. 工具使用 → 查找工具使用模式
    /// 4. 文件操作 → 查找文件相关记忆
    pub async fn proactive_recall(
        &self,
        context: &ProactiveRecallContext,
        max_results: usize,
    ) -> Result<Vec<MemoryRecallCandidate>, Error> {
        let mut all_candidates: Vec<MemoryRecallCandidate> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // 1. 任务类型匹配 — 查找相似历史任务
        if let Some(ref task_type) = context.task_type {
            let query = format!("{:?} {}", task_type, context.user_query.as_deref().unwrap_or(""));
            let similar_tasks = self
                .task_memory
                .find_similar_tasks(&context.space_id, &query, 5)?;

            for task in &similar_tasks {
                if let Some(ref desc) = task.solution_summary {
                    // 从 MemoryGraph 中查找任务记忆节点
                    if let Ok(nodes) = self
                        .store
                        .search_by_keyword(&context.space_id, desc)
                    {
                        for node in nodes {
                            if seen.insert(node.id.clone()) {
                                all_candidates.push(MemoryRecallCandidate {
                                    node_id: node.id.clone(),
                                    title: node.title.clone(),
                                    content: desc.clone(),
                                    kind: node.kind,
                                    source: "proactive_task_match".to_string(),
                                    reason: format!("similar task: {:?}", task_type),
                                    score: Some(0.7),
                                    fts_rank: None,
                                    vector_rank: None,
                                    matched_keywords: vec![],
                                    metadata: node.metadata.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        // 2. 错误信号匹配 — 查找相关失败经验
        for error_signal in &context.error_signals {
            let failures = self.failure_memory.find_related_failures(
                &context.space_id,
                &context.user_query.as_deref().unwrap_or(""),
                error_signal,
                3,
            )?;

            for failure in &failures {
                if let Some(ref node_id) = failure.node_id {
                    if seen.insert(node_id.clone()) {
                        all_candidates.push(MemoryRecallCandidate {
                            node_id: node_id.clone(),
                            title: format!("失败经验: {}", failure.error_pattern),
                            content: failure
                                .resolution
                                .clone()
                                .unwrap_or_else(|| failure.error_pattern.clone()),
                            kind: crate::memory_graph::models::MemoryNodeKind::Episode,
                            source: "proactive_failure_match".to_string(),
                            reason: format!(
                                "related failure: {}",
                                failure.error_pattern
                            ),
                            score: Some(0.8),
                            fts_rank: None,
                            vector_rank: None,
                            matched_keywords: vec![failure.error_pattern.clone()],
                            metadata: None,
                        });
                    }
                }
            }
        }

        // 3. 工具使用 — 查找工具使用模式
        for tool_name in &context.tools_suggested {
            if let Ok(Some(stats)) = self
                .tool_memory
                .get_tool_stats(&context.space_id, tool_name)
            {
                // 查找工具统计信息并作为候选
                let summary = format!(
                    "工具 {} 使用统计: 成功率 {:.0}%, 平均延迟 {}ms",
                    tool_name,
                    stats.success_rate * 100.0,
                    stats.avg_latency_ms
                );

                let node_id = format!("tool_stats_{}", tool_name);
                if seen.insert(node_id.clone()) {
                    all_candidates.push(MemoryRecallCandidate {
                        node_id,
                        title: format!("工具统计: {}", tool_name),
                        content: summary,
                        kind: crate::memory_graph::models::MemoryNodeKind::Procedure,
                        source: "proactive_tool_stats".to_string(),
                        reason: format!("tool usage pattern: {}", tool_name),
                        score: Some(0.5),
                        fts_rank: None,
                        vector_rank: None,
                        matched_keywords: vec![tool_name.clone()],
                        metadata: None,
                    });
                }
            }
        }

        // 4. 文件操作 — 查找文件相关记忆
        for file_path in &context.files_involved {
            let file_name = std::path::Path::new(file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(file_path);

            if let Ok(nodes) = self.store.search_by_keyword(&context.space_id, file_name) {
                for node in nodes {
                    if seen.insert(node.id.clone()) {
                        all_candidates.push(MemoryRecallCandidate {
                            node_id: node.id.clone(),
                            title: node.title.clone(),
                            content: format!("相关文件: {}", file_path),
                            kind: node.kind,
                            source: "proactive_file_match".to_string(),
                            reason: format!("file related: {}", file_path),
                            score: Some(0.6),
                            fts_rank: None,
                            vector_rank: None,
                            matched_keywords: vec![file_name.to_string()],
                            metadata: node.metadata.clone(),
                        });
                    }
                }
            }
        }

        // 5. 使用 MemoryRecallEngine 进行语义召回作为补充
        if let Some(ref query) = context.user_query {
            if !query.trim().is_empty() {
                let recall_config = crate::memory_graph::recall::MemoryRecallConfig {
                    boot_limit: 3,
                    trigger_limit: 5,
                    seed_limit: 5,
                    expansion_limit: 3,
                    recent_limit: 3,
                    ..Default::default()
                };

                let recall_engine = MemoryRecallEngine::new(
                    self.store.clone(),
                    recall_config,
                );

                if let Ok(plan) = recall_engine
                    .build_recall_plan(&context.space_id, query, false)
                    .await
                {
                    for c in &plan.boot {
                        if seen.insert(c.node_id.clone()) {
                            all_candidates.push(c.clone());
                        }
                    }
                    for c in &plan.relevant {
                        if seen.insert(c.node_id.clone()) {
                            all_candidates.push(c.clone());
                        }
                    }
                    for c in &plan.triggered {
                        if seen.insert(c.node_id.clone()) {
                            all_candidates.push(c.clone());
                        }
                    }
                }
            }
        }

        // 6. 按分数排序并截断
        all_candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        all_candidates.truncate(max_results);

        tracing::debug!(
            candidates = all_candidates.len(),
            error_signals = context.error_signals.len(),
            files = context.files_involved.len(),
            "[ProactiveRecallService] proactive_recall completed"
        );

        Ok(all_candidates)
    }

    // ─── 背景知识准备 ────────────────────────────────────────────

    /// 在 Agent 循环开始前准备背景上下文。
    ///
    /// 构建包含人格画像摘要、最近任务、失败警告的紧凑上下文块。
    pub async fn prepare_background_context(
        &self,
        user_query: &str,
        _session_id: Option<&str>,
        space_id: &str,
    ) -> Result<BackgroundContext, Error> {
        // 1. 最近任务上下文
        let recent_tasks = self
            .task_memory
            .list_recent_tasks(space_id, 5)?;
        let task_summaries: Vec<String> = recent_tasks
            .iter()
            .map(|t| {
                format!(
                    "- [{}] {} ({:?})",
                    t.status.as_str(),
                    t.solution_summary.as_deref().unwrap_or(""),
                    t.task_type
                )
            })
            .collect();

        // 2. 潜在失败经验警告（基于当前查询检查是否有匹配的错误模式）
        let failure_warnings: Vec<String> = {
            let query_lower = user_query.to_lowercase();
            let error_keywords = ["error", "失败", "错误", "bug", "fix", "修复", "编译", "compile"];

            if error_keywords.iter().any(|k| query_lower.contains(k)) {
                let failures = self.failure_memory.find_related_failures(
                    space_id,
                    user_query,
                    user_query,
                    3,
                )?;

                failures
                    .iter()
                    .map(|f| {
                        format!(
                            "⚠ 已知失败模式: {} — 解决方案: {}",
                            f.error_pattern,
                            f.resolution.as_deref().unwrap_or("暂无")
                        )
                    })
                    .collect()
            } else {
                Vec::new()
            }
        };

        // 3. 工具使用建议（基于上下文推断）
        let tool_suggestions: Vec<String> = {
            let mut suggestions = Vec::new();
            let query_lower = user_query.to_lowercase();

            if query_lower.contains("search") || query_lower.contains("搜索") || query_lower.contains("查找") {
                suggestions.push("grep_code".to_string());
                suggestions.push("search_file".to_string());
            }
            if query_lower.contains("compile") || query_lower.contains("编译") || query_lower.contains("build") {
                suggestions.push("run_in_terminal".to_string());
            }
            if query_lower.contains("git") || query_lower.contains("commit") {
                suggestions.push("git".to_string());
            }

            suggestions
        };

        // 4. 语义召回相关记忆
        let recall_config = crate::memory_graph::recall::MemoryRecallConfig {
            boot_limit: 5,
            trigger_limit: 3,
            seed_limit: 3,
            expansion_limit: 2,
            recent_limit: 2,
            ..Default::default()
        };

        let recall_engine = MemoryRecallEngine::new(
            self.store.clone(),
            recall_config,
        );

        let related_memories = recall_engine
            .build_recall_plan(space_id, user_query, false)
            .await
            .map(|plan| {
                let mut all = Vec::new();
                all.extend(plan.boot);
                all.extend(plan.relevant);
                all.extend(plan.triggered);
                all
            })
            .unwrap_or_default();

        Ok(BackgroundContext {
            personality_summary: None, // 人格画像太细节，按需加载
            recent_tasks: task_summaries,
            failure_warnings,
            tool_suggestions,
            related_memories,
        })
    }

    /// 将 BackgroundContext 格式化为系统提示文本
    pub fn format_background_for_prompt(ctx: &BackgroundContext) -> String {
        let mut parts = Vec::new();

        if !ctx.failure_warnings.is_empty() {
            parts.push("## ⚠ 已知失败经验".to_string());
            for w in &ctx.failure_warnings {
                parts.push(w.clone());
            }
        }

        if !ctx.recent_tasks.is_empty() {
            parts.push("\n## 最近任务".to_string());
            for t in &ctx.recent_tasks {
                parts.push(t.clone());
            }
        }

        if !ctx.tool_suggestions.is_empty() {
            parts.push(format!(
                "\n## 推荐工具: {}",
                ctx.tool_suggestions.join(", ")
            ));
        }

        if parts.is_empty() {
            return String::new();
        }

        parts.join("\n")
    }
}
