# uClaw Project Reference

> Detailed project reference for any agent or contributor. Loaded on demand by
> `CLAUDE.md` via `@CONTEXT.md`. Behavior rules live in `BEHAVIOR.md`, not here.

---

## Project Overview

**uClaw** is an AI-powered desktop coworker built as a Tauri v2 application.
The Rust backend (`uclaw_core` crate) hosts the agent, LLM providers, MCP
integration, memory subsystems, and a local HTTP API. The React + Vite
frontend (`ui/`) builds into `static/` and is served by Tauri. A bundled
Python runtime (`src-tauri/pyembed/`) drives the **memU** memory service via a
JSON-RPC stdio bridge. A bundled Bun runtime + gbrain (`src-tauri/bunembed/`,
`src-tauri/gbrain-source/`) provides the primary durable knowledge layer.

The original migration target documented in `docs/uclaw-migration-plan.md`
mentions Svelte 5, but the implementation is React 18 + TypeScript with
Tailwind and Radix UI primitives — trust the code, not that doc.

**Strategic north star**: [`docs/adr/2026-05-20-uclaw-agent-platform-north-star.md`](docs/adr/2026-05-20-uclaw-agent-platform-north-star.md).
New agent / runtime / browser / memory / automation / team / cluster designs
should preserve the Agent OS v2 direction: local-first long-running work,
small Rust runtime kernel, `IntentSpec` / `TaskSpec` / `TaskEvent` contracts,
Context Fabric, Capability Mesh, World Projection, Hermes-aligned registries,
gbrain as primary long-term knowledge, harness-gated self-evolution.

**Pi Framework Convergence (ADR §20, 2026-05-26)**: UClaw's agent core is actively converging toward the Pi reference framework (`/Users/ryanliu/Documents/pi`). The four immediate adoption targets are: (1) `SteeringQueue + FollowUpQueue` dual interactive queues, (2) iterative compaction with `UPDATE_SUMMARIZATION_PROMPT` + Split-Turn recovery, (3) `SessionFileOps` persistent file-operations memory (9th StructuredFold axis), (4) `RollingTailBuffer` bash temp logging. Full design: `docs/superpowers/specs/2026-05-26-agent-framework-pi-upgrade-design.md`.

---

## Common Commands

Tauri orchestrates dev/build via `src-tauri/tauri.conf.json`, which calls the
`ui/` npm scripts itself. From `src-tauri/`:

- `cargo tauri dev` — runs `cd ../ui && npm run dev` (Vite at `:9527`) and starts the Rust app pointed at `devUrl`.
- `cargo tauri build` — runs `cd ../ui && npm run build` (outputs to `../static`) and produces a release bundle.
- `cargo build` / `cargo build --release` — Rust-only build of the `uclaw` binary and `uclaw_core` library.
- `cargo test [-- <filter>]` — runs Rust unit tests (defined inline with `#[cfg(test)]`).

From `ui/` (only when iterating on the frontend in isolation):
- `npm run dev` / `npm run build` / `npm run preview`
- `npm test -- --run` — Vitest suite (jsdom environment). Tests live next to the components they exercise: `*.test.tsx`. The setup file at `ui/src/test-utils/setup.ts` shims `localStorage` for `atomWithStorage` under jsdom — don't remove it.

Bootstrap of the embedded Python (required before first run if `src-tauri/pyembed/` is empty — gitignored):
- `./scripts/setup-python-env.sh` — downloads python-build-standalone (Python 3.13) and pip-installs `memu` (preferring a local checkout at `~/Documents/memU` if present) plus `fastembed`.
- `./scripts/setup-python-env.sh --optimize` — same, then strips `__pycache__`, tests, idle/turtle to shrink the bundle.
- `./scripts/setup-python-env.sh --clean` — wipes `pyembed/`.

Bootstrap of the embedded Bun + gbrain (gitignored):
- `./scripts/setup-bun-runtime.sh` — downloads the Bun static binary (~50MB per platform) to `src-tauri/bunembed/bun`. Honors `BUN_VERSION` env var.
- `./scripts/setup-gbrain-source.sh` — clones `garrytan/gbrain` to `src-tauri/gbrain-source/`, runs `bun install --production` against the bundled Bun, strips `.git`. Prereq: `setup-bun-runtime.sh` must have run first.

Install the git pre-commit hooks (after a fresh clone):
- `./scripts/install-git-hooks.sh` — sets `git config core.hooksPath scripts/git-hooks`. See `scripts/git-hooks/README.md` for what each hook enforces.

---

## Runtime Layout

On first launch the Rust side creates:
- `~/.uclaw/` — `config.json`, `llm_config.json`, `uclaw.db` (main SQLite), plus per-feature DBs (`memorization.db`, `proactive.db`).
- `~/Documents/workground/` — workspace root for artifacts/files exposed to the agent.

This section describes the runtime layout only. Code must not construct
`~/.uclaw` by hand with `dirs::home_dir()`; use `uclaw_utils_home` helpers so
tests, sandboxing, and platform-specific paths stay consistent.

A local HTTP API listens on `127.0.0.1:27270` (Axum, with WebSocket support).
Spun up in a dedicated thread with its own Tokio runtime in `main.rs`,
independent of Tauri's async runtime — keep that boundary in mind when adding
handlers.

---

## Backend Architecture (`src-tauri/src/`)

`main.rs` is intentionally thin: it builds `AppState`, spawns the HTTP server
thread, then drives a **phased boot** sequence inside
`tauri::async_runtime::spawn` against the `ServiceManager`. Stages are logged
with `[Stage 3]` (registration) and `[Stage 4]` (start). The
`WindowEvent::Destroyed` hook stops services and shuts down the memU client
synchronously on quit.

`AppState` (`app.rs`) is the central DI container managed via `tauri::Manager`.
It owns the SQLite connection, settings, `SessionManager`, `ProviderService`,
`SafetyManager`, `MemoryStore`, `MemoryGraphStore` (frozen — see ADR §11.2),
`SkillsRegistry`, `SharedMcpManager`, `ChannelManager`, the optional
`MemUClient`, the `InfraService` message bus, the `ServiceManager`, and a
`PendingApprovals` map.

Key module roles:

- **`agent/`** — agentic loop (`agentic_loop.rs`), tool dispatcher, sessions,
  teams (`agent/teams/`), and built-in tools (`tools/builtin/`: file, edit,
  search, shell, web, plan, self_eval) plus MCP and memU tool adapters.
  **Loop v1**: `check_signals → compress_context → before_llm → call_llm → handle_response → after_iteration`.
  **Loop v2 (Pi-aligned, in progress)**: dual-layer outer/inner loop, `TurnBoundaryDelegate`, `SteeringQueue` + `FollowUpQueue`, `TurnSnapshot` isolation — see `docs/superpowers/specs/2026-05-26-agent-framework-pi-upgrade-design.md`.
  **Cost capture** lives at `agent/dispatcher.rs::emit_turn_cost`.
  **Pi convergence modules** (landing across Sprint 1-3):
  `agent/steering.rs` (dual queues), `agent/compaction.rs` (iterative + split-turn),
  `agent/file_ops.rs` (SessionFileOps / 9th StructuredFold axis), `agent/tools/bash.rs` (RollingTailBuffer + temp logging).
  `agentic_loop.rs` is already large; keep it as orchestration and put new behavior in focused `agent/*` modules when possible.
- **`llm/` and `providers/`** — `llm/` provides the lower-level provider trait
  + `anthropic`/`openai` clients; `providers/` is the higher-level
  configuration/registry/service wrapping multiple providers with credential
  storage. **Streaming has tiered timeouts**: `connect_timeout=15s`,
  `STREAM_STALL_TIMEOUT=45s` per chunk, `COMPLETE_TIMEOUT=120s` overall.
- **`api/` + `local_api/`** — HTTP/WebSocket layer serving the local API on
  port 27270.
- **`mcp.rs`** — Model Context Protocol server management (add/remove/connect/restart, tool listing).
- **`skills.rs`** + `proactive/scenarios/skill_extraction.rs` — Skills are both **static** (declared in registry) and **learned**.
- **`memory.rs`** + `memory_graph/` (**FROZEN per ADR §11.2**) + `memu/` (Python bridge) + `gbrain/` (Bun bridge, primary durable knowledge per ADR §11.2).
- **`proactive/`** — background scenario runner.
- **`automation/`** — automation runtime, specs, service.
- **`observability/`** — metrics and tracing.
- **`harness/`** — evaluation harness for agent testing (≈ ADR §6 Harness Layer).
- **`services/` + `infra/`** — `ServiceManager` lifecycle manager and `InfraService` in-process message bus.
- **`safety/`** — `SafetyManager` enforces tool policies; risky tool calls go through `pending_approvals`.
- **`tauri_commands.rs`** — single flat module exposing every IPC command. Adding a new command requires both defining it here **and** listing it in the `invoke_handler!` macro in `main.rs`. This file is under normal code-change discipline: GitNexus impact for changed symbols, narrow scope, focused command-path tests, and fresh review only when the change is broad or risky. Keep it thin: move command logic into feature modules and leave `tauri_commands.rs` as request parsing, state lookup, and delegation whenever practical.
- **`cost_store.rs`** — per-turn cost persistence into `cost_records`. Best-effort.
- **`db/migrations.rs`** — embedded migrations run on every startup; each migration is idempotent.
- **`secrets/`** — credential management for provider API keys.

### Active migration registry

Track which V-number is claimed by which open PR before starting schema work.
Two PRs reusing the same V-number is the most common merge accident in this repo.

| V | What | Status |
|---|---|---|
| V1–V10 | Initial schema → V10 messages_fts (unicode61) | merged |
| V11 | trigram tokenizer for messages_fts + agent_turns_fts | merged (PR #33) |
| V12 | agent_messages_fts (trigram) + sync triggers + backfill | merged |
| V13 | cost_records + indexes | merged (PR #39) |
| V14 | tool_permission_rules + permission_audit_log | merged (PR #41) |
| V15 | agent_messages metrics columns (duration_ms, token counts, cost) | merged |
| V16 | persist 'default' workspace + heal orphan agent_sessions | merged (PR #75) |
| V17 | spaces.sort_order + spaces.attached_dirs + agent_sessions.attached_dirs | merged (PR #76) |
| V18 | agent_sessions.pinned_at | merged (PR #92) |
| V19 | spaces.skill_tags | merged |
| V20 | rewrite automation_specs + activities + migrate legacy TOML | merged |
| V21 | automation_subscriptions + automation_memory + automation_escalations | merged |
| V22 | automation_installed_skills + idx_aut_inst_skills_slug | merged (PR #160) |
| V23a | Marketplace cache (Phase 3a) | merged |
| V24 | automation_activities +session_id +report_artifacts_json -tool_calls_json; agent_sessions +archived_at | merged (PR #172) |
| V25 | marketplace_standalone_installs | merged (Phase 3b-γ) |
| V26 | conversations.archived + conversations.archived_at | merged |
| V27 | system_prompts table | merged |
| V28 | system_prompt_versions | merged |
| V29 | compaction support — `compacted` column + compaction_markers | merged |
| V30 | fragment_reviews + daily_summaries | merged |
| V31 | rebuild memory_fts with trigram tokenizer + backfill | merged |
| V32 | IM channel infrastructure | merged |
| V32b | automation_specs IM columns | merged |
| V33 | symphony_workflows + symphony_workflow_versions + symphony_runs + symphony_node_runs | merged (Symphony runtime) |
| V34 | plan_suggest_events + mode_suggest_overrides | merged (PR #185) |
| V35 | memory_edge_audit + wiki_artifacts + memory_health_findings | merged (Memory OS Foundation Phase 1) |
| V36 | (skipped — renumbered to V38 when Phase 7 claimed V37) | — |
| V37 | brain_sync_state — disk-mirror metadata for Memory OS Phase 7 markdown sync | merged (PR #193) |
| V38 | automation_chat_sessions(spec_id, identity_key, agent_session_id) | merged (PR #194) |
| V39 | user_profile_facets — openhuman-style stability-graded user profile facet store | merged (PR #199) |
| V40 | mcp_audit — env-redacted MCP audit log | merged (MCP completeness PR-5) |
| V41 | browser_task_runs + browser_task_steps + browser_task_memory | merged (Browser agent v2) |
| V42 | browser_task_checkpoints | merged (Browser agent v2) |
| V43 | Memory OS Cognitive Layer Phase 8.1 — 5 new tables | shipped empty + 7-row template seed; **PAUSED — see [ADR 2026-05-20](docs/adr/2026-05-20-gbrain-primary-freeze-l2-cognitive.md) (gbrain is primary)** |
| V44 | Memory OS L3 Engines RETAINED schema (per ADR 2026-05-20 §8) — 4 new tables | in progress |
| V45 | Memory OS L3 §4.12.3 RETAINED — `spaced_repetition_state` | in progress |
| V46 | Memory OS L3 §4.12.4 RETAINED — `drift_events` | in progress |
| V47 | Memory OS L3 §4.12.5 RETAINED — `triangulation_evidence` | in progress |
| V48 | task_events_rollout (M1-T5) | merged (PR #310) |
| V49 | cost_records.cached_input_tokens + reasoning_output_tokens (M1-T6) | merged (PR #313) |
| V50 | Halo automation_specs metadata (status / user_overrides_json / browser_login / uninstalled_at) | merged |
| V51 | memorization_queue + memorization_state shared schema | merged |
| V52 | agent_fold_baselines — per-session StructuredFold cache for Bundle 17-B `/compact` delta-rendered path | in progress (C1.1 PR-1) |
| V53 | living persona MVP — persona profiles, bond, journal, keepsakes, badges, candidates | in progress |
| V54 | persona_events — append-only Living Persona event ledger | in progress |
| V55 | session_tree + session_leaves — fork/rewind 谱系 | (this PR) |

**If you're adding a migration**: pick the next number after both merged AND open PRs. Update this table in your PR.

---

## Frontend Architecture (`ui/src/`)

- React 18 + TypeScript, Vite (port `9527`, strict), Tailwind, Radix UI, Jotai for state, `react-markdown` + `shiki` for rendering, `sonner` for toasts.
- `@/*` path alias maps to `ui/src/*` (see `vite.config.ts` and `tsconfig.json`).
- Build output goes to `../static`, which Tauri serves as `frontendDist`.
- Manual chunk splitting in `vite.config.ts`: `react`, `tauri`, `vendor`.
- Components are organized by feature (`agent/`, `chat/`, `artifacts/`, `automation/`, `memory/`, etc.); UI primitives live in `components/ui/`.
- State is managed via Jotai atoms (`atoms/` — 27+ atom files organized by feature).
- All backend interaction is via `@tauri-apps/api` `invoke()` against the commands listed in `tauri_commands.rs`. Lower-level IPC types are in `lib/tauri-bridge.ts`.
- **Theming**: 11 themes defined in `ui/src/styles/globals.css` as CSS variables. New components must use the theme tokens (`bg-popover`, `text-muted-foreground`) rather than hardcoded colors — hardcoded values break under warm-paper / qingye / forest-* themes.
- **Tests**: Vitest + React Testing Library + jsdom. `renderWithProviders` from `ui/src/test-utils/render.tsx`.

---

## Gotchas

- **FTS backfill.** When adding FTS coverage of a new table, don't forget `INSERT INTO …_fts(rowid, …) SELECT … FROM source WHERE rowid NOT IN (SELECT rowid FROM …_fts)`. Without it, search misses everything that pre-dates the migration.
- **CSP + providers.** Adding a new LLM provider requires updating both `providers/registry.rs` and the `connect-src` allow-list in `tauri.conf.json`'s CSP.
- **Embedded Python is gitignored.** Assume `src-tauri/pyembed/` is missing on a fresh checkout — run `scripts/setup-python-env.sh` before `cargo tauri dev`. If `MemUClient` fails to start, `AppState.memu_client` is `None` and memU-dependent features degrade gracefully.
- **Embedded Bun + gbrain are gitignored.** Run `scripts/setup-bun-runtime.sh` then `scripts/setup-gbrain-source.sh` before `cargo tauri dev`.
- **Chat-composer behavior change.** uClaw has **two parallel composers** that wrap the same `RichTextInput`: `ui/src/components/chat/ChatInput.tsx` (Chat mode) and `ui/src/components/agent/AgentView.tsx` (Agent mode). Any paste/drop/attachment/submit behavior change must be applied to **both** files.
