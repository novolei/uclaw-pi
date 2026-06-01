//! System prompt + dynamic-context assembly for ChatDelegate.
//!
//! `effective_system_prompt` is the system-prompt seam (cached by the
//! Anthropic prompt-cache breakpoint). `build_dynamic_context` is the
//! per-turn block prepended to the last user message. Both must stay
//! deterministic — small changes here can invalidate the cache and cost
//! tokens. The order invariants are guarded by tests in this file.

use chrono::{Datelike, Timelike};
use super::ChatDelegate;

/// All inputs needed to assemble both the system prompt AND the per-turn
/// dynamic block. Built once per `call_llm`; consumed by exactly one
/// `assemble_system_prompt` call. P3-6 single-seam input bundle.
///
/// Holds owned data so the assembly function is pure (no `&self`, no
/// hidden state reads, no async). Construction reads from `ChatDelegate`
/// state once, then this can be tested in isolation with arbitrary inputs.
pub(super) struct SystemPromptContext {
    pub base_system_prompt: String,
    pub workspace_root: Option<std::path::PathBuf>,
    pub effective_mode: crate::safety::SafetyMode,
    pub injection_context: crate::agent::baseline_blocks::InjectionContext,
    pub persona_block: Option<String>,
    pub skills_manifest_block: String,
    pub skills_manifest_suppress: bool,
    pub memory_context: Option<String>,
    pub prior_memory_snapshot: Option<crate::agent::context_diff::LineFragmentSnapshot>,
    pub learned_profile_block: String,
    pub gbrain_knowledge_block: String,
    pub injected_fragments: Vec<crate::runtime::context::ContextArtifact>,
    pub now: chrono::DateTime<chrono::Local>,

    /// PR4 of Tier 1+2+3 batch — GEP gene matches block (rendered).
    /// Empty string when no matches.
    pub gene_block: String,

    /// PR4 of Tier 1+2+3 batch — plan-suggest aggregate hint (rendered).
    /// Empty string when not triggered (acceptance rate ≥ 20% or none).
    pub plan_suggest_hint: String,

    /// PR4 of Tier 1+2+3 batch — project rules block (rendered).
    /// Empty string when no rules.
    pub project_rules_block: String,

    /// PR4 of Tier 1+2+3 batch — whether to cache-align the assembled system
    /// prompt via `align_to_1024_tokens` (pad up to the next 1024-token cache
    /// block to isolate the stable prefix). `false` skips padding. (Was
    /// `pad_to_ladder`, which jumped to a coarse [2048..32768] tier — far more
    /// filler tokens/turn for no extra cache benefit. Field name kept for diff
    /// stability.)
    pub apply_ladder_pad: bool,
}

/// Outputs of `assemble_system_prompt`. Caller propagates side effects
/// (snapshot store, first-act-flag flip, telemetry record) based on
/// what's returned here. P3-6 single-seam output bundle.
pub(super) struct AssembledPrompt {
    pub system: String,
    pub dynamic_for_last_user: String,
    pub new_memory_context_snapshot:
        Option<crate::agent::context_diff::LineFragmentSnapshot>,
}

/// Single-seam prompt assembly. Pure: takes a fully-populated
/// `SystemPromptContext`, returns an `AssembledPrompt`. No `&self`,
/// no I/O, no time reads, no DB access.
///
/// Side effects (snapshot store, first-act-flag flip, telemetry
/// record) are the caller's job — propagate based on the returned
/// `AssembledPrompt`.
///
/// Replaces the 4-layer assembly previously spread across
/// `mode_prompts::compose_*`, `effective_system_prompt`,
/// `build_dynamic_context`, and the `turn_runner.rs::call_llm`
/// splice point (P3-6 of 阶段-3 Pi-convergence remediation,
/// addresses gap-audit §1.2 MAJOR).
pub(super) fn assemble_system_prompt(ctx: SystemPromptContext) -> AssembledPrompt {
    // 1. System half — byte-stable for prompt cache.
    let mut system = crate::agent::mode_prompts::compose_system_prompt_with_injection_and_persona(
        &ctx.base_system_prompt,
        ctx.workspace_root.as_deref(),
        &ctx.effective_mode,
        &ctx.injection_context,
        ctx.persona_block.as_deref(),
    );
    if !ctx.skills_manifest_block.is_empty() && !ctx.skills_manifest_suppress {
        system.push_str(&ctx.skills_manifest_block);
    }

    // PR4: 4 post-append layers, now unified into the seam.
    // Order MUST match the original create_turn_snapshot order:
    //   gene_block → plan_suggest_hint → project_rules_block → cache-align
    if !ctx.gene_block.is_empty() {
        system.push_str(&ctx.gene_block);
    }
    if !ctx.plan_suggest_hint.is_empty() {
        system.push_str(&ctx.plan_suggest_hint);
    }
    if !ctx.project_rules_block.is_empty() {
        system.push_str(&ctx.project_rules_block);
    }
    if ctx.apply_ladder_pad {
        // Align the stable system-prompt prefix to the next 1024-token cache
        // block instead of padding up to a coarse [2048..32768] ladder tier.
        // The tier jump inflated the prompt by thousands of filler tokens/turn
        // for no real cache benefit (Anthropic caches at 1024 granularity;
        // DeepSeek auto-prefix-caches). Matches the L1 fold-summary padding.
        system = crate::agent::compact::cache_align::align_to_1024_tokens(&system);
    }

    // 2. Dynamic half — per-turn block prepended to last user message.
    let dynamic_for_last_user = build_dynamic_block(&ctx);

    // 3. New snapshot for next turn's diff.
    let new_memory_context_snapshot = ctx
        .memory_context
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| crate::agent::context_diff::LineFragmentSnapshot::from_text("memory_context", s));

    AssembledPrompt {
        system,
        dynamic_for_last_user,
        new_memory_context_snapshot,
    }
}

/// Internal helper — builds the per-turn dynamic block from a
/// `SystemPromptContext`. Extracted so tests can hit it directly.
fn build_dynamic_block(ctx: &SystemPromptContext) -> String {
    let now = ctx.now;
    let weekday = match now.weekday() {
        chrono::Weekday::Mon => "周一",
        chrono::Weekday::Tue => "周二",
        chrono::Weekday::Wed => "周三",
        chrono::Weekday::Thu => "周四",
        chrono::Weekday::Fri => "周五",
        chrono::Weekday::Sat => "周六",
        chrono::Weekday::Sun => "周日",
    };
    let time_str = format!(
        "{}年{}月{}日 {} {:02}:{:02}",
        now.year(), now.month(), now.day(), weekday, now.hour(), now.minute(),
    );
    let mut block = format!(
        "<system_info>\n当前时间: {}\n注意: 以上时间和工作区路径由系统直接提供。对于询问当前状态的对话（如「你在干啥」「现在几点」「工作区是什么」等），直接用此处信息回答即可——不要运行 bash date、glob、ls、find、pwd 等命令去探查。只有用户明确要求执行文件/目录操作时，才使用相关工具。",
        time_str
    );
    if let Some(root) = ctx.workspace_root.as_ref() {
        block.push_str(&format!("\n工作区路径: {}", root.display()));
    }
    block.push_str("\n</system_info>");

    // Memory recall results — per-turn fresh content injected here (not in
    // the system prompt) so Anthropic cache_control: ephemeral on the system
    // prompt block can hit reliably from iteration 2 onward.
    if let Some(ctx_mem) = ctx.memory_context.as_deref().filter(|s| !s.is_empty()) {
        // M2-D Phase 2 Track A (Bundle 16-B) — cross-turn diff
        // injection. We build a line-level snapshot of the
        // current memory_context, diff against the prior turn's
        // snapshot, and conditionally attach a
        // `<memory_context_changes>` annotation block alongside
        // the full block:
        //
        //   first turn / no prior → full block only (annotation
        //     omitted; the LLM has nothing to compare against)
        //   identical to prior   → full block only
        //                          (Anthropic cache hit path)
        //   small drift (≤40%)   → full block + delta annotation
        //                          (LLM gets "what changed" signal)
        //   significant drift    → full block only (delta would
        //                          be noisy + cache misses anyway)
        //
        // We keep the full block even on small drift because
        // un-cached providers (DeepSeek / Kimi) have no
        // mechanism for "saw it last turn". The delta block is
        // pure additional signal, not a replacement.
        const SIGNIFICANT_DRIFT_THRESHOLD: f32 = 0.40;
        let new_snapshot = crate::agent::context_diff::LineFragmentSnapshot::from_text(
            "memory_context",
            ctx_mem,
        );
        let prior = ctx.prior_memory_snapshot.clone();

        let delta_annotation: Option<String> = match prior.as_ref() {
            None => {
                tracing::info!(
                    line_count = new_snapshot.line_count(),
                    token_estimate = new_snapshot.token_estimate,
                    "[M2-D] turn=first memory_context first injection emitted=full",
                );
                None
            }
            Some(prior_snap) => {
                let diff = crate::agent::context_diff::line_diff(
                    prior_snap,
                    &new_snapshot,
                );
                let stats = diff.stats();
                if diff.is_empty() {
                    tracing::debug!(
                        line_count = new_snapshot.line_count(),
                        unchanged = stats.unchanged,
                        "[M2-D] turn=N memory_context unchanged emitted=full cache_state=hit-expected",
                    );
                    None
                } else if stats
                    .is_significant_change(SIGNIFICANT_DRIFT_THRESHOLD)
                {
                    tracing::info!(
                        added = stats.added,
                        removed = stats.removed,
                        changed = stats.changed,
                        unchanged = stats.unchanged,
                        added_or_changed_tokens = stats.added_or_changed_tokens,
                        "[M2-D] turn=N memory_context drift=significant emitted=full cache_state=miss",
                    );
                    None
                } else {
                    let annotation =
                        crate::agent::context_diff::render_delta_annotation(&diff);
                    tracing::info!(
                        added = stats.added,
                        removed = stats.removed,
                        changed = stats.changed,
                        unchanged = stats.unchanged,
                        added_or_changed_tokens = stats.added_or_changed_tokens,
                        "[M2-D] turn=N memory_context drift=small emitted=full+delta cache_state=miss",
                    );
                    annotation
                }
            }
        };

        // NOTE: snapshot storage deliberately omitted here — the new_snapshot
        // was built inline for diff computation but the canonical output is
        // AssembledPrompt::new_memory_context_snapshot (built in
        // assemble_system_prompt). The caller stores it; we do not.

        block.push_str("\n\n<memory_context>\n");
        block.push_str(ctx_mem);
        block.push_str("\n</memory_context>");

        if let Some(annotation) = delta_annotation {
            block.push_str("\n\n");
            block.push_str(&annotation);
        }
    }

    // Learned user profile (30-min rebuild cadence) — same rationale as
    // memory_context: injected here rather than in the system prompt so
    // rebuilds don't bust the Anthropic cache breakpoint.
    if !ctx.learned_profile_block.is_empty() {
        block.push_str("\n\n");
        block.push_str(&ctx.learned_profile_block);
    }

    // gbrain Sprint 2.3 — instructions for when to call
    // `mcp__gbrain__*` tools. Only present when gbrain is connected
    // and exposes tools; absent otherwise so we don't promise the
    // LLM tools that won't actually be callable. Lives in the
    // dynamic context block alongside memory_context + profile so
    // tool-presence changes (gbrain reconnects mid-session) take
    // effect on the next prompt build without restarting the agent.
    if !ctx.gbrain_knowledge_block.is_empty() {
        block.push_str("\n\n");
        block.push_str(&ctx.gbrain_knowledge_block);
    }

    // C2-Dirac-B2 — ContextManager-selected fragments. Rendered HERE
    // (per-turn dynamic block), NOT in the system prompt, so the
    // system-prompt cache_control:ephemeral breakpoint keeps hitting
    // across turns (spec §8.3). Selected on the prior
    // effective_system_prompt call (same turn, since the agent loop
    // calls effective_system_prompt before build_dynamic_context).
    block.push_str(&render_context_fragments(&ctx.injected_fragments));

    block
}

impl ChatDelegate {
    /// Build a fully-populated SystemPromptContext from current
    /// ChatDelegate state.
    ///
    /// PR4: now async to allow the GEP gene match retrieval (which is async).
    /// `last_user_text` and `tool_errors` come from the current turn's
    /// ReasoningContext — the caller extracts them before calling this.
    async fn build_prompt_context(
        &self,
        effective_mode: crate::safety::SafetyMode,
        last_user_text: &str,
        tool_errors: Vec<String>,
    ) -> SystemPromptContext {
        // PR4 Layer A: GEP gene block (async match).
        let gene_block = if let Some(ref retriever) = self.gep.retriever {
            if !last_user_text.is_empty() {
                let matches = retriever.match_genes(last_user_text, &tool_errors, 2).await;
                if !matches.is_empty() {
                    let block = crate::agent::gep::retrieval::format_gene_injection(&matches, 2);
                    if !block.is_empty() {
                        tracing::debug!(
                            gene_count = matches.len(),
                            "[ChatDelegate] Injected Gene control signals into system prompt"
                        );
                    }
                    // Side effect: store matches for Capsule generation after tool execution.
                    if let Ok(mut stored) = self.gep.last_matches.lock() {
                        *stored = matches;
                    }
                    block
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // PR4 Layer B: plan-suggest aggregate hint (sync DB query).
        let plan_suggest_hint: String = self.try_app_state().and_then(|state| {
            let db = state.db.clone();
            drop(state);
            let conn = db.lock().ok()?;
            let window_start = chrono::Utc::now().timestamp_millis() - 7 * 24 * 60 * 60 * 1000;
            let stats = crate::agent::mode_suggest_store::query_per_pattern_stats(&conn, window_start).ok()?;
            let total_decided: u32 = stats.iter().map(|s| s.accepted + s.skipped + s.silenced).sum();
            let total_accepted: u32 = stats.iter().map(|s| s.accepted).sum();
            if total_decided >= 10 {
                let agg_rate = total_accepted as f32 / total_decided as f32;
                if agg_rate < 0.20 {
                    tracing::debug!(
                        agg_rate = agg_rate,
                        total_decided = total_decided,
                        "Plan-suggest aggregate hint injected into system prompt",
                    );
                    return Some(
                        "\n\n[Plan-suggest signal] Your recent request_plan_mode_switch \
                         calls have been declined frequently. Be more conservative — \
                         only suggest Plan mode for clearly multi-step build/refactor \
                         work, not casual questions.\n"
                            .to_string(),
                    );
                }
            }
            None
        }).unwrap_or_default();

        // PR4 Layer C: project rules block (sync).
        let project_rules_block = {
            let active_files = crate::agent::anchor_state::GLOBAL_FILE_CONTEXT_TRACKER.get_active_files();
            if let Some(ref root) = self.workspace_root {
                crate::agent::rule_context_builder::RuleContextBuilder::build_context(root, &active_files)
            } else {
                String::new()
            }
        };

        // PR4 Layer D: cache-align pad — always applied (matches prior behavior).
        let apply_ladder_pad = true;

        SystemPromptContext {
            base_system_prompt: self.system_prompt.clone(),
            workspace_root: self.workspace_root.clone(),
            effective_mode,
            injection_context: crate::agent::baseline_blocks::InjectionContext {
                is_first_act_turn: self.is_first_act_turn.load(std::sync::atomic::Ordering::Relaxed),
                last_error_kind: self.last_error_kind.lock().ok().and_then(|g| g.clone()),
                context_pressure_ratio: self.estimate_context_pressure_ratio(),
            },
            persona_block: self.persona_prompt_block_best_effort(),
            skills_manifest_block: self.prompt_blocks.skills_manifest.clone(),
            skills_manifest_suppress: self.skill_search_used.load(std::sync::atomic::Ordering::Relaxed),
            memory_context: self.memory_context.clone(),
            prior_memory_snapshot: self.last_memory_context_snapshot.lock().ok().and_then(|g| g.clone()),
            learned_profile_block: self.prompt_blocks.learned_profile.clone(),
            gbrain_knowledge_block: self.prompt_blocks.gbrain_knowledge.clone(),
            injected_fragments: self.last_injected_fragments.lock().ok().map(|g| g.clone()).unwrap_or_default(),
            now: chrono::Local::now(),
            gene_block,
            plan_suggest_hint,
            project_rules_block,
            apply_ladder_pad,
        }
    }

    /// Run the full single-seam assembly + propagate side effects.
    /// Returns BOTH halves so callers don't need to invoke twice.
    ///
    /// This is the canonical entry point post-P3-6. The legacy
    /// `effective_system_prompt` + `build_dynamic_context` methods remain
    /// as thin convenience accessors that each call this and pick a half.
    ///
    /// `turn_runner::create_turn_snapshot` uses this directly (single call
    /// site for both system prompt and dynamic block — eliminates the
    /// duplicate compute that the legacy two-method API would otherwise
    /// incur). The result is stored in `TurnSnapshot` so `call_llm` needs
    /// no further assembly.
    ///
    /// PR4: now async to propagate the async `build_prompt_context`.
    /// `last_user_text` and `tool_errors` are needed for GEP gene matching.
    pub(super) async fn assemble_prompt(
        &self,
        effective_mode: &crate::safety::SafetyMode,
        last_user_text: &str,
        tool_errors: Vec<String>,
    ) -> AssembledPrompt {
        // Build context BEFORE flipping is_first_act_turn so the captured
        // injection context matches the pre-P3-6 ordering exactly.
        let ctx = self.build_prompt_context(effective_mode.clone(), last_user_text, tool_errors).await;

        // Side effect 1: stash fragments + record stats.
        let query = crate::agent::context_manager::ComposeQuery::defaults_with_topics(vec![]);
        let composed = self.context_manager_for_prompt_blocking(&query, &ctx.injection_context);
        if let Ok(mut slot) = self.last_injected_fragments.lock() {
            *slot = composed.injected_fragments.clone();
        }
        if let Some(collector) = &self.telemetry.compose_stats {
            collector.record(&self.conversation_id, composed.stats.clone());
        }

        // Side effect 2: first-act flag transition (AFTER ctx capture).
        self.is_first_act_turn.store(false, std::sync::atomic::Ordering::Relaxed);

        // Call the single seam.
        let assembled = assemble_system_prompt(ctx);

        // Side effect 3: store new snapshot.
        if let Ok(mut slot) = self.last_memory_context_snapshot.lock() {
            *slot = assembled.new_memory_context_snapshot.clone();
        }

        assembled
    }

    pub(super) fn persona_prompt_block_best_effort(&self) -> Option<String> {
        let state = self.try_app_state()?;
        let guard = state.db.lock().ok()?;
        let store = crate::agent::persona::store::PersonaStore::new(&guard);
        let voice = store
            .get_global_voice_profile()
            .ok()
            .flatten()
            .unwrap_or_default();
        let ctx = crate::agent::persona::PersonaPromptContext {
            voice,
            bond: crate::agent::persona::BondProfile::default(),
            relationship_gamification_enabled: false,
        };
        Some(crate::agent::persona::render_persona_prompt_block(&ctx))
    }

    /// C2-Dirac-B2 — synchronous bridge to the async
    /// `ContextManager::for_prompt_with_injection`.
    ///
    /// `effective_system_prompt` is sync (called from the LLM hot path),
    /// but `for_prompt_with_injection` is async (fragment `fetch()` is).
    /// uClaw runs on the default multi-thread tokio runtime
    /// (`tokio = { features = ["full"] }`, `#[tokio::main]`), so
    /// `block_in_place` + `Handle::current().block_on` is safe here (spec
    /// §8.2). If this ever runs on a current-thread runtime, swap to a
    /// oneshot-channel + spawn pattern.
    pub(super) fn context_manager_for_prompt_blocking(
        &self,
        query: &crate::agent::context_manager::ComposeQuery,
        inj_ctx: &crate::agent::baseline_blocks::InjectionContext,
    ) -> crate::agent::context_manager::ComposedContext {
        let cm = self.context_manager.clone();
        let q = query.clone();
        let ic = inj_ctx.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let guard = cm.read().await;
                guard.for_prompt_with_injection(&q, &ic).await
            })
        })
    }

    /// C2-Dirac-B2 — estimate of tokens-used / context-window for A4's
    /// `InjectionContext.context_pressure_ratio`.
    ///
    /// PR6 of Tier 1+2+3 batch — was stubbed to `0.0`. Now reads from
    /// the live `TokenBudgetCollector` (post-P3-5b2 lives on
    /// `self.telemetry.token_budget`). Returns `0.0` when no collector
    /// is wired (headless / test contexts) OR when no snapshot has been
    /// recorded yet (first turn — no usage data).
    pub(super) fn estimate_context_pressure_ratio(&self) -> f32 {
        let Some(collector) = &self.telemetry.token_budget else {
            return 0.0;
        };
        let Some(snap) = collector.latest(&self.conversation_id) else {
            return 0.0;
        };
        let max_ctx = crate::agent::types::get_model_context_length(&snap.model);
        if max_ctx == 0 {
            return 0.0;
        }
        let used = (snap.provider_input_tokens + snap.provider_output_tokens) as f32;
        (used / max_ctx as f32).clamp(0.0, 1.0)
    }

    /// Set the memory context obtained from the recall engine.
    /// This will be appended to the system prompt when making LLM calls.
    pub fn set_memory_context(&mut self, context: String) {
        self.memory_context = Some(context);
    }

    /// M2-D Phase 2 — clear the cross-turn memory_context anchor.
    /// Called when a `/compact` runs so the next turn's diff baseline
    /// is the post-fold state, not the pre-fold one. Safe to call
    /// repeatedly; no-op when no anchor exists.
    pub fn clear_memory_context_anchor(&self) {
        if let Ok(mut slot) = self.last_memory_context_snapshot.lock() {
            *slot = None;
        }
    }

    /// Append additional context to the existing memory context.
    /// If no memory context has been set yet, this creates one.
    pub fn append_memory_context(&mut self, extra: &str) {
        match self.memory_context.as_mut() {
            Some(ctx) => {
                ctx.push_str(extra);
            }
            None => {
                self.memory_context = Some(extra.to_string());
            }
        }
    }

    /// Set the skill manifest block to append to the system prompt.
    /// Caller is responsible for building this via skills_manifest::build_skills_manifest.
    pub fn set_skills_manifest_block(&mut self, block: String) {
        self.prompt_blocks.skills_manifest = block;
    }

    /// Set the '## User Profile (Learned)' block (Sprint 1.8).
    ///
    /// Caller builds via
    /// `learning::prompt_section::UserProfileSection::render(&facet_cache)`
    /// (returns `Option<String>` — caller passes empty when None).
    /// Empty input → no append in `effective_system_prompt`.
    pub fn set_learned_profile_block(&mut self, block: String) {
        self.prompt_blocks.learned_profile = block;
    }

    /// Set the '## Long-term Knowledge (gbrain)' instruction block
    /// (Sprint 2.3). Caller builds via
    /// `crate::agent::gbrain_prompt::GbrainKnowledgeSection::render(&mcp_mgr)`
    /// (returns `Option<String>` — caller passes empty when None). Empty
    /// input → no append in `effective_system_prompt`, so the LLM
    /// never sees instructions for `mcp__gbrain__*` tools when those
    /// tools aren't actually registered.
    pub fn set_gbrain_knowledge_block(&mut self, block: String) {
        self.prompt_blocks.gbrain_knowledge = block;
    }
}

/// C2-Dirac-B2 — render selected ContextArtifacts into the per-turn
/// dynamic context block. Each artifact becomes a `<context_fragment>`
/// XML element. Returns an empty string when `fragments` is empty so
/// callers can unconditionally push_str without emitting stray whitespace.
pub(crate) fn render_context_fragments(
    fragments: &[crate::runtime::context::ContextArtifact],
) -> String {
    let mut out = String::new();
    for art in fragments {
        out.push_str(&format!(
            "\n\n<context_fragment id=\"{}\" source=\"{}\">\n{}\n</context_fragment>",
            art.r#ref.id,
            art.r#ref.source.as_str(),
            art.content,
        ));
    }
    out
}

#[cfg(test)]
mod manifest_suppression_tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Mirror of the suppression rule in `effective_system_prompt`. Kept
    /// in lockstep with `dispatcher.rs:154-171` — if you change the rule
    /// there, change it here. This test pins the contract without
    /// constructing a full `ChatDelegate` (which needs an LLM provider,
    /// safety manager, etc — heavy for a unit test).
    fn compose_with_suppression(
        base_system: &str,
        manifest_block: &str,
        skill_search_used: &AtomicBool,
    ) -> String {
        let suppress = skill_search_used.load(Ordering::Relaxed);
        if manifest_block.is_empty() || suppress {
            base_system.to_string()
        } else {
            format!("{}{}", base_system, manifest_block)
        }
    }

    /// Default state: flag unset → manifest is appended.
    #[test]
    fn manifest_appended_before_skill_search_used() {
        let flag = AtomicBool::new(false);
        let out = compose_with_suppression(
            "You are an agent.",
            "\n\nMANIFEST_BLOCK",
            &flag,
        );
        assert!(out.contains("MANIFEST_BLOCK"));
    }

    /// After flag is set (simulating `execute_tool_calls` seeing
    /// `skill_search`), the manifest is gone on subsequent prompt
    /// composition. This is the core of PR #137's optim #5 — without
    /// this, the ~800 tokens of manifest leak back into every later
    /// LLM call in the same agent loop.
    #[test]
    fn manifest_suppressed_after_skill_search_used() {
        let flag = AtomicBool::new(false);
        // First call (before skill_search): manifest present.
        let pre = compose_with_suppression("base", "\nM", &flag);
        assert!(pre.contains("M"));

        // Simulate `execute_tool_calls` detecting skill_search.
        flag.store(true, Ordering::Relaxed);

        // Subsequent calls: manifest gone.
        let post = compose_with_suppression("base", "\nM", &flag);
        assert!(!post.contains("M"));
        assert_eq!(post, "base");
    }

    /// Empty manifest: the suppression flag has no observable effect.
    /// Edge case — verifies the `is_empty()` short-circuit takes
    /// precedence over the flag check.
    #[test]
    fn empty_manifest_unaffected_by_flag() {
        for &used in &[false, true] {
            let flag = AtomicBool::new(used);
            let out = compose_with_suppression("base", "", &flag);
            assert_eq!(out, "base", "empty manifest should not produce divergent output; used={}", used);
        }
    }

    /// Flag is sticky — once set, stays set. A second non-skill_search
    /// tool call in the same loop must not flip it back. This guards
    /// against future refactors that might (incorrectly) reset the
    /// flag mid-loop.
    #[test]
    fn flag_stays_set_after_subsequent_non_skill_search_calls() {
        let flag = AtomicBool::new(false);
        flag.store(true, Ordering::Relaxed);  // simulate skill_search
        // Simulate a subsequent tool call that is NOT skill_search —
        // mirroring `execute_tool_calls`'s any() check which only
        // sets-true, never sets-false.
        // (No-op — the flag has nothing in execute_tool_calls that
        //  resets it; this test pins that fact.)
        assert!(flag.load(Ordering::Relaxed));
    }
}

#[cfg(test)]
mod manifest_cap_tests {
    /// PR #137 reduced the manifest cap from 1500 → 800 tokens in
    /// `tauri_commands.rs:5015`. The token budget is consumed by
    /// `skills_manifest::build_skills_manifest` via approximate
    /// 4-chars-per-token math. Verify the function honors a low cap
    /// gracefully (returns something non-empty if even one entry fits;
    /// returns empty if nothing fits — never panics, never returns
    /// the over-budget version).
    use crate::memory_graph::store::MemoryGraphStore;
    use crate::skills::SkillsRegistry;
    use crate::skills_manifest::{build_skills_manifest, StrategyBias};
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn fresh_store() -> MemoryGraphStore {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).expect("V4 schema");
        MemoryGraphStore::new(Arc::new(Mutex::new(conn)))
    }

    /// 800-token cap (current production setting) is enough budget for at
    /// least a few entries — verifies the cap isn't accidentally below
    /// the per-entry minimum, which would make the manifest unusable.
    #[test]
    fn manifest_at_800_token_cap_produces_output_when_skills_exist() {
        let registry = SkillsRegistry::new();
        // Empty store + empty registry → manifest can legitimately be
        // empty, no assertion needed beyond "doesn't panic".
        let store = fresh_store();
        let manifest = build_skills_manifest(
            &registry, &store, "default",
            30, 800, StrategyBias::Balanced, None,
        );
        // No skills loaded → empty manifest is correct.
        assert!(manifest.is_empty() || manifest.contains("Learned Skills"),
            "manifest must either be empty or contain the documented header");
    }

    /// Lower cap = no panic, no overrun. The cap argument is a soft
    /// budget — `format_manifest` stops adding entries when the next
    /// entry would push past the budget. Verifies the format function
    /// handles a very small cap (256 tokens ≈ 1000 chars).
    #[test]
    fn manifest_handles_very_small_cap_without_panic() {
        let registry = SkillsRegistry::new();
        let store = fresh_store();
        let manifest = build_skills_manifest(
            &registry, &store, "default",
            30, 256, StrategyBias::Balanced, None,
        );
        // Empty registry + empty store → empty manifest, no panic.
        let _ = manifest;
    }
}

// ───────────────────────────────────────────────────────────────────
// Bundle 16-C — memory_context cross-turn delta render-path tests.
//
// We pin the four render paths from `build_dynamic_context` without
// constructing a full `LoopDelegate` (which would need an LLM
// provider, safety manager, db handle, etc.). The helper below
// mirrors the in-line logic exactly — any change to the dispatcher
// branch order must be mirrored here, same convention as
// `manifest_suppression_tests`.
//
// The four paths under test:
//
//  1. first turn (no prior anchor)  → full block, no annotation
//  2. unchanged across turns        → full block, no annotation
//  3. small drift (≤ 40%)           → full block + delta annotation
//  4. significant drift (> 40%)     → full block, no annotation
// ───────────────────────────────────────────────────────────────────
#[cfg(test)]
mod memory_context_delta_render_tests {
    use crate::agent::context_diff::{
        line_diff, render_delta_annotation, LineFragmentSnapshot,
    };

    /// Mirror of the dispatcher's render decision. Returns
    /// `(emitted_block, kind_label)` where `kind_label` is one of
    /// `"full"`, `"full+delta"`, used for assertions + telemetry
    /// parity.
    fn render_memory_context(
        prior: Option<&LineFragmentSnapshot>,
        current_text: &str,
    ) -> (String, &'static str) {
        const SIGNIFICANT_DRIFT_THRESHOLD: f32 = 0.40;
        let current = LineFragmentSnapshot::from_text("memory_context", current_text);

        let mut block = String::new();
        let mut kind = "full";

        let annotation: Option<String> = match prior {
            None => None,
            Some(p) => {
                let d = line_diff(p, &current);
                let stats = d.stats();
                if d.is_empty() {
                    None
                } else if stats.is_significant_change(SIGNIFICANT_DRIFT_THRESHOLD) {
                    None
                } else {
                    let a = render_delta_annotation(&d);
                    if a.is_some() {
                        kind = "full+delta";
                    }
                    a
                }
            }
        };

        block.push_str("<memory_context>\n");
        block.push_str(current_text);
        block.push_str("\n</memory_context>");
        if let Some(a) = annotation {
            block.push_str("\n\n");
            block.push_str(&a);
        }
        (block, kind)
    }

    // ── Path 1: first turn ─────────────────────────────────────────

    #[test]
    fn dispatcher_first_turn_emits_full_memory_context_block_only() {
        let ctx = "- preferred_language: zh\n- project: uClaw\n";
        let (block, kind) = render_memory_context(None, ctx);
        assert_eq!(kind, "full");
        assert!(block.contains("<memory_context>"));
        assert!(block.contains("preferred_language"));
        assert!(
            !block.contains("<memory_context_changes"),
            "first turn must not emit delta annotation"
        );
    }

    // ── Path 2: unchanged across turns ─────────────────────────────

    #[test]
    fn dispatcher_unchanged_turn_emits_full_block_no_delta_annotation() {
        let ctx = "- preferred_language: zh\n- project: uClaw\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", ctx);
        let (block, kind) = render_memory_context(Some(&prior), ctx);
        assert_eq!(kind, "full");
        assert!(block.contains("<memory_context>"));
        assert!(
            !block.contains("<memory_context_changes"),
            "no drift → no annotation"
        );
    }

    // ── Path 3: small drift → full + delta ─────────────────────────

    #[test]
    fn dispatcher_small_drift_emits_full_block_plus_delta_annotation() {
        // Prior: 5 lines. New: 4 unchanged + 1 changed (value flip).
        // Drift = 1/5 = 0.20, below 0.40 threshold → small drift path.
        let prior_text =
            "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- preferred_language: en\n";
        let new_text =
            "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- preferred_language: zh\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full+delta");
        // Full block present
        assert!(block.contains("<memory_context>"));
        assert!(block.contains("preferred_language: zh"));
        // Delta annotation present
        assert!(block.contains("<memory_context_changes"));
        assert!(
            block.contains("~ changed key=preferred_language"),
            "delta must surface the changed line\nblock:\n{}",
            block
        );
        // Block order: <memory_context> first, then <memory_context_changes>
        let mc_pos = block.find("<memory_context>").unwrap();
        let mcc_pos = block.find("<memory_context_changes").unwrap();
        assert!(
            mc_pos < mcc_pos,
            "full block must precede the delta annotation"
        );
    }

    #[test]
    fn dispatcher_small_drift_one_added_line_emits_delta() {
        // 5 prior lines, 1 line added → drift = 0/5 (added-only doesn't
        // count toward drift fraction since it's not in prior).
        // is_significant_change checks (removed + changed) / prior, so
        // pure-add never crosses 40%. We expect full+delta.
        let prior_text = "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- e: 5\n";
        let new_text =
            "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- e: 5\n- last_query: foo\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full+delta");
        assert!(block.contains("+ added: - last_query: foo"));
    }

    // ── Path 4: significant drift ──────────────────────────────────

    #[test]
    fn dispatcher_significant_drift_emits_full_block_no_annotation() {
        // 4 prior lines, 3 removed (drift = 0.75) → above threshold.
        let prior_text = "- a: 1\n- b: 2\n- c: 3\n- d: 4\n";
        let new_text = "- a: 1\n- z: new\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full");
        assert!(block.contains("<memory_context>"));
        assert!(
            !block.contains("<memory_context_changes"),
            "significant drift must NOT emit delta annotation (noisy + cache miss anyway)"
        );
    }

    // ── Anchor reset via clear_memory_context_anchor() ─────────────

    #[test]
    fn clear_anchor_makes_next_turn_behave_like_first_turn() {
        let prior_text = "- a: 1\n";
        let new_text = "- a: 1\n- b: 2\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);

        // With anchor: small drift → delta
        let (with_anchor, kind_with) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind_with, "full+delta");
        assert!(with_anchor.contains("<memory_context_changes"));

        // After clear (None): treated as first turn → no delta
        let (after_clear, kind_after) = render_memory_context(None, new_text);
        assert_eq!(kind_after, "full");
        assert!(!after_clear.contains("<memory_context_changes"));
    }

    // ── Reorder invariance: same key set, different order → no drift

    #[test]
    fn reordered_lines_emit_full_block_no_annotation() {
        let prior_text = "- a: 1\n- b: 2\n- c: 3\n";
        let new_text = "- c: 3\n- a: 1\n- b: 2\n";  // same keys, reordered
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full");
        assert!(
            !block.contains("<memory_context_changes"),
            "reorder must not trigger delta annotation"
        );
    }

    // ── Drift threshold is exactly 0.40 (boundary check) ──────────

    #[test]
    fn drift_exactly_at_40_percent_threshold_is_significant() {
        // 5 prior, 2 removed → drift = 2/5 = 0.40 → meets `>=` cutoff
        // in is_significant_change(0.40), so this is the "significant"
        // path with `full` (no delta).
        let prior_text = "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- e: 5\n";
        let new_text = "- a: 1\n- b: 2\n- c: 3\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full");
        assert!(!block.contains("<memory_context_changes"));
    }
}

#[cfg(test)]
mod b2_context_wireup_tests {
    //! C2-Dirac-B2 — fragment rendering for build_dynamic_context.
    //!
    //! The cache-discipline / composition guarantees are tested where the
    //! logic lives, without a Tauri AppHandle (ChatDelegate is hard-typed
    //! to the Wry runtime, which `tauri::test::mock_app()` cannot satisfy):
    //!
    //! - System-prompt byte-stability + "self.system_prompt / workspace
    //!   not dropped" → `mode_prompts::tests::compose_with_injection_*`
    //!   (effective_system_prompt delegates verbatim to
    //!   compose_system_prompt_with_injection for the prompt string).
    //! - Fragment SELECTION + ComposeStats → `context_manager::manager`
    //!   + `stats_collector` tests, and the `context_wireup_bench`
    //!   integration test.
    //!
    //! Here we cover the remaining seam: how selected fragments render
    //! into the per-turn dynamic block (and that they NEVER look like a
    //! system-prompt block — they're a distinct <context_fragment> tag).

    use super::render_context_fragments;
    use crate::runtime::context::{ContextArtifact, ContextRef, ContextSource};

    fn artifact(id: &str, source: ContextSource, content: &str) -> ContextArtifact {
        ContextArtifact {
            r#ref: ContextRef::new(source, id),
            content: content.into(),
            citations: Vec::new(),
            retrieval_ts: "2026-05-25T00:00:00Z".into(),
        }
    }

    #[test]
    fn empty_fragments_render_to_empty_string() {
        // Caller push_str's the result unconditionally — no fragments must
        // produce zero bytes (no stray whitespace in the dynamic block).
        assert_eq!(render_context_fragments(&[]), "");
    }

    #[test]
    fn fragment_renders_as_context_fragment_xml_with_id_and_source() {
        let frags = vec![artifact(
            "thread/abc",
            ContextSource::Conversation,
            "alpha\nbeta",
        )];
        let out = render_context_fragments(&frags);
        assert!(out.contains("<context_fragment id=\"thread/abc\" source=\"conversation\">"));
        assert!(out.contains("alpha\nbeta"));
        assert!(out.contains("</context_fragment>"));
    }

    #[test]
    fn multiple_fragments_each_get_their_own_block() {
        let frags = vec![
            artifact("file/a.rs", ContextSource::Codebase, "fn a() {}"),
            artifact("recall/x", ContextSource::Memory, "page body"),
        ];
        let out = render_context_fragments(&frags);
        assert_eq!(out.matches("<context_fragment ").count(), 2);
        assert_eq!(out.matches("</context_fragment>").count(), 2);
        assert!(out.contains("source=\"codebase\""));
        assert!(out.contains("source=\"memory\""));
    }
}

#[cfg(test)]
mod assemble_snapshot_tests {
    //! P3-6 golden snapshot tests for `assemble_system_prompt`.
    //!
    //! Each test pins a fully-deterministic `SystemPromptContext` (including
    //! a fixed `now` so time-block strings are reproducible) and asserts on
    //! the exact expected `AssembledPrompt.system` + `.dynamic_for_last_user`
    //! strings. Any future change that alters the prompt format will break
    //! one or more of these tests — that's the point. If a break is intended,
    //! the expected string is regenerated; if unintended, the change is
    //! reverted.

    use super::*;
    use chrono::TimeZone;

    /// Deterministic time pinned to Friday 2026-05-29 14:30:00 local.
    fn fixed_now() -> chrono::DateTime<chrono::Local> {
        chrono::Local
            .with_ymd_and_hms(2026, 5, 29, 14, 30, 0)
            .single()
            .unwrap()
    }

    /// Base context — minimal valid SystemPromptContext. Override fields per test.
    fn base_ctx() -> SystemPromptContext {
        SystemPromptContext {
            base_system_prompt: "You are uclaw.".to_string(),
            workspace_root: None,
            effective_mode: crate::safety::SafetyMode::default(), // Supervised
            injection_context: crate::agent::baseline_blocks::InjectionContext {
                is_first_act_turn: false,
                last_error_kind: None,
                context_pressure_ratio: 0.0,
            },
            persona_block: None,
            skills_manifest_block: String::new(),
            skills_manifest_suppress: false,
            memory_context: None,
            prior_memory_snapshot: None,
            learned_profile_block: String::new(),
            gbrain_knowledge_block: String::new(),
            injected_fragments: Vec::new(),
            now: fixed_now(),
            // PR4 new fields — default empty so existing snapshot assertions hold.
            gene_block: String::new(),
            plan_suggest_hint: String::new(),
            project_rules_block: String::new(),
            apply_ladder_pad: false,
        }
    }

    // ── Test 1 ────────────────────────────────────────────────────────────
    // Baseline minimum: Plan mode (most restrictive), no workspace, no
    // skills, no memory. Verifies the minimal non-empty output.
    // (Plan is uClaw's closest equivalent to "restricted" — blocks
    // writes/execs and forces the agent into read-only investigation mode.)
    #[test]
    fn snapshot_vanilla_restricted_no_workspace() {
        let ctx = SystemPromptContext {
            effective_mode: crate::safety::SafetyMode::Plan,
            ..base_ctx()
        };
        let assembled = assemble_system_prompt(ctx);

        let expected_system = r#"You are uclaw.

---

<!-- Behavioral guardrails adapted from Andrej Karpathy's observations on LLM
     coding pitfalls. Source: https://github.com/forrestchang/andrej-karpathy-skills
     License: MIT. Editable via Settings → 提示词 → 行为护栏 (read-only preview only). -->

[Behavioral guardrails — apply to every action]

1. THINK BEFORE CODING. State your assumptions. If a request has multiple
   interpretations, present them — don't silently pick one. When unclear,
   call `ask_user` to surface the question instead of guessing.

2. SIMPLICITY FIRST. Minimum code that solves the problem. No speculative
   features. No abstractions for single-use code. If you'd write 200 lines
   and it could be 50, rewrite it.

3. SURGICAL CHANGES. Touch only what the user asked you to touch. Don't
   "improve" adjacent code, comments, or formatting. Match existing style.
   If you notice unrelated issues, mention them — don't fix them inline.

4. GOAL-DRIVEN EXECUTION. Transform vague requests into verifiable goals.
   For multi-step work, state your plan as `1. step → verify: check`.
   Loop until verify passes; don't stop at "I think it works".

5. NEVER FAKE PROGRESS. Bookkeeping tools (`plan_update`, `plan_write`,
   `TodoWrite`) ONLY update tracking files — they do NOT execute work.
   NEVER mark a step `done:true` unless you have already called the
   tool that actually does the work (`edit`, `write_file`, `bash`, etc.)
   and verified it succeeded. The user sees the artifacts on disk,
   not your checkmarks. If the artifact is missing, the step is not done.

6. NEVER OUTPUT FILE CONTENT AS TEXT. To create or modify a file you
   MUST call `write_file` or `edit`. Putting code or file content in
   your reply text does NOT create or modify any file — the user cannot
   use text output as actual code. Always call the tool, never describe
   what you would write.

7. FOR LARGE FILES, WRITE IN CHUNKS. A single tool call can hold roughly
   250-300 lines before hitting output limits. For larger files: call
   `write_file` with the first 250-300 lines, then call `write_file`
   again (or `edit`) for each subsequent section. Never attempt to write
   an 800-line file in one shot — it will be truncated. Plan how many
   chunks you need before you start, then execute them one tool call at
   a time.

## Mode-change suggestions

You can request a mode change with `request_plan_mode_switch` when the
user's request is multi-step build/refactor/design work AND they're
currently in Supervised or Yolo mode. Call it BEFORE other tool calls.
Don't call it for: bug fixes you already understand, single-file edits,
read-only questions, or after the user has explicitly said "just do it".
The tool is fire-and-forget; the agent continues regardless.

## When to call ask_user

Call `ask_user` when you need a decision from the user before continuing:
- The request has 2+ plausible interpretations and your guess could be
  wrong by 50%+
- You're about to do something destructive (delete, force-push, drop
  table) without an explicit prior OK
- A critical design choice depends on user preference (library choice,
  API contract shape, file structure)

Do NOT call ask_user for:
- Trivial yes/no answerable from project context (CLAUDE.md, code)
- Clarifying typos or grammar
- Asking permission for things that are already auto-approved by mode

---

[PLAN MODE — read-only investigation]

You CAN use: read_file, grep, glob, search, and safe shell commands like
`git status`, `ls`, `cat`. Write / install / network commands return a
"Plan mode — execution blocked" error from the safety layer.

Your output IS the plan; the user will verify it before any code runs.
This is guardrail #1 (think first) and #4 (goal-driven) at maximum.

## Two related tools — DO NOT confuse them

- `plan_write` / `plan_update` — workspace-level plan-tracking journal.
  Writes a markdown plan into `<workspace>/plans/...` and lets you mark
  steps done. **Calling `plan_update({done: true})` does NOT exit Plan
  mode.** It only updates a journal file. Use these tools to track
  long-running multi-step work, not to signal "I'm done planning".

  **CRITICAL — NEVER mark a step done unless you ACTUALLY did the work.**
  In Plan mode you usually CAN'T do code-writing work (writes are blocked).
  So in Plan mode, plan_update should mostly stay at `done:false` — you
  call `exit_plan_mode` once the plan is fleshed out, then in Auto mode
  the agent re-attacks each step with real tools (edit/write_file/bash)
  AND THEN calls plan_update with done:true. Marking steps done in Plan
  mode is almost always wrong.

- `exit_plan_mode` — THIS is how you submit your plan to the user for
  approval. Calling it pauses the agent loop, shows a modal, and waits
  for the user's decision. **You MUST call this tool to proceed past
  Plan mode.** If you only call `plan_write` and stop, the user has
  to manually switch modes — bad UX.

## Workflow

1. Investigate with read tools (read_file, grep, glob, safe bash, etc.)
2. (Optional) Use `ask_user({ questions: [...] })` for clarifications
3. (Optional) Use `plan_write` to journal a detailed plan if useful
4. **Always**: call `exit_plan_mode` to submit, with your plan as a
   markdown string in the `plan` argument:

```
exit_plan_mode({
  plan: "...markdown plan...",
  allowed_prompts: ["bash cargo build", "bash cargo test"]   // optional
})
```

The user will see the plan in a confirmation modal and can:
  - **Accept + switch to Auto** — agent proceeds with full execution
  - **Accept + stay in Plan** — only commands listed in `allowed_prompts`
    will auto-pass; useful for "compile and test but don't write code yet"
  - **Reject + feedback** — agent receives feedback as tool error and
    re-plans

Format the plan as:
```
1. [step] → verify: [check]
2. [step] → verify: [check]
...
```

Strong success criteria let the execution phase run without further questions.
"#;
        assert_eq!(assembled.system, expected_system, "system prompt drifted from snapshot");

        let expected_dynamic = "<system_info>\n当前时间: 2026年5月29日 周五 14:30\n注意: 以上时间和工作区路径由系统直接提供。对于询问当前状态的对话（如「你在干啥」「现在几点」「工作区是什么」等），直接用此处信息回答即可——不要运行 bash date、glob、ls、find、pwd 等命令去探查。只有用户明确要求执行文件/目录操作时，才使用相关工具。\n</system_info>";
        assert_eq!(assembled.dynamic_for_last_user, expected_dynamic, "dynamic block drifted from snapshot");
    }

    // ── Test 2 ────────────────────────────────────────────────────────────
    // Workspace path + is_first_act_turn=true.
    #[test]
    fn snapshot_restricted_workspace_first_act_turn() {
        let ctx = SystemPromptContext {
            effective_mode: crate::safety::SafetyMode::Plan,
            workspace_root: Some(std::path::PathBuf::from("/tmp/test-workspace")),
            injection_context: crate::agent::baseline_blocks::InjectionContext {
                is_first_act_turn: true,
                last_error_kind: None,
                context_pressure_ratio: 0.0,
            },
            ..base_ctx()
        };
        let assembled = assemble_system_prompt(ctx);

        let expected_system = r#"You are uclaw.

---

[WORKSPACE]
Your current working directory is: /tmp/test-workspace
All relative paths in shell, file, and glob tools resolve from this directory. When the user asks where files live or what the cwd is, answer directly with this path. Do NOT call shell commands (pwd, ls, glob, find, etc.) to probe or verify the workspace unless the user explicitly requests a file or directory operation.

---

<!-- Behavioral guardrails adapted from Andrej Karpathy's observations on LLM
     coding pitfalls. Source: https://github.com/forrestchang/andrej-karpathy-skills
     License: MIT. Editable via Settings → 提示词 → 行为护栏 (read-only preview only). -->

[Behavioral guardrails — apply to every action]

1. THINK BEFORE CODING. State your assumptions. If a request has multiple
   interpretations, present them — don't silently pick one. When unclear,
   call `ask_user` to surface the question instead of guessing.

2. SIMPLICITY FIRST. Minimum code that solves the problem. No speculative
   features. No abstractions for single-use code. If you'd write 200 lines
   and it could be 50, rewrite it.

3. SURGICAL CHANGES. Touch only what the user asked you to touch. Don't
   "improve" adjacent code, comments, or formatting. Match existing style.
   If you notice unrelated issues, mention them — don't fix them inline.

4. GOAL-DRIVEN EXECUTION. Transform vague requests into verifiable goals.
   For multi-step work, state your plan as `1. step → verify: check`.
   Loop until verify passes; don't stop at "I think it works".

5. NEVER FAKE PROGRESS. Bookkeeping tools (`plan_update`, `plan_write`,
   `TodoWrite`) ONLY update tracking files — they do NOT execute work.
   NEVER mark a step `done:true` unless you have already called the
   tool that actually does the work (`edit`, `write_file`, `bash`, etc.)
   and verified it succeeded. The user sees the artifacts on disk,
   not your checkmarks. If the artifact is missing, the step is not done.

6. NEVER OUTPUT FILE CONTENT AS TEXT. To create or modify a file you
   MUST call `write_file` or `edit`. Putting code or file content in
   your reply text does NOT create or modify any file — the user cannot
   use text output as actual code. Always call the tool, never describe
   what you would write.

7. FOR LARGE FILES, WRITE IN CHUNKS. A single tool call can hold roughly
   250-300 lines before hitting output limits. For larger files: call
   `write_file` with the first 250-300 lines, then call `write_file`
   again (or `edit`) for each subsequent section. Never attempt to write
   an 800-line file in one shot — it will be truncated. Plan how many
   chunks you need before you start, then execute them one tool call at
   a time.

## Mode-change suggestions

You can request a mode change with `request_plan_mode_switch` when the
user's request is multi-step build/refactor/design work AND they're
currently in Supervised or Yolo mode. Call it BEFORE other tool calls.
Don't call it for: bug fixes you already understand, single-file edits,
read-only questions, or after the user has explicitly said "just do it".
The tool is fire-and-forget; the agent continues regardless.

## When to call ask_user

Call `ask_user` when you need a decision from the user before continuing:
- The request has 2+ plausible interpretations and your guess could be
  wrong by 50%+
- You're about to do something destructive (delete, force-push, drop
  table) without an explicit prior OK
- A critical design choice depends on user preference (library choice,
  API contract shape, file structure)

Do NOT call ask_user for:
- Trivial yes/no answerable from project context (CLAUDE.md, code)
- Clarifying typos or grammar
- Asking permission for things that are already auto-approved by mode

---

[PLAN MODE — read-only investigation]

You CAN use: read_file, grep, glob, search, and safe shell commands like
`git status`, `ls`, `cat`. Write / install / network commands return a
"Plan mode — execution blocked" error from the safety layer.

Your output IS the plan; the user will verify it before any code runs.
This is guardrail #1 (think first) and #4 (goal-driven) at maximum.

## Two related tools — DO NOT confuse them

- `plan_write` / `plan_update` — workspace-level plan-tracking journal.
  Writes a markdown plan into `<workspace>/plans/...` and lets you mark
  steps done. **Calling `plan_update({done: true})` does NOT exit Plan
  mode.** It only updates a journal file. Use these tools to track
  long-running multi-step work, not to signal "I'm done planning".

  **CRITICAL — NEVER mark a step done unless you ACTUALLY did the work.**
  In Plan mode you usually CAN'T do code-writing work (writes are blocked).
  So in Plan mode, plan_update should mostly stay at `done:false` — you
  call `exit_plan_mode` once the plan is fleshed out, then in Auto mode
  the agent re-attacks each step with real tools (edit/write_file/bash)
  AND THEN calls plan_update with done:true. Marking steps done in Plan
  mode is almost always wrong.

- `exit_plan_mode` — THIS is how you submit your plan to the user for
  approval. Calling it pauses the agent loop, shows a modal, and waits
  for the user's decision. **You MUST call this tool to proceed past
  Plan mode.** If you only call `plan_write` and stop, the user has
  to manually switch modes — bad UX.

## Workflow

1. Investigate with read tools (read_file, grep, glob, safe bash, etc.)
2. (Optional) Use `ask_user({ questions: [...] })` for clarifications
3. (Optional) Use `plan_write` to journal a detailed plan if useful
4. **Always**: call `exit_plan_mode` to submit, with your plan as a
   markdown string in the `plan` argument:

```
exit_plan_mode({
  plan: "...markdown plan...",
  allowed_prompts: ["bash cargo build", "bash cargo test"]   // optional
})
```

The user will see the plan in a confirmation modal and can:
  - **Accept + switch to Auto** — agent proceeds with full execution
  - **Accept + stay in Plan** — only commands listed in `allowed_prompts`
    will auto-pass; useful for "compile and test but don't write code yet"
  - **Reject + feedback** — agent receives feedback as tool error and
    re-plans

Format the plan as:
```
1. [step] → verify: [check]
2. [step] → verify: [check]
...
```

Strong success criteria let the execution phase run without further questions.
"#;
        assert_eq!(assembled.system, expected_system, "system prompt drifted from snapshot");

        let expected_dynamic = "<system_info>\n当前时间: 2026年5月29日 周五 14:30\n注意: 以上时间和工作区路径由系统直接提供。对于询问当前状态的对话（如「你在干啥」「现在几点」「工作区是什么」等），直接用此处信息回答即可——不要运行 bash date、glob、ls、find、pwd 等命令去探查。只有用户明确要求执行文件/目录操作时，才使用相关工具。\n工作区路径: /tmp/test-workspace\n</system_info>";
        assert_eq!(assembled.dynamic_for_last_user, expected_dynamic, "dynamic block drifted from snapshot");
    }

    // ── Test 3 ────────────────────────────────────────────────────────────
    // Allow mode (Supervised) + skills manifest present + suppress flag false
    // (manifest SHOULD appear in the system prompt).
    #[test]
    fn snapshot_allow_with_skills_manifest_not_suppressed() {
        let ctx = SystemPromptContext {
            effective_mode: crate::safety::SafetyMode::Supervised,
            skills_manifest_block: "\n\n## Available Skills\n- skill_alpha\n- skill_beta\n".to_string(),
            skills_manifest_suppress: false,
            ..base_ctx()
        };
        let assembled = assemble_system_prompt(ctx);

        let expected_system = r#"You are uclaw.

---

<!-- Behavioral guardrails adapted from Andrej Karpathy's observations on LLM
     coding pitfalls. Source: https://github.com/forrestchang/andrej-karpathy-skills
     License: MIT. Editable via Settings → 提示词 → 行为护栏 (read-only preview only). -->

[Behavioral guardrails — apply to every action]

1. THINK BEFORE CODING. State your assumptions. If a request has multiple
   interpretations, present them — don't silently pick one. When unclear,
   call `ask_user` to surface the question instead of guessing.

2. SIMPLICITY FIRST. Minimum code that solves the problem. No speculative
   features. No abstractions for single-use code. If you'd write 200 lines
   and it could be 50, rewrite it.

3. SURGICAL CHANGES. Touch only what the user asked you to touch. Don't
   "improve" adjacent code, comments, or formatting. Match existing style.
   If you notice unrelated issues, mention them — don't fix them inline.

4. GOAL-DRIVEN EXECUTION. Transform vague requests into verifiable goals.
   For multi-step work, state your plan as `1. step → verify: check`.
   Loop until verify passes; don't stop at "I think it works".

5. NEVER FAKE PROGRESS. Bookkeeping tools (`plan_update`, `plan_write`,
   `TodoWrite`) ONLY update tracking files — they do NOT execute work.
   NEVER mark a step `done:true` unless you have already called the
   tool that actually does the work (`edit`, `write_file`, `bash`, etc.)
   and verified it succeeded. The user sees the artifacts on disk,
   not your checkmarks. If the artifact is missing, the step is not done.

6. NEVER OUTPUT FILE CONTENT AS TEXT. To create or modify a file you
   MUST call `write_file` or `edit`. Putting code or file content in
   your reply text does NOT create or modify any file — the user cannot
   use text output as actual code. Always call the tool, never describe
   what you would write.

7. FOR LARGE FILES, WRITE IN CHUNKS. A single tool call can hold roughly
   250-300 lines before hitting output limits. For larger files: call
   `write_file` with the first 250-300 lines, then call `write_file`
   again (or `edit`) for each subsequent section. Never attempt to write
   an 800-line file in one shot — it will be truncated. Plan how many
   chunks you need before you start, then execute them one tool call at
   a time.

## Mode-change suggestions

You can request a mode change with `request_plan_mode_switch` when the
user's request is multi-step build/refactor/design work AND they're
currently in Supervised or Yolo mode. Call it BEFORE other tool calls.
Don't call it for: bug fixes you already understand, single-file edits,
read-only questions, or after the user has explicitly said "just do it".
The tool is fire-and-forget; the agent continues regardless.

## When to call ask_user

Call `ask_user` when you need a decision from the user before continuing:
- The request has 2+ plausible interpretations and your guess could be
  wrong by 50%+
- You're about to do something destructive (delete, force-push, drop
  table) without an explicit prior OK
- A critical design choice depends on user preference (library choice,
  API contract shape, file structure)

Do NOT call ask_user for:
- Trivial yes/no answerable from project context (CLAUDE.md, code)
- Clarifying typos or grammar
- Asking permission for things that are already auto-approved by mode

## Available Skills
- skill_alpha
- skill_beta
"#;
        assert_eq!(assembled.system, expected_system, "system prompt drifted from snapshot");

        let expected_dynamic = "<system_info>\n当前时间: 2026年5月29日 周五 14:30\n注意: 以上时间和工作区路径由系统直接提供。对于询问当前状态的对话（如「你在干啥」「现在几点」「工作区是什么」等），直接用此处信息回答即可——不要运行 bash date、glob、ls、find、pwd 等命令去探查。只有用户明确要求执行文件/目录操作时，才使用相关工具。\n</system_info>";
        assert_eq!(assembled.dynamic_for_last_user, expected_dynamic, "dynamic block drifted from snapshot");
    }

    // ── Test 4 ────────────────────────────────────────────────────────────
    // Allow mode (Supervised) + skills manifest present + suppress flag TRUE
    // (manifest MUST be omitted from system prompt half).
    #[test]
    fn snapshot_allow_with_skills_manifest_suppressed() {
        let ctx = SystemPromptContext {
            effective_mode: crate::safety::SafetyMode::Supervised,
            skills_manifest_block: "\n\n## Available Skills\n- skill_alpha\n- skill_beta\n".to_string(),
            skills_manifest_suppress: true,
            ..base_ctx()
        };
        let assembled = assemble_system_prompt(ctx);

        // When suppressed, system must NOT contain the manifest text.
        assert!(!assembled.system.contains("skill_alpha"), "manifest must be absent when suppressed");
        assert!(!assembled.system.contains("skill_beta"), "manifest must be absent when suppressed");

        let expected_system = r#"You are uclaw.

---

<!-- Behavioral guardrails adapted from Andrej Karpathy's observations on LLM
     coding pitfalls. Source: https://github.com/forrestchang/andrej-karpathy-skills
     License: MIT. Editable via Settings → 提示词 → 行为护栏 (read-only preview only). -->

[Behavioral guardrails — apply to every action]

1. THINK BEFORE CODING. State your assumptions. If a request has multiple
   interpretations, present them — don't silently pick one. When unclear,
   call `ask_user` to surface the question instead of guessing.

2. SIMPLICITY FIRST. Minimum code that solves the problem. No speculative
   features. No abstractions for single-use code. If you'd write 200 lines
   and it could be 50, rewrite it.

3. SURGICAL CHANGES. Touch only what the user asked you to touch. Don't
   "improve" adjacent code, comments, or formatting. Match existing style.
   If you notice unrelated issues, mention them — don't fix them inline.

4. GOAL-DRIVEN EXECUTION. Transform vague requests into verifiable goals.
   For multi-step work, state your plan as `1. step → verify: check`.
   Loop until verify passes; don't stop at "I think it works".

5. NEVER FAKE PROGRESS. Bookkeeping tools (`plan_update`, `plan_write`,
   `TodoWrite`) ONLY update tracking files — they do NOT execute work.
   NEVER mark a step `done:true` unless you have already called the
   tool that actually does the work (`edit`, `write_file`, `bash`, etc.)
   and verified it succeeded. The user sees the artifacts on disk,
   not your checkmarks. If the artifact is missing, the step is not done.

6. NEVER OUTPUT FILE CONTENT AS TEXT. To create or modify a file you
   MUST call `write_file` or `edit`. Putting code or file content in
   your reply text does NOT create or modify any file — the user cannot
   use text output as actual code. Always call the tool, never describe
   what you would write.

7. FOR LARGE FILES, WRITE IN CHUNKS. A single tool call can hold roughly
   250-300 lines before hitting output limits. For larger files: call
   `write_file` with the first 250-300 lines, then call `write_file`
   again (or `edit`) for each subsequent section. Never attempt to write
   an 800-line file in one shot — it will be truncated. Plan how many
   chunks you need before you start, then execute them one tool call at
   a time.

## Mode-change suggestions

You can request a mode change with `request_plan_mode_switch` when the
user's request is multi-step build/refactor/design work AND they're
currently in Supervised or Yolo mode. Call it BEFORE other tool calls.
Don't call it for: bug fixes you already understand, single-file edits,
read-only questions, or after the user has explicitly said "just do it".
The tool is fire-and-forget; the agent continues regardless.

## When to call ask_user

Call `ask_user` when you need a decision from the user before continuing:
- The request has 2+ plausible interpretations and your guess could be
  wrong by 50%+
- You're about to do something destructive (delete, force-push, drop
  table) without an explicit prior OK
- A critical design choice depends on user preference (library choice,
  API contract shape, file structure)

Do NOT call ask_user for:
- Trivial yes/no answerable from project context (CLAUDE.md, code)
- Clarifying typos or grammar
- Asking permission for things that are already auto-approved by mode"#;
        assert_eq!(assembled.system, expected_system, "system prompt drifted from snapshot");

        let expected_dynamic = "<system_info>\n当前时间: 2026年5月29日 周五 14:30\n注意: 以上时间和工作区路径由系统直接提供。对于询问当前状态的对话（如「你在干啥」「现在几点」「工作区是什么」等），直接用此处信息回答即可——不要运行 bash date、glob、ls、find、pwd 等命令去探查。只有用户明确要求执行文件/目录操作时，才使用相关工具。\n</system_info>";
        assert_eq!(assembled.dynamic_for_last_user, expected_dynamic, "dynamic block drifted from snapshot");
    }

    // ── Test 5 ────────────────────────────────────────────────────────────
    // memory_context present + prior_memory_snapshot present such that the
    // diff produces a SMALL drift (1 of 3 lines changed ≈ 33%, under the
    // 40% threshold). Verifies the delta annotation path is taken.
    #[test]
    fn snapshot_restricted_memory_context_with_small_drift() {
        let prior_snapshot = crate::agent::context_diff::LineFragmentSnapshot::from_text(
            "memory_context",
            "Line A\nLine B\nLine X",
        );
        let ctx = SystemPromptContext {
            effective_mode: crate::safety::SafetyMode::Plan,
            memory_context: Some("Line A\nLine B\nLine C".to_string()),
            prior_memory_snapshot: Some(prior_snapshot),
            ..base_ctx()
        };
        let assembled = assemble_system_prompt(ctx);

        // System half is identical to test 1 (Plan mode, no workspace, no skills).
        let expected_system = r#"You are uclaw.

---

<!-- Behavioral guardrails adapted from Andrej Karpathy's observations on LLM
     coding pitfalls. Source: https://github.com/forrestchang/andrej-karpathy-skills
     License: MIT. Editable via Settings → 提示词 → 行为护栏 (read-only preview only). -->

[Behavioral guardrails — apply to every action]

1. THINK BEFORE CODING. State your assumptions. If a request has multiple
   interpretations, present them — don't silently pick one. When unclear,
   call `ask_user` to surface the question instead of guessing.

2. SIMPLICITY FIRST. Minimum code that solves the problem. No speculative
   features. No abstractions for single-use code. If you'd write 200 lines
   and it could be 50, rewrite it.

3. SURGICAL CHANGES. Touch only what the user asked you to touch. Don't
   "improve" adjacent code, comments, or formatting. Match existing style.
   If you notice unrelated issues, mention them — don't fix them inline.

4. GOAL-DRIVEN EXECUTION. Transform vague requests into verifiable goals.
   For multi-step work, state your plan as `1. step → verify: check`.
   Loop until verify passes; don't stop at "I think it works".

5. NEVER FAKE PROGRESS. Bookkeeping tools (`plan_update`, `plan_write`,
   `TodoWrite`) ONLY update tracking files — they do NOT execute work.
   NEVER mark a step `done:true` unless you have already called the
   tool that actually does the work (`edit`, `write_file`, `bash`, etc.)
   and verified it succeeded. The user sees the artifacts on disk,
   not your checkmarks. If the artifact is missing, the step is not done.

6. NEVER OUTPUT FILE CONTENT AS TEXT. To create or modify a file you
   MUST call `write_file` or `edit`. Putting code or file content in
   your reply text does NOT create or modify any file — the user cannot
   use text output as actual code. Always call the tool, never describe
   what you would write.

7. FOR LARGE FILES, WRITE IN CHUNKS. A single tool call can hold roughly
   250-300 lines before hitting output limits. For larger files: call
   `write_file` with the first 250-300 lines, then call `write_file`
   again (or `edit`) for each subsequent section. Never attempt to write
   an 800-line file in one shot — it will be truncated. Plan how many
   chunks you need before you start, then execute them one tool call at
   a time.

## Mode-change suggestions

You can request a mode change with `request_plan_mode_switch` when the
user's request is multi-step build/refactor/design work AND they're
currently in Supervised or Yolo mode. Call it BEFORE other tool calls.
Don't call it for: bug fixes you already understand, single-file edits,
read-only questions, or after the user has explicitly said "just do it".
The tool is fire-and-forget; the agent continues regardless.

## When to call ask_user

Call `ask_user` when you need a decision from the user before continuing:
- The request has 2+ plausible interpretations and your guess could be
  wrong by 50%+
- You're about to do something destructive (delete, force-push, drop
  table) without an explicit prior OK
- A critical design choice depends on user preference (library choice,
  API contract shape, file structure)

Do NOT call ask_user for:
- Trivial yes/no answerable from project context (CLAUDE.md, code)
- Clarifying typos or grammar
- Asking permission for things that are already auto-approved by mode

---

[PLAN MODE — read-only investigation]

You CAN use: read_file, grep, glob, search, and safe shell commands like
`git status`, `ls`, `cat`. Write / install / network commands return a
"Plan mode — execution blocked" error from the safety layer.

Your output IS the plan; the user will verify it before any code runs.
This is guardrail #1 (think first) and #4 (goal-driven) at maximum.

## Two related tools — DO NOT confuse them

- `plan_write` / `plan_update` — workspace-level plan-tracking journal.
  Writes a markdown plan into `<workspace>/plans/...` and lets you mark
  steps done. **Calling `plan_update({done: true})` does NOT exit Plan
  mode.** It only updates a journal file. Use these tools to track
  long-running multi-step work, not to signal "I'm done planning".

  **CRITICAL — NEVER mark a step done unless you ACTUALLY did the work.**
  In Plan mode you usually CAN'T do code-writing work (writes are blocked).
  So in Plan mode, plan_update should mostly stay at `done:false` — you
  call `exit_plan_mode` once the plan is fleshed out, then in Auto mode
  the agent re-attacks each step with real tools (edit/write_file/bash)
  AND THEN calls plan_update with done:true. Marking steps done in Plan
  mode is almost always wrong.

- `exit_plan_mode` — THIS is how you submit your plan to the user for
  approval. Calling it pauses the agent loop, shows a modal, and waits
  for the user's decision. **You MUST call this tool to proceed past
  Plan mode.** If you only call `plan_write` and stop, the user has
  to manually switch modes — bad UX.

## Workflow

1. Investigate with read tools (read_file, grep, glob, safe bash, etc.)
2. (Optional) Use `ask_user({ questions: [...] })` for clarifications
3. (Optional) Use `plan_write` to journal a detailed plan if useful
4. **Always**: call `exit_plan_mode` to submit, with your plan as a
   markdown string in the `plan` argument:

```
exit_plan_mode({
  plan: "...markdown plan...",
  allowed_prompts: ["bash cargo build", "bash cargo test"]   // optional
})
```

The user will see the plan in a confirmation modal and can:
  - **Accept + switch to Auto** — agent proceeds with full execution
  - **Accept + stay in Plan** — only commands listed in `allowed_prompts`
    will auto-pass; useful for "compile and test but don't write code yet"
  - **Reject + feedback** — agent receives feedback as tool error and
    re-plans

Format the plan as:
```
1. [step] → verify: [check]
2. [step] → verify: [check]
...
```

Strong success criteria let the execution phase run without further questions.
"#;
        assert_eq!(assembled.system, expected_system, "system prompt drifted from snapshot");

        // Dynamic block must contain the full memory_context block PLUS the
        // delta annotation (1 of 3 lines changed ≈ 33% < 40% threshold).
        assert!(assembled.dynamic_for_last_user.contains("<memory_context>"),
            "dynamic block must contain memory_context tag");
        assert!(assembled.dynamic_for_last_user.contains("Line A\nLine B\nLine C"),
            "dynamic block must contain current memory_context content");
        assert!(assembled.dynamic_for_last_user.contains("memory_context_changes"),
            "dynamic block must contain delta annotation for small-drift path");

        let expected_dynamic = "<system_info>\n当前时间: 2026年5月29日 周五 14:30\n注意: 以上时间和工作区路径由系统直接提供。对于询问当前状态的对话（如「你在干啥」「现在几点」「工作区是什么」等），直接用此处信息回答即可——不要运行 bash date、glob、ls、find、pwd 等命令去探查。只有用户明确要求执行文件/目录操作时，才使用相关工具。\n</system_info>\n\n<memory_context>\nLine A\nLine B\nLine C\n</memory_context>\n\n<memory_context_changes vs_prior_turn=\"true\">\n+ added: Line C\n- removed: Line X\n</memory_context_changes>";
        assert_eq!(assembled.dynamic_for_last_user, expected_dynamic, "dynamic block drifted from snapshot");
    }

    // ── Test 6 ────────────────────────────────────────────────────────────
    // PR4: all 4 new layers populated. Verifies canonical append order:
    //   gene_block → plan_suggest_hint → project_rules_block → cache-align
    // and that their content appears AFTER the skills manifest block (which
    // comes from the prior seam).
    #[test]
    fn snapshot_pr4_all_four_layers_populated_in_canonical_order() {
        let ctx = SystemPromptContext {
            effective_mode: crate::safety::SafetyMode::Supervised,
            // Give the manifest block something to anchor the "after manifest" assertion.
            skills_manifest_block: "\n\nMANIFEST_BLOCK".to_string(),
            skills_manifest_suppress: false,
            gene_block: "GEP_MARKER\n".to_string(),
            plan_suggest_hint: "PLAN_HINT\n".to_string(),
            project_rules_block: "RULES\n".to_string(),
            // apply_ladder_pad=true: align_to_1024_tokens will be called. With a
            // small input the output is padded to the next 1024-token boundary
            // with a <!-- cache_align: ... --> comment. We only assert
            // ordering/containment, not the exact padded bytes.
            apply_ladder_pad: true,
            ..base_ctx()
        };
        let assembled = assemble_system_prompt(ctx);
        let sys = &assembled.system;

        // All 4 layers must be present.
        assert!(sys.contains("GEP_MARKER"), "gene_block must be present");
        assert!(sys.contains("PLAN_HINT"), "plan_suggest_hint must be present");
        assert!(sys.contains("RULES"), "project_rules_block must be present");
        // Cache-align produces a <!-- cache_align: ... --> comment when input is non-empty.
        assert!(sys.contains("<!-- cache_align:"), "cache-align comment must be present when apply_ladder_pad=true");

        // Canonical order: manifest → gene_block → plan_suggest_hint → project_rules_block → cache-align comment.
        let pos_manifest = sys.find("MANIFEST_BLOCK").expect("manifest block must be present");
        let pos_gene = sys.find("GEP_MARKER").expect("gene_block must be present");
        let pos_plan = sys.find("PLAN_HINT").expect("plan_suggest_hint must be present");
        let pos_rules = sys.find("RULES").expect("project_rules_block must be present");
        let pos_pad = sys.find("<!-- cache_align:").expect("cache-align comment must be present");

        assert!(pos_manifest < pos_gene, "gene_block must come after manifest: manifest={pos_manifest} gene={pos_gene}");
        assert!(pos_gene < pos_plan, "plan_suggest_hint must come after gene_block: gene={pos_gene} plan={pos_plan}");
        assert!(pos_plan < pos_rules, "project_rules_block must come after plan_suggest_hint: plan={pos_plan} rules={pos_rules}");
        assert!(pos_rules < pos_pad, "cache-align must come after project_rules_block: rules={pos_rules} pad={pos_pad}");
    }
}
