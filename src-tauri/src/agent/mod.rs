pub mod api;
pub mod agentic_loop;
pub mod anchor_state;
pub mod skeleton;
pub mod rule_context_builder;
// M1-T2c — RegularTask: SessionTask wrap of run_agentic_loop.
pub mod regular_task;
// M2-A pilot — BaselineBlock trait + 3-block registry.
pub mod baseline_blocks;
// M2-G pilot — StructuredFold 8-field compact representation.
pub mod compact;
// Pi Sprint 2 item 1 — CompactionState + structural split-turn cut-point detection.
pub mod compaction;
// Pi Sprint 2 item 2 — TurnSnapshot immutable per-turn config + NextTurnPatch + apply_patch.
pub mod turn;
// Pi Sprint 2 item 3 — SteeringQueue (drain-all) + FollowUpQueue (OneAtATime).
pub mod queues;
// Pi Sprint 1 — SessionFileOps persistent file memory (StructuredFold axis 10).
pub mod file_ops;
// M2-H L1 pilot — TruncationPolicy + per-handler budgets.
pub mod truncation;
// M2-H L2 pilot — ToolExposure + normalize_tool_schema.
pub mod tool_shaping;
// M2-H L6 pilot — orphan tool-call audit + "aborted" synthesis.
pub mod call_audit;
// M2-H L5 pilot — image stripping for image-blind providers.
pub mod image_policy;
// M2-H L7 pilot — Compaction state machine.
pub mod compression_state;
// M2-B pilot — ContextManager per-turn composition skeleton.
pub mod context_manager;
// M2-I pilot — Prompt caching policy (4 cache breakpoint placement).
pub mod cache_policy;
// M2-J pilot — TokenBudgetSnapshot UI backend contract.
pub mod token_budget;
// Slice 1 — Runtime telemetry collector (bridges agent loop to M2-J).
pub mod telemetry;
// M5 pilot — HookBus 13-event type skeleton.
pub mod hook_bus;
// M2-D pilot — Diff-based context re-injection.
pub mod context_diff;
// M1-T4b — opt-in rollout bridge for direct run_agentic_loop callsites.
pub mod rollout_integration;
pub mod code_rescue;
pub mod context;
pub mod dispatcher;
pub mod gbrain_prompt;
pub mod gep;
pub mod headless;
// Bundle 27-A — Heartbeat / stall detection / flight recorder.
pub mod heartbeat;
// Bundle 27-A — Reply recovery after unclean shutdown (uses flight record + 27-C's ProcessLock).
pub mod recovery;
pub mod history_window;
pub mod llm_stream;
pub mod memory_context;
pub mod mode_prompts;
pub mod mode_suggest;
pub mod mode_suggest_store;
pub mod persona;
pub mod plan_state;
pub mod retry;
pub mod session;
// Sprint 3 ③ — fork/rewind lineage storage (session_tree + session_leaves).
pub mod session_tree;
pub mod teams;
// P3.2 — ToolBudgetManager moved from harness/budget.rs (eval-only) to agent/ (production).
pub mod tool_budget;
pub mod tool_dispatch;
pub mod tool_families;
pub mod tools;
pub mod trajectory;
pub mod types;
// Tier 1.1 — per-conversation cancellation registry (PR1 of Tier 1+2+3 batch).
pub mod cancellation_registry;
