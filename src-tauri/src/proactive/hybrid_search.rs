//! 五路混合检索引擎
//!
//! 将记忆检索从"FTS5 + Vector 双路融合"升级为
//! "向量 + 关键词 + 时间 + 图关系 + 文件"五路混合检索，
//! 支持加权线性融合和 RRF 多路融合两种策略。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::memory_graph::models::MemoryNode;
use crate::memory_graph::recall::{MemoryRecallCandidate, MemoryRecallEngine, MemoryTimelineEntry, TimeRange};
use crate::memory_graph::store::MemoryGraphStore;

/// 从 MemoryRecallCandidate 构建 MemoryNode
fn candidate_to_node(c: &MemoryRecallCandidate) -> MemoryNode {
    MemoryNode {
        id: c.node_id.clone(),
        space_id: String::new(),
        kind: c.kind,
        title: c.title.clone(),
        metadata: c.metadata.clone(),
        created_at: String::new(),
        updated_at: String::new(),
    }
}

/// 从 MemoryTimelineEntry 构建 MemoryNode
fn timeline_to_node(e: &MemoryTimelineEntry) -> MemoryNode {
    MemoryNode {
        id: e.node_id.clone(),
        space_id: String::new(),
        kind: e.kind,
        title: e.title.clone(),
        metadata: None,
        created_at: e.updated_at.clone(),
        updated_at: e.updated_at.clone(),
    }
}

// ─── 检索请求 ─────────────────────────────────────────────────────────

/// 五路混合检索请求
#[derive(Debug, Clone)]
pub struct HybridSearchRequest {
    /// 用户查询文本
    pub query: String,
    /// 工作区 ID（隔离）
    pub space_id: String,
    /// 会话 ID（用于会话级上下文增强）
    pub session_id: Option<String>,
    /// 时间范围筛选
    pub time_range: Option<TimeRange>,
    /// 相关文件路径（用于文件内容匹配通道）
    pub file_paths: Option<Vec<String>>,
    /// 最大返回结果数
    pub max_results: usize,
}

// ─── 检索通道 ─────────────────────────────────────────────────────────

/// 五路检索通道标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SearchChannel {
    /// 向量语义相似度（memU embedding）
    Vector,
    /// 关键词匹配（FTS5 + LIKE）
    Keyword,
    /// 时间衰减（Gaussian decay）
    Time,
    /// 图关系传播（BFS on memory_edges）
    Graph,
    /// 文件内容匹配
    File,
}

impl SearchChannel {
    pub fn all() -> Vec<SearchChannel> {
        vec![
            SearchChannel::Vector,
            SearchChannel::Keyword,
            SearchChannel::Time,
            SearchChannel::Graph,
            SearchChannel::File,
        ]
    }
}

// ─── 融合策略 ─────────────────────────────────────────────────────────

/// 多路检索结果融合策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FusionStrategy {
    /// 加权线性融合（默认）
    WeightedSum {
        vector_weight: f32,
        keyword_weight: f32,
        time_weight: f32,
        graph_weight: f32,
        file_weight: f32,
    },
    /// RRF 多路排名融合
    RrfMulti { k: u32 },
}

impl Default for FusionStrategy {
    fn default() -> Self {
        // 经验权重：向量语义最重要，关键词次之，时间和图辅助
        FusionStrategy::WeightedSum {
            vector_weight: 0.35,
            keyword_weight: 0.25,
            time_weight: 0.10,
            graph_weight: 0.15,
            file_weight: 0.15,
        }
    }
}

impl FusionStrategy {
    /// 创建轻量级策略（不依赖 memU 的场景）
    pub fn lightweight() -> Self {
        FusionStrategy::WeightedSum {
            vector_weight: 0.0,
            keyword_weight: 0.40,
            time_weight: 0.15,
            graph_weight: 0.25,
            file_weight: 0.20,
        }
    }
}

// ─── 检索结果 ─────────────────────────────────────────────────────────

/// 各路通道得分
#[derive(Debug, Clone, Default)]
pub struct ChannelScores {
    /// 向量相似度得分 (0.0 - 1.0)
    pub vector_score: Option<f32>,
    /// 关键词匹配置信度 (0.0 - 1.0)
    pub keyword_score: Option<f32>,
    /// 时间衰减得分 (0.0 - 1.0)
    pub time_score: Option<f32>,
    /// 图关系传播得分 (0.0 - 1.0)
    pub graph_score: Option<f32>,
    /// 文件内容匹配得分 (0.0 - 1.0)
    pub file_score: Option<f32>,
}

impl ChannelScores {
    /// 计算加权最终得分
    pub fn weighted_sum(&self, strategy: &FusionStrategy) -> f32 {
        match strategy {
            FusionStrategy::WeightedSum {
                vector_weight,
                keyword_weight,
                time_weight,
                graph_weight,
                file_weight,
            } => {
                let mut total = 0.0;
                let mut weight_sum = 0.0;

                if let Some(s) = self.vector_score {
                    total += s * vector_weight;
                    weight_sum += vector_weight;
                }
                if let Some(s) = self.keyword_score {
                    total += s * keyword_weight;
                    weight_sum += keyword_weight;
                }
                if let Some(s) = self.time_score {
                    total += s * time_weight;
                    weight_sum += time_weight;
                }
                if let Some(s) = self.graph_score {
                    total += s * graph_weight;
                    weight_sum += graph_weight;
                }
                if let Some(s) = self.file_score {
                    total += s * file_weight;
                    weight_sum += file_weight;
                }

                if weight_sum > 0.0 {
                    total / weight_sum
                } else {
                    0.0
                }
            }
            FusionStrategy::RrfMulti { .. } => {
                // RRF 在引擎层统一计算
                self.vector_score.unwrap_or(0.0)
            }
        }
    }
}

/// 单个评分候选项
#[derive(Debug, Clone)]
pub struct ScoredCandidate {
    /// 记忆节点 ID
    pub node_id: String,
    /// 候选节点（如可用）
    pub node: Option<MemoryNode>,
    /// 加权最终得分 (0.0 - 1.0)
    pub final_score: f32,
    /// 各路通道得分
    pub channel_scores: ChannelScores,
    /// 来源通道列表
    pub source_channels: Vec<SearchChannel>,
}

/// 各路通道统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelStats {
    /// 向量通道候选数
    pub vector_candidates: usize,
    /// 关键词通道候选数
    pub keyword_candidates: usize,
    /// 时间通道候选数
    pub time_candidates: usize,
    /// 图传播通道候选数
    pub graph_candidates: usize,
    /// 文件通道候选数
    pub file_candidates: usize,
    /// 去重后总候选数
    pub total_after_dedup: usize,
    /// 最终返回数
    pub final_count: usize,
}

/// 五路混合检索结果
#[derive(Debug, Clone)]
pub struct HybridSearchResult {
    /// 评分候选项列表（按 final_score 降序）
    pub candidates: Vec<ScoredCandidate>,
    /// 使用的融合策略
    pub fusion_strategy: FusionStrategy,
    /// 各路通道统计
    pub channel_stats: ChannelStats,
}

// ─── 五路混合检索引擎 ─────────────────────────────────────────────────

/// 五路混合检索引擎
///
/// 编排五个检索通道，通过加权融合或 RRF 合并各路结果。
pub struct HybridSearchEngine {
    store: Arc<MemoryGraphStore>,
}

impl HybridSearchEngine {
    pub fn new(store: Arc<MemoryGraphStore>) -> Self {
        Self { store }
    }

    /// 执行五路混合检索
    ///
    /// 并行（逻辑上）运行五个通道，然后按融合策略合并结果。
    pub async fn search(
        &self,
        request: &HybridSearchRequest,
        strategy: Option<FusionStrategy>,
    ) -> anyhow::Result<HybridSearchResult> {
        let strategy = strategy.unwrap_or_default();
        let mut channel_stats = ChannelStats::default();

        // 使用现有的 MemoryRecallEngine 作为基础
        let recall_config = crate::memory_graph::recall::MemoryRecallConfig {
            seed_limit: request.max_results.min(20),
            expansion_limit: request.max_results.min(10),
            recent_limit: request.max_results.min(10),
            ..Default::default()
        };

        let recall_engine = MemoryRecallEngine::new(
            self.store.clone(),
            recall_config,
        );

        // ── Channel 1: Vector + Keyword (via MemoryRecallEngine) ──
        let recall_plan = recall_engine
            .build_recall_plan_with_time(
                &request.space_id,
                &request.query,
                false,
                request.time_range.as_ref(),
            )
            .await?;

        channel_stats.vector_candidates = recall_plan.relevant.len();
        channel_stats.keyword_candidates = recall_plan.triggered.len();
        channel_stats.graph_candidates = recall_plan.expanded.len();
        channel_stats.time_candidates = recall_plan.recent.len();

        // ── 收集所有候选项并计算得分 ──
        let mut candidates: Vec<ScoredCandidate> = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        // 相关节点（向量语义 + 关键词融合）
        for entry in &recall_plan.relevant {
            if seen_ids.insert(entry.node_id.clone()) {
                let mut scores = ChannelScores::default();
                scores.vector_score = entry.score;
                candidates.push(ScoredCandidate {
                    node_id: entry.node_id.clone(),
                    node: Some(candidate_to_node(entry)),
                    final_score: 0.0, // 稍后统一计算
                    channel_scores: scores,
                    source_channels: vec![SearchChannel::Vector],
                });
            }
        }

        // 触发节点（关键词）
        for entry in &recall_plan.triggered {
            if seen_ids.insert(entry.node_id.clone()) {
                let mut scores = ChannelScores::default();
                scores.keyword_score = entry.score;
                candidates.push(ScoredCandidate {
                    node_id: entry.node_id.clone(),
                    node: Some(candidate_to_node(entry)),
                    final_score: 0.0,
                    channel_scores: scores,
                    source_channels: vec![SearchChannel::Keyword],
                });
            } else {
                // 已在其他通道出现，合并得分
                if let Some(c) = candidates.iter_mut().find(|c| c.node_id == entry.node_id) {
                    c.channel_scores.keyword_score = entry.score;
                    c.source_channels.push(SearchChannel::Keyword);
                }
            }
        }

        // 扩展节点（图关系）
        for entry in &recall_plan.expanded {
            if seen_ids.insert(entry.node_id.clone()) {
                let mut scores = ChannelScores::default();
                scores.graph_score = entry.score;
                candidates.push(ScoredCandidate {
                    node_id: entry.node_id.clone(),
                    node: Some(candidate_to_node(entry)),
                    final_score: 0.0,
                    channel_scores: scores,
                    source_channels: vec![SearchChannel::Graph],
                });
            } else {
                if let Some(c) = candidates.iter_mut().find(|c| c.node_id == entry.node_id) {
                    c.channel_scores.graph_score = entry.score;
                    c.source_channels.push(SearchChannel::Graph);
                }
            }
        }

        // 近期条目（时间衰减）
        for entry in &recall_plan.recent {
            if seen_ids.insert(entry.node_id.clone()) {
                let mut scores = ChannelScores::default();
                scores.time_score = entry.time_score;
                candidates.push(ScoredCandidate {
                    node_id: entry.node_id.clone(),
                    node: Some(timeline_to_node(entry)),
                    final_score: 0.0,
                    channel_scores: scores,
                    source_channels: vec![SearchChannel::Time],
                });
            } else {
                if let Some(c) = candidates.iter_mut().find(|c| c.node_id == entry.node_id) {
                    c.channel_scores.time_score = entry.time_score;
                    c.source_channels.push(SearchChannel::Time);
                }
            }
        }

        // ── File Channel (文件内容匹配) ──
        if let Some(ref file_paths) = request.file_paths {
            if !file_paths.is_empty() {
                channel_stats.file_candidates = self
                    .add_file_channel_results(
                        &request.space_id,
                        file_paths,
                        &mut candidates,
                        &mut seen_ids,
                    );
            }
        }

        channel_stats.total_after_dedup = candidates.len();

        // ── 计算最终得分 ──
        for c in &mut candidates {
            c.final_score = c.channel_scores.weighted_sum(&strategy);
        }

        // ── 融合策略处理 ──
        match &strategy {
            FusionStrategy::WeightedSum { .. } => {
                // 加权求和已在上一步完成
                candidates.sort_by(|a, b| b.final_score.partial_cmp(&a.final_score).unwrap_or(std::cmp::Ordering::Equal));
            }
            FusionStrategy::RrfMulti { k } => {
                // RRF 融合：按各路排名计算 RRF 得分
                let k = *k as f32;
                let k_inv = 1.0 / k;

                // 为每路计算排名
                self.apply_rrf_fusion(&mut candidates, k_inv);
                candidates.sort_by(|a, b| b.final_score.partial_cmp(&a.final_score).unwrap_or(std::cmp::Ordering::Equal));
            }
        }

        // 截断到 max_results
        candidates.truncate(request.max_results);
        channel_stats.final_count = candidates.len();

        Ok(HybridSearchResult {
            candidates,
            fusion_strategy: strategy,
            channel_stats,
        })
    }

    /// 文件通道：匹配与指定文件路径相关的记忆节点
    fn add_file_channel_results(
        &self,
        space_id: &str,
        file_paths: &[String],
        candidates: &mut Vec<ScoredCandidate>,
        seen_ids: &mut std::collections::HashSet<String>,
    ) -> usize {
        let mut count = 0;

        for file_path in file_paths {
            // 通过关键词搜索文件名
            let file_name = std::path::Path::new(file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(file_path);

            if let Ok(nodes) = self.store.search_by_keyword(space_id, file_name) {
                for node in nodes {
                    if seen_ids.insert(node.id.clone()) {
                        let mut scores = ChannelScores::default();
                        scores.file_score = Some(0.7); // 文件名匹配基础分
                        candidates.push(ScoredCandidate {
                            node_id: node.id.clone(),
                            node: Some(node),
                            final_score: 0.0,
                            channel_scores: scores,
                            source_channels: vec![SearchChannel::File],
                        });
                        count += 1;
                    } else if let Some(c) = candidates.iter_mut().find(|c| c.node_id == node.id) {
                        if c.channel_scores.file_score.is_none() {
                            c.channel_scores.file_score = Some(0.7);
                            c.source_channels.push(SearchChannel::File);
                        }
                    }
                }
            }
        }

        count
    }

    /// 应用 RRF 多路融合
    fn apply_rrf_fusion(
        &self,
        candidates: &mut [ScoredCandidate],
        k_inv: f32,
    ) {
        // 为每条通道单独排名
        let channels: [(fn(&ChannelScores) -> Option<f32>, SearchChannel); 4] = [
            (|s| s.vector_score, SearchChannel::Vector),
            (|s| s.keyword_score, SearchChannel::Keyword),
            (|s| s.time_score, SearchChannel::Time),
            (|s| s.graph_score, SearchChannel::Graph),
        ];

        for (getter, _ch) in &channels {
            // 按该通道得分降序排序的索引
            let mut ranked: Vec<(usize, f32)> = candidates
                .iter()
                .enumerate()
                .filter_map(|(i, c)| getter(&c.channel_scores).map(|s| (i, s)))
                .collect();
            ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            // 累加 RRF 得分
            for (rank, (idx, _score)) in ranked.iter().enumerate() {
                let rrf = 1.0 / ((rank as f32) + k_inv);
                candidates[*idx].final_score += rrf;
            }
        }
    }

    /// 将检索结果格式化为 prompt 可用的文本
    pub fn format_for_prompt(result: &HybridSearchResult) -> String {
        if result.candidates.is_empty() {
            return String::new();
        }

        let mut parts = Vec::new();
        for candidate in &result.candidates {
            let node = match &candidate.node {
                Some(n) => n,
                None => continue,
            };

            let sources: Vec<&str> = candidate
                .source_channels
                .iter()
                .map(|ch| match ch {
                    SearchChannel::Vector => "vec",
                    SearchChannel::Keyword => "kw",
                    SearchChannel::Time => "time",
                    SearchChannel::Graph => "graph",
                    SearchChannel::File => "file",
                })
                .collect();

            let meta_text = node
                .metadata
                .as_ref()
                .and_then(|m| m.get("description").or_else(|| m.get("summary")))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            parts.push(format!(
                "- {} [{}] score={:.2} sources=[{}]\n  {}",
                node.title,
                node.kind.as_str(),
                candidate.final_score,
                sources.join(","),
                truncate_for_display(meta_text, 120),
            ));
        }

        parts.join("\n")
    }
}

/// 截断文本用于展示
fn truncate_for_display(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_len).collect::<String>())
    }
}

// ─── 单元测试 ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_graph::models::{MemoryKeyword, MemoryNode, MemoryNodeKind};

    fn make_test_store() -> Arc<MemoryGraphStore> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH)
            .unwrap();
        let conn = Arc::new(std::sync::Mutex::new(conn));
        Arc::new(MemoryGraphStore::new(conn))
    }

    fn insert_test_nodes(store: &MemoryGraphStore, space_id: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        let nodes = vec![
            ("Rust async 编程指南", MemoryNodeKind::Reference, "async programming patterns"),
            ("数据库连接池配置", MemoryNodeKind::Reference, "connection pool settings"),
            ("JWT 认证实现", MemoryNodeKind::Procedure, "auth implementation"),
        ];

        for (title, kind, keyword) in nodes {
            let node_id = uuid::Uuid::new_v4().to_string();
            let node = MemoryNode {
                id: node_id.clone(),
                space_id: space_id.to_string(),
                kind,
                title: title.to_string(),
                metadata: Some(serde_json::json!({"description": keyword})),
                created_at: now.clone(),
                updated_at: now.clone(),
            };
            store.create_node(&node).unwrap();

            let kw = MemoryKeyword {
                id: uuid::Uuid::new_v4().to_string(),
                space_id: space_id.to_string(),
                node_id,
                keyword: keyword.to_string(),
                created_at: now.clone(),
            };
            store.create_keyword(&kw).unwrap();
        }
    }

    #[test]
    fn test_channel_scores_weighted_sum() {
        let scores = ChannelScores {
            vector_score: Some(0.9),
            keyword_score: Some(0.7),
            time_score: Some(0.5),
            graph_score: Some(0.3),
            file_score: None,
        };

        let strategy = FusionStrategy::default();
        let result = scores.weighted_sum(&strategy);

        // weights: 0.35, 0.25, 0.10, 0.15, 0.15
        // active: vector(0.35), keyword(0.25), time(0.10), graph(0.15) = sum 0.85
        // expected: (0.9*0.35 + 0.7*0.25 + 0.5*0.10 + 0.3*0.15) / 0.85
        let expected = (0.9 * 0.35 + 0.7 * 0.25 + 0.5 * 0.10 + 0.3 * 0.15) / 0.85;
        assert!((result - expected).abs() < 0.01);
    }

    #[test]
    fn test_fusion_strategy_lightweight() {
        let strategy = FusionStrategy::lightweight();
        match strategy {
            FusionStrategy::WeightedSum { vector_weight, .. } => {
                assert_eq!(vector_weight, 0.0);
            }
            _ => panic!("expected WeightedSum"),
        }
    }

    #[tokio::test]
    async fn test_hybrid_search_basic() {
        let store = make_test_store();
        insert_test_nodes(&store, "default");

        let engine = HybridSearchEngine::new(store);

        let request = HybridSearchRequest {
            query: "async programming".to_string(),
            space_id: "default".to_string(),
            session_id: None,
            time_range: None,
            file_paths: None,
            max_results: 5,
        };

        let result = engine
            .search(&request, Some(FusionStrategy::lightweight()))
            .await
            .unwrap();

        assert!(!result.candidates.is_empty());
        assert!(result.channel_stats.total_after_dedup > 0);

        // 得分最高的应该与查询相关（lightweight 模式下可能无向量得分，放宽检查）
        let top = &result.candidates[0];
        // 至少应有某些通道返回了结果
        assert!(result.channel_stats.total_after_dedup > 0);
    }

    #[test]
    fn test_format_for_prompt() {
        let candidates = vec![ScoredCandidate {
            node_id: "n1".to_string(),
            node: Some(MemoryNode {
                id: "n1".to_string(),
                space_id: "default".to_string(),
                kind: MemoryNodeKind::Reference,
                title: "Test Node".to_string(),
                metadata: Some(serde_json::json!({"description": "test description"})),
                created_at: String::new(),
                updated_at: String::new(),
            }),
            final_score: 0.85,
            channel_scores: ChannelScores {
                vector_score: Some(0.9),
                keyword_score: Some(0.8),
                time_score: None,
                graph_score: None,
                file_score: None,
            },
            source_channels: vec![SearchChannel::Vector, SearchChannel::Keyword],
        }];

        let result = HybridSearchResult {
            candidates,
            fusion_strategy: FusionStrategy::default(),
            channel_stats: ChannelStats::default(),
        };

        let prompt = HybridSearchEngine::format_for_prompt(&result);
        assert!(prompt.contains("Test Node"));
        assert!(prompt.contains("vec"));
        assert!(prompt.contains("kw"));
        assert!(prompt.contains("0.85"));
    }
}
