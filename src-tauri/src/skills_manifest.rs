//! Build a Markdown manifest block describing the agent's available skills,
//! unioning builtin skills from `SkillsRegistry` and learned skills from
//! `MemoryGraphStore`. Injected into the system prompt by
//! `ChatDelegate::effective_system_prompt`.
//!
//! See docs/superpowers/specs/2026-05-12-skill-recall-design.md §1.

use crate::browser::playwright_skills::{
    classify_playwright_skill, is_managed_playwright_skill_path,
    PlaywrightSkillCompatibilityStatus, PlaywrightSkillManifest,
};
use crate::memory_graph::store::MemoryGraphStore;
use crate::skills::{LoadedSkill, SkillsRegistry};
use std::hash::{Hash, Hasher};

const APPROX_TOKENS_PER_CHAR: f64 = 0.25;

/// Token budget for the skills manifest injected into the agent/chat system
/// prompt. `build_skills_manifest` truncates each summary (100 chars) and stops
/// adding entries once this budget is hit, surfacing the highest-ranked skills
/// compactly; every other skill stays reachable through the `skill_search` /
/// `load_skill` tools (the manifest footer teaches that flow).
///
/// Replaces the previously-uncapped `SkillsRegistry::format_for_system_prompt_xml`
/// path, which rendered ALL skills with full descriptions + absolute `<location>`
/// paths (~34KB / ~9–10K tokens for ~100 skills, re-sent every turn until the
/// agent first calls `skill_search`). Tune this down to trade manifest coverage
/// for fewer per-turn tokens.
pub const SYSTEM_PROMPT_MANIFEST_MAX_TOKENS: usize = 1500;

/// Hard cap on manifest entry count — a safety bound independent of the token
/// budget above. Set high enough that the token budget is the effective limit.
pub const SYSTEM_PROMPT_MANIFEST_MAX_ENTRIES: usize = 200;

/// Manifest re-ranking bias: learned skills whose `metadata.category` matches
/// the active strategy receive +3.0 score bonus and float to the top of the
/// learned block (stable sort preserves E3 order within tied score groups).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum StrategyBias {
    /// No category bias — default E3 ordering.
    #[default]
    Balanced,
    /// Bias toward skills tagged `category: "repair"`.
    Repair,
    /// Bias toward skills tagged `category: "optimize"`.
    Optimize,
    /// Bias toward skills tagged `category: "innovate"`.
    Innovate,
}

/// Build the manifest block to append to the system prompt.
///
/// Returns an empty String when no skills exist in either source — caller
/// is responsible for not appending separator markers around an empty body.
///
/// `bias` biases the learned-skill ranking: skills whose `metadata.category`
/// matches the active strategy receive +3.0 score and are sorted to the top
/// of the learned block (stable sort, so E3 order is preserved within ties).
/// A "Current mode: X" annotation is prepended to the header when bias != Balanced.
pub fn build_skills_manifest(
    registry: &SkillsRegistry,
    store: &MemoryGraphStore,
    space_id: &str,
    max_entries: usize,
    max_tokens: usize,
    bias: StrategyBias,
    workspace_tags: Option<&[String]>,
    query: Option<&str>,
) -> String {
    // Lexical relevance terms from the current user message: learned skills whose
    // title/summary overlap the query float to the top of the budget-capped
    // manifest (recency stays the tiebreaker). Makes the injected skills
    // *task-relevant* instead of just *recently used* — the model sees the skills
    // for THIS turn. Empty query ⇒ pure recency/bias order (unchanged behavior).
    let query_terms = query.map(extract_query_terms).unwrap_or_default();
    let entries = collect_entries(
        registry,
        store,
        space_id,
        max_entries,
        &bias,
        workspace_tags,
        &query_terms,
    );
    if entries.is_empty() {
        return String::new();
    }

    // Best-effort: bump manifest_appearance_count for learned skills that
    // survived ranking + budget and will appear in the system prompt.
    let learned_ids: Vec<&str> = entries
        .iter()
        .filter_map(|e| e.node_id.as_deref())
        .collect();
    if !learned_ids.is_empty() {
        store.bump_manifest_appearance(&learned_ids);
    }

    format_manifest(&entries, max_tokens, &bias)
}

/// Public mirror of `build_skills_manifest` that returns the structured
/// entries (rather than the formatted prompt string) so the Settings UI's
/// "active manifest" debug panel can show users exactly what's being
/// injected. Same inputs, same selection logic, same ranking — only the
/// output shape differs.
pub fn compute_active_manifest_entries(
    registry: &SkillsRegistry,
    store: &MemoryGraphStore,
    space_id: &str,
    max_entries: usize,
    bias: StrategyBias,
    workspace_tags: Option<&[String]>,
) -> Vec<ManifestEntry> {
    // Debug panel shows the recency/bias-ordered manifest (no live query context).
    collect_entries(registry, store, space_id, max_entries, &bias, workspace_tags, &[])
}

/// Manifest filter rule for per-workspace skill tag scoping (V19+).
///
/// Returns true iff the skill should be included given the workspace's
/// tag set. The semantic is **"opt-in scoping with global fallback"**:
///
///   - workspace has no tags (None or empty) → always include (default)
///   - workspace has tags AND skill has no tags → include (untagged =
///     global, like a fresh-extracted learned skill before tagging)
///   - workspace has tags AND skill has tags → include iff intersection
///
/// Tag comparison is case-sensitive — normalization happened at write
/// time via `normalize_skill_tags`.
pub fn skill_matches_workspace(
    skill_tags: &[String],
    workspace_tags: Option<&[String]>,
) -> bool {
    let Some(ws) = workspace_tags else { return true };
    if ws.is_empty() { return true; }
    if skill_tags.is_empty() { return true; }
    skill_tags.iter().any(|t| ws.iter().any(|w| w == t))
}

/// Single row in the manifest. Public for the debug panel IPC.
///
/// `provenance` is a static string: `"builtin"` for static/borrowed skills
/// or `"learned"` for graph-stored ones. The debug-panel IPC layer
/// enriches `"builtin"` to the real tier (`"bundled"`/`"user"`/`"project"`)
/// using the registry's LoadedSkill data so users see the actual
/// provenance, not the lumped category.
#[derive(Debug, Clone)]
pub struct ManifestEntry {
    pub name: String,
    pub summary: String,
    pub provenance: &'static str,
    pub cited_count: u64,
    /// Node ID for learned skills (None for builtins). Used by
    /// `bump_manifest_appearance` to record the inclusion signal.
    pub node_id: Option<String>,
}

/// Whether `c` is a CJK ideograph (so Chinese queries get bigram terms instead
/// of relying on whitespace, which Chinese text lacks).
fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

fn push_ascii_word(word: &mut String, out: &mut std::collections::BTreeSet<String>) {
    if word.chars().count() >= 2 {
        out.insert(std::mem::take(word));
    } else {
        word.clear();
    }
}

fn push_cjk_run(run: &mut Vec<char>, out: &mut std::collections::BTreeSet<String>) {
    if run.len() >= 2 {
        // The whole short run (e.g. 清蒸桂鱼) + its 2-grams (清蒸, 蒸桂, 桂鱼),
        // so a query overlaps a skill title even with different segmentation.
        if run.len() <= 4 {
            out.insert(run.iter().collect());
        }
        for w in run.windows(2) {
            out.insert(w.iter().collect());
        }
    }
    run.clear();
}

/// Tokenize a user query into lowercase relevance terms: ASCII alphanumeric words
/// (≥2 chars) + CJK bigrams from contiguous Chinese runs. Deduped. Used to
/// lexically rank learned skills against the current turn. Pure + unit-tested.
fn extract_query_terms(query: &str) -> Vec<String> {
    let lower = query.to_lowercase();
    let mut terms: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut word = String::new();
    let mut cjk: Vec<char> = Vec::new();
    for c in lower.chars() {
        if is_cjk(c) {
            push_ascii_word(&mut word, &mut terms);
            cjk.push(c);
        } else if c.is_ascii_alphanumeric() {
            push_cjk_run(&mut cjk, &mut terms);
            word.push(c);
        } else {
            push_ascii_word(&mut word, &mut terms);
            push_cjk_run(&mut cjk, &mut terms);
        }
    }
    push_ascii_word(&mut word, &mut terms);
    push_cjk_run(&mut cjk, &mut terms);
    terms.into_iter().collect()
}

fn collect_entries(
    registry: &SkillsRegistry,
    store: &MemoryGraphStore,
    space_id: &str,
    max_entries: usize,
    bias: &StrategyBias,
    workspace_tags: Option<&[String]>,
    query_terms: &[String],
) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();

    // Builtin: alphabetical (list_enabled is already alpha-sorted),
    // honors runtime disable via SkillsRegistry::disable.
    // V19+: workspace tag filter applied via skill_matches_workspace().
    // Skills with no activation.tags are global (included regardless).
    let mut builtin: Vec<_> = registry
        .all_loaded_skills()
        .into_iter()
        .filter(|skill| registry.is_enabled(&skill.manifest.name))
        .collect();
    builtin.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    for loaded in builtin {
        if let Some(playwright_skill) = managed_playwright_skill_manifest(loaded) {
            let report = classify_playwright_skill(&playwright_skill);
            if report.status != PlaywrightSkillCompatibilityStatus::Enabled {
                tracing::warn!(
                    skill = %report.name,
                    reason = ?report.reason,
                    "Skipping unavailable managed Playwright skill in agent manifest"
                );
                continue;
            }
        }
        let info = &loaded.manifest;
        if !skill_matches_workspace(&info.activation.tags, workspace_tags) {
            continue;
        }
        entries.push(ManifestEntry {
            name: info.name.clone(),
            summary: truncate_summary(&info.description, 100),
            provenance: "builtin",
            cited_count: 0,
            node_id: None,
        });
        if entries.len() >= max_entries {
            return entries;
        }
    }

    // Learned: E3 ranking from list_promoted_learned_skills (cited DESC, usage DESC, updated DESC),
    // then re-ranked by category bonus when bias != Balanced.
    // Fetch a larger window (max_entries * 2, capped at 200) so bias re-ranking has
    // candidates to promote even when the top E3 results aren't category-matched.
    //
    // PR-mattpocock-3: drafts are deliberately excluded here — they're still
    // reachable via skill_search and slash commands, but only `promoted`
    // skills get auto-injected into the system prompt.
    let learned_limit = max_entries.saturating_sub(entries.len());
    if learned_limit == 0 {
        return entries;
    }
    let fetch_limit = if *bias == StrategyBias::Balanced {
        learned_limit
    } else {
        (learned_limit * 2).min(200)
    };
    let learned = store
        .list_promoted_learned_skills(space_id, fetch_limit)
        .unwrap_or_else(|e| {
            tracing::warn!("skills_manifest: failed to load learned skills: {}", e);
            Vec::new()
        });

    // Collect candidates with their category, then optionally re-rank.
    let bias_category = match bias {
        StrategyBias::Balanced  => None,
        StrategyBias::Repair    => Some("repair"),
        StrategyBias::Optimize  => Some("optimize"),
        StrategyBias::Innovate  => Some("innovate"),
    };

    struct Candidate {
        entry: ManifestEntry,
        bonus: f32,
    }
    let mut candidates: Vec<Candidate> = learned.into_iter().filter_map(|detail| {
        // V19+ workspace tag filter for learned skills. Tags live in
        // `metadata.tags` (JSON array of strings) when populated, or
        // are missing entirely on legacy/untagged skills. Missing or
        // empty → global (always included if the workspace has no
        // filter or the skill is untagged).
        let skill_tags: Vec<String> = detail
            .node
            .metadata
            .as_ref()
            .and_then(|m| m.get("tags"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if !skill_matches_workspace(&skill_tags, workspace_tags) {
            return None;
        }

        let summary = detail
            .node
            .metadata
            .as_ref()
            .and_then(|m| m.get("summary"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| detail.node.title.clone());
        let cited_count = detail
            .node
            .metadata
            .as_ref()
            .and_then(|m| m.get("cited_count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let category = detail
            .node
            .metadata
            .as_ref()
            .and_then(|m| m.get("category"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let bias_bonus = if let (Some(bias_cat), Some(ref cat)) = (bias_category, &category) {
            if cat.as_str() == bias_cat { 3.0_f32 } else { 0.0 }
        } else {
            0.0
        };
        // Lexical relevance to the current query: each distinct matched term in
        // title+summary adds 1.5 (capped at 6.0 ≈ 4 terms). A 2-term match (3.0)
        // matches a category bias, so a task-relevant skill floats above a merely
        // recent one. Zero when there's no query (manifest stays recency-ordered).
        let relevance_bonus = if query_terms.is_empty() {
            0.0
        } else {
            let hay = format!("{} {}", detail.node.title, summary).to_lowercase();
            let matched = query_terms.iter().filter(|t| hay.contains(t.as_str())).count();
            (matched as f32 * 1.5).min(6.0)
        };
        Some(Candidate {
            entry: ManifestEntry {
                name: detail.node.title.clone(),
                summary: truncate_summary(&summary, 100),
                provenance: "learned",
                cited_count,
                node_id: Some(detail.node.id.clone()),
            },
            bonus: bias_bonus + relevance_bonus,
        })
    }).collect();

    // Stable sort by combined bonus descending (recency/E3 order preserved within
    // ties). Sort whenever there's a re-ranking signal — a strategy bias OR query
    // relevance; a Balanced bias with no query keeps the original recency order.
    let has_relevance = !query_terms.is_empty();
    if *bias != StrategyBias::Balanced || has_relevance {
        candidates.sort_by(|a, b| b.bonus.partial_cmp(&a.bonus).unwrap_or(std::cmp::Ordering::Equal));
    }

    for cand in candidates.into_iter().take(learned_limit) {
        entries.push(cand.entry);
        if entries.len() >= max_entries {
            break;
        }
    }

    entries
}

fn managed_playwright_skill_manifest(skill: &LoadedSkill) -> Option<PlaywrightSkillManifest> {
    if !is_managed_playwright_skill_path(&skill.manifest.path) {
        return None;
    }

    let mut capabilities = Vec::new();
    if skill.manifest.tools.iter().any(|tool| tool.command.is_some()) {
        capabilities.push("raw_shell".to_string());
    }
    for tag in &skill.manifest.activation.tags {
        if matches!(
            tag.as_str(),
            "navigate" | "click" | "type" | "snapshot" | "screenshot" | "trace" | "raw_shell"
        ) && !capabilities.iter().any(|existing| existing == tag)
        {
            capabilities.push(tag.clone());
        }
    }
    if capabilities.is_empty() {
        capabilities.push(infer_playwright_capability(&skill.manifest.name).to_string());
    }

    Some(PlaywrightSkillManifest {
        name: skill.manifest.name.clone(),
        source_version: skill.manifest.version.clone(),
        required_capabilities: capabilities,
        hash: prompt_hash(&skill.prompt_content),
    })
}

fn prompt_hash(content: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn infer_playwright_capability(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.contains("click") {
        "click"
    } else if lower.contains("type") || lower.contains("fill") {
        "type"
    } else if lower.contains("snapshot") || lower.contains("accessibility") {
        "snapshot"
    } else if lower.contains("screenshot") {
        "screenshot"
    } else if lower.contains("trace") {
        "trace"
    } else {
        "navigate"
    }
}

fn format_manifest(entries: &[ManifestEntry], max_tokens: usize, bias: &StrategyBias) -> String {
    let mode_hint = match bias {
        StrategyBias::Balanced  => String::new(),
        StrategyBias::Repair    => "Current mode: 修 bug — repair-tagged skills ranked first.\n".to_string(),
        StrategyBias::Optimize  => "Current mode: 优化 — optimize-tagged skills ranked first.\n".to_string(),
        StrategyBias::Innovate  => "Current mode: 探索 — innovate-tagged skills ranked first.\n".to_string(),
    };
    let header = format!(
        "\n\n---\n\n## 你已学习到的技能 (Learned Skills)\n\n{}下面是你过往会话中沉淀的技能清单。当遇到相关问题时，先用 `skill_search` 查询，再用 `load_skill` 加载完整内容。**不强制使用**——只有当技能确实匹配当前任务时再调用。\n\n",
        mode_hint,
    );
    let footer = "\n\n**使用流程**：\n1. `skill_search(query: \"...\", top_k: 3)` → 看摘要决定\n2. `load_skill(name: \"...\", reason: \"...\")` → 拿完整指引\n3. 应用后，在回复末尾用 `> 应用技能：name — 简短原因` 标注（供未来检索）\n\n---\n";

    let mut body = String::new();
    let budget_chars = (max_tokens as f64 / APPROX_TOKENS_PER_CHAR) as usize;
    let header_footer_chars = header.len() + footer.len();
    let mut remaining = budget_chars.saturating_sub(header_footer_chars);

    for entry in entries {
        let cite = if entry.cited_count > 0 {
            format!(" · cited {}", entry.cited_count)
        } else {
            String::new()
        };
        let line = format!(
            "- **{}** [{}{}] — {}\n",
            entry.name, entry.provenance, cite, entry.summary
        );
        if line.len() > remaining {
            break;
        }
        remaining -= line.len();
        body.push_str(&line);
    }

    if body.is_empty() {
        return String::new();
    }

    format!("{}{}{}", header, body, footer)
}

fn truncate_summary(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        // Trim at last word boundary to avoid mid-word cut
        if let Some(last_space) = out.rfind(char::is_whitespace) {
            out.truncate(last_space);
        }
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Pure filter helper: covers all four branches of the rule ────────

    #[test]
    fn skill_matches_workspace_no_filter_when_workspace_tags_missing() {
        // None == "this workspace was loaded with no filter info" — must
        // never exclude anything (graceful degradation).
        assert!(skill_matches_workspace(&[], None));
        assert!(skill_matches_workspace(&["engineering".into()], None));
    }

    #[test]
    fn skill_matches_workspace_no_filter_when_workspace_tags_empty() {
        // Empty == "user explicitly opted out of scoping" — same effect
        // as None: include everything.
        let empty: &[String] = &[];
        assert!(skill_matches_workspace(&[], Some(empty)));
        assert!(skill_matches_workspace(&["engineering".into()], Some(empty)));
    }

    #[test]
    fn skill_matches_workspace_untagged_skill_is_global() {
        // The "untagged = global" rule: a skill with no tags appears in
        // every workspace, even tag-scoped ones. Protects fresh-extracted
        // learned skills from being silently hidden.
        let ws = ["engineering".to_string()];
        assert!(skill_matches_workspace(&[], Some(&ws)));
    }

    #[test]
    fn skill_matches_workspace_intersection_required() {
        let ws = ["engineering".to_string(), "process".to_string()];

        // Skill `["engineering", "tdd"]` matches via `engineering`.
        let skill1 = ["engineering".to_string(), "tdd".to_string()];
        assert!(skill_matches_workspace(&skill1, Some(&ws)));

        // Skill `["research"]` has no overlap — excluded.
        let skill2 = ["research".to_string()];
        assert!(!skill_matches_workspace(&skill2, Some(&ws)));
    }

    #[test]
    fn skill_matches_workspace_case_sensitive() {
        // Normalization happens at write time. The filter does bare
        // comparison so we don't pay per-call lowercase costs.
        let ws = ["engineering".to_string()];
        let skill_upper = ["Engineering".to_string()];
        assert!(!skill_matches_workspace(&skill_upper, Some(&ws)),
            "filter is case-sensitive — caller must normalize before write");
    }

    // ─── Existing tests below ────────────────────────────────────────────

    #[test]
    fn empty_registry_and_store_returns_empty_string() {
        // Will need a way to construct an empty MemoryGraphStore — use the
        // in-memory SQLite helper from memory_graph tests.
        let conn = std::sync::Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().unwrap(),
        ));
        // Run minimal V4 migration so memory_nodes table exists.
        let _ = conn.lock().unwrap().execute_batch(
            crate::db::migrations::V4_MEMORY_GRAPH,
        );
        let store = MemoryGraphStore::new(conn);
        let registry = SkillsRegistry::new();

        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500, StrategyBias::Balanced, None, None);
        assert!(manifest.is_empty(), "expected empty manifest, got: {}", manifest);
    }

    use crate::memory_graph::models::{MemoryNode, MemoryNodeKind, MemoryVersion, MemoryVersionStatus};
    use chrono::Utc;
    use serde_json::json;

    fn fresh_store() -> std::sync::Arc<MemoryGraphStore> {
        let conn = std::sync::Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().unwrap(),
        ));
        let _ = conn.lock().unwrap().execute_batch(crate::db::migrations::V4_MEMORY_GRAPH);
        std::sync::Arc::new(MemoryGraphStore::new(conn))
    }

    fn make_learned_node(
        store: &MemoryGraphStore,
        title: &str,
        summary: &str,
        cited_count: u64,
        usage_count: u64,
    ) {
        let now = Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        let node = MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Procedure,
            title: title.into(),
            metadata: Some(json!({
                "skill_type": "learned",
                "enabled": true,
                "summary": summary,
                "cited_count": cited_count,
                "usage_count": usage_count,
            })),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.create_node(&node).unwrap();
        let version = MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: id,
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: format!("# {}\n\n{}", title, summary),
            metadata: None,
            embedding_json: None,
            created_at: now,
        };
        store.create_version(&version).unwrap();
    }

    #[test]
    fn extract_query_terms_handles_ascii_and_cjk() {
        // ASCII: words ≥2 chars, lowercased, deduped; 1-char dropped.
        let t = extract_query_terms("Fix the Login API a");
        assert!(t.contains(&"fix".to_string()));
        assert!(t.contains(&"login".to_string()));
        assert!(t.contains(&"api".to_string()));
        assert!(t.contains(&"the".to_string()));
        assert!(!t.iter().any(|s| s == "a"), "1-char token dropped");
        // CJK: contiguous run → 2-grams (+ the whole short run).
        let z = extract_query_terms("帮我做清蒸桂鱼");
        assert!(z.contains(&"清蒸".to_string()));
        assert!(z.contains(&"蒸桂".to_string()));
        assert!(z.contains(&"桂鱼".to_string()));
        // Empty / whitespace → no terms.
        assert!(extract_query_terms("   ").is_empty());
    }

    #[test]
    fn query_relevance_floats_matching_skill_above_more_recent_one() {
        let store = fresh_store();
        // Higher cited/usage ⇒ ranks first WITHOUT a query (recency/decay order).
        make_learned_node(&store, "数据库迁移助手", "生成数据库迁移脚本", 50, 50);
        // Low cited, but matches the query.
        make_learned_node(&store, "清蒸桂鱼做法", "清蒸桂鱼的详细步骤", 1, 1);
        let registry = SkillsRegistry::new();

        // No query → the more-cited migration skill comes first.
        let baseline = build_skills_manifest(
            &registry, &store, "default", 30, 4000, StrategyBias::Balanced, None, None,
        );
        let mig = baseline.find("**数据库迁移助手**").expect("migration skill");
        let fish = baseline.find("**清蒸桂鱼做法**").expect("fish skill");
        assert!(mig < fish, "without a query, higher-cited skill ranks first");

        // With a fish query → the relevant skill floats above the more-cited one.
        let ranked = build_skills_manifest(
            &registry, &store, "default", 30, 4000, StrategyBias::Balanced, None,
            Some("帮我做清蒸桂鱼"),
        );
        let mig2 = ranked.find("**数据库迁移助手**").expect("migration skill");
        let fish2 = ranked.find("**清蒸桂鱼做法**").expect("fish skill");
        assert!(fish2 < mig2, "query-relevant skill should float above the recent one");
    }

    #[test]
    fn learned_only_renders_block_with_cited_count() {
        let store = fresh_store();
        make_learned_node(&store, "stock-research", "Cross-validate stock financials", 7, 12);

        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500, StrategyBias::Balanced, None, None);

        assert!(manifest.contains("## 你已学习到的技能"), "missing header");
        assert!(manifest.contains("**stock-research**"), "missing skill name");
        assert!(manifest.contains("[learned · cited 7]"), "expected cited 7 segment; got:\n{}", manifest);
        assert!(manifest.contains("Cross-validate stock financials"), "missing summary");
    }

    #[test]
    fn zero_cited_omits_cite_segment() {
        let store = fresh_store();
        make_learned_node(&store, "newly-extracted", "Just extracted, never cited yet", 0, 1);

        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500, StrategyBias::Balanced, None, None);

        assert!(manifest.contains("[learned]"), "expected bare [learned] when cited=0; got:\n{}", manifest);
        assert!(!manifest.contains("cited 0"), "must not say 'cited 0'");
    }

    #[test]
    fn max_entries_caps_count() {
        let store = fresh_store();
        for i in 0..5 {
            make_learned_node(&store, &format!("skill-{}", i), "blah", 0, 0);
        }
        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 2, 1500, StrategyBias::Balanced, None, None);

        let lines = manifest.matches("\n- **").count();
        assert_eq!(lines, 2, "expected exactly 2 entries; got:\n{}", manifest);
    }

    #[test]
    fn long_summary_truncates_at_word_boundary() {
        let summary = "Cross-validate stock financials across Yahoo Finance Macrotrends and StockAnalysis with HTTP 403 fallback handling and many more details that should be cut off";
        let store = fresh_store();
        make_learned_node(&store, "long-skill", summary, 0, 0);
        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500, StrategyBias::Balanced, None, None);

        // Should be truncated with "…"
        assert!(manifest.contains("…"), "expected truncation marker; got:\n{}", manifest);
        // Should not contain the tail of the original summary
        assert!(!manifest.contains("cut off"));
    }

    #[test]
    fn token_budget_can_truncate_entries() {
        let store = fresh_store();
        for i in 0..30 {
            make_learned_node(&store, &format!("skill-{}", i), "some summary text", 0, 0);
        }
        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 100, StrategyBias::Balanced, None, None);

        let lines = manifest.matches("\n- **").count();
        assert!(lines < 30, "expected token budget to drop entries; got {} lines", lines);
    }

    #[test]
    fn system_prompt_budget_consts_bound_manifest_size() {
        // With far more skills than the budget can hold, the live agent/chat
        // manifest must stay bounded by SYSTEM_PROMPT_MANIFEST_MAX_TOKENS rather
        // than growing unbounded like the old format_for_system_prompt_xml path.
        let store = fresh_store();
        for i in 0..300 {
            make_learned_node(&store, &format!("skill-{i:03}"), "a representative skill summary", 0, 0);
        }
        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(
            &registry,
            &store,
            "default",
            SYSTEM_PROMPT_MANIFEST_MAX_ENTRIES,
            SYSTEM_PROMPT_MANIFEST_MAX_TOKENS,
            StrategyBias::Balanced,
            None,
            None,
        );

        assert!(!manifest.is_empty(), "manifest should render some entries");
        // budget_chars = max_tokens / APPROX_TOKENS_PER_CHAR, plus header/footer slack.
        let budget_chars = (SYSTEM_PROMPT_MANIFEST_MAX_TOKENS as f64 / APPROX_TOKENS_PER_CHAR) as usize;
        assert!(
            manifest.len() <= budget_chars + 1024,
            "manifest len {} should stay within budget {} (+slack)",
            manifest.len(),
            budget_chars,
        );
        // Footer must teach progressive disclosure so dropped skills remain reachable.
        assert!(manifest.contains("skill_search"), "footer should mention skill_search");
    }

    #[test]
    fn builtin_emitted_before_learned_with_alpha_sort() {
        use crate::skills::{ActivationCriteria, LoadedSkill, SkillManifest};
        use std::path::PathBuf;

        let store = fresh_store();
        // Insert two learned skills with high cited_count — would rank first if there were no builtins
        make_learned_node(&store, "z-learned-skill", "Z summary", 99, 99);
        make_learned_node(&store, "a-learned-skill", "A summary", 99, 99);

        // Build a registry with two builtin skills (note: out-of-alpha order)
        let mut registry = SkillsRegistry::new();
        let mk_skill = |name: &str, description: &str| LoadedSkill {
            manifest: SkillManifest {
                name: name.to_string(),
                version: "1.0.0".to_string(),
                description: description.to_string(),
                author: String::new(),
                enabled: true,
                category: String::new(),
                activation: ActivationCriteria::default(),
                parameters: vec![],
                requires: vec![],
                tools: vec![],
                path: PathBuf::from(format!("/test/{}", name)),
            },
            prompt_content: format!("# {}\n\nbody", name),
            compiled_patterns: vec![],
            lowercased_keywords: vec![],
            lowercased_exclude_keywords: vec![],
            lowercased_tags: vec![],
            provenance: crate::skills::SkillProvenance::Bundled,
        };
        registry.register(mk_skill("z-builtin", "Z builtin"));
        registry.register(mk_skill("a-builtin", "A builtin"));

        let manifest = build_skills_manifest(&registry, &store, "default", 30, 4000, StrategyBias::Balanced, None, None);

        // Find positions of each skill name in the manifest body
        let a_builtin_pos = manifest.find("**a-builtin**").expect("a-builtin missing");
        let z_builtin_pos = manifest.find("**z-builtin**").expect("z-builtin missing");
        let a_learned_pos = manifest.find("**a-learned-skill**").expect("a-learned-skill missing");
        let z_learned_pos = manifest.find("**z-learned-skill**").expect("z-learned-skill missing");

        // Builtins come first (both before any learned)
        assert!(a_builtin_pos < a_learned_pos);
        assert!(z_builtin_pos < a_learned_pos);
        assert!(a_builtin_pos < z_learned_pos);
        assert!(z_builtin_pos < z_learned_pos);

        // Builtins are alphabetical: a-builtin before z-builtin
        assert!(a_builtin_pos < z_builtin_pos, "expected alphabetical builtin order");

        // Runtime disable removes a builtin from the manifest
        registry.disable("a-builtin");
        let manifest2 = build_skills_manifest(&registry, &store, "default", 30, 4000, StrategyBias::Balanced, None, None);
        assert!(
            !manifest2.contains("**a-builtin**"),
            "runtime-disabled skill should not appear; got:\n{}",
            manifest2
        );
        assert!(
            manifest2.contains("**z-builtin**"),
            "non-disabled builtin should still appear"
        );
    }

    #[test]
    fn unavailable_managed_playwright_skill_is_not_injected() {
        use crate::skills::{ActivationCriteria, LoadedSkill, SkillManifest, SkillToolDef};
        use std::path::PathBuf;

        let store = fresh_store();
        let mut registry = SkillsRegistry::new();
        registry.register(LoadedSkill {
            manifest: SkillManifest {
                name: "playwright-raw-shell".to_string(),
                version: "1.0.0".to_string(),
                description: "Raw Playwright shell".to_string(),
                author: String::new(),
                enabled: true,
                category: String::new(),
                activation: ActivationCriteria::default(),
                parameters: vec![],
                requires: vec![],
                tools: vec![SkillToolDef {
                    name: "raw".to_string(),
                    description: "raw shell".to_string(),
                    parameters: serde_json::Value::Null,
                    command: Some("playwright-cli".to_string()),
                }],
                path: PathBuf::from("/tmp/uclaw/builtin-skills/playwright-cli/raw-shell"),
            },
            prompt_content: "raw".to_string(),
            compiled_patterns: vec![],
            lowercased_keywords: vec![],
            lowercased_exclude_keywords: vec![],
            lowercased_tags: vec![],
            provenance: crate::skills::SkillProvenance::Bundled,
        });

        let manifest = build_skills_manifest(
            &registry,
            &store,
            "default",
            30,
            4000,
            StrategyBias::Balanced,
            None,
            None,
        );

        assert!(!manifest.contains("playwright-raw-shell"));
    }

    fn make_learned_with_category(
        store: &MemoryGraphStore,
        title: &str,
        summary: &str,
        cited_count: u64,
        usage_count: u64,
        category: Option<&str>,
    ) {
        let now = Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        let mut meta = serde_json::json!({
            "skill_type": "learned",
            "enabled": true,
            "summary": summary,
            "cited_count": cited_count,
            "usage_count": usage_count,
        });
        if let Some(cat) = category {
            meta["category"] = serde_json::Value::String(cat.to_string());
        }
        let node = MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Procedure,
            title: title.into(),
            metadata: Some(meta),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.create_node(&node).unwrap();
        let version = MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: id,
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: format!("# {}\n\n{}", title, summary),
            metadata: None,
            embedding_json: None,
            created_at: now,
        };
        store.create_version(&version).unwrap();
    }

    #[test]
    fn repair_bias_reorders_learned() {
        let store = fresh_store();
        // Insert a non-repair skill with higher E3 rank (higher cited_count)
        make_learned_with_category(&store, "optimize-skill", "Optimize DB queries", 10, 20, Some("optimize"));
        // Insert a repair skill with lower E3 rank
        make_learned_with_category(&store, "repair-skill", "Fix auth failures", 1, 1, Some("repair"));

        let registry = SkillsRegistry::new();

        // Balanced: optimize-skill (cited=10) comes before repair-skill (cited=1)
        let balanced = build_skills_manifest(&registry, &store, "default", 30, 4000, StrategyBias::Balanced, None, None);
        let opt_pos = balanced.find("**optimize-skill**").expect("optimize-skill missing");
        let rep_pos = balanced.find("**repair-skill**").expect("repair-skill missing");
        assert!(opt_pos < rep_pos, "balanced: higher-cited skill should appear first");

        // Repair bias: repair-skill should float to the top of the learned block
        let biased = build_skills_manifest(&registry, &store, "default", 30, 4000, StrategyBias::Repair, None, None);
        let opt_biased = biased.find("**optimize-skill**").expect("optimize-skill missing in biased");
        let rep_biased = biased.find("**repair-skill**").expect("repair-skill missing in biased");
        assert!(rep_biased < opt_biased, "repair bias: repair-tagged skill must precede unmatched skill");

        // Mode hint should appear in biased manifest
        assert!(biased.contains("Current mode: 修 bug"), "mode hint missing from biased manifest");

        // Balanced manifest should NOT have mode hint
        assert!(!balanced.contains("Current mode:"), "balanced manifest must not have mode hint");
    }
}
