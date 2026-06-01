# CLAUDE.md

Top-level entry file for Claude Code (Cowork, any IDE) working in uClaw.

The full multi-session behavior contract is in **`@BEHAVIOR.md`** — consult it
before non-trivial or policy-sensitive work. Detailed project reference
material is in **`@CONTEXT.md`**. The strategic baseline — the **Pi-lightweight product
philosophy** (which superseded the earlier "Agent OS v2" heavyweight positioning) — is
summarized inline just below; its source ADRs were removed in the `a11d57dc` repo cleanup.

**Product philosophy (2026-05-28)**: uClaw is a **Pi-style lightweight, pluggable,
domain-blind agent kernel** serving **everyday/office users + vibe-coding users**.
Kernel stays pure (stateless loop + one `AgentApi` handle + Pi `AgentHarness` layer);
domains/heavy features (Teams, World Projection, Evolution Factory) live **above** the
kernel as optional layers, never as loop branches. Plugins: one handle; third-party code
plugins via subprocess/RPC (MCP generalized); domains as capability presets. Memory:
modernize via openhuman ideas behind one `MemoryAdapter` (detailed gbrain↔openhuman
architecture deferred to a dedicated effort). Borrow Pi (kernel/plugins), openhuman
(memory), hermes (coding edits) — no language migration.

**Agent framework direction**: The 2026-05-27 pi-convergence gap audit's 5-phase
remediation is **fully landed** — one `AgentApi` handle (`agent/api/mod.rs`), one safety
chokepoint (`agent/tool_dispatch`), `CancellationToken` threaded to the flight points
(`agent/llm_stream.rs`, `agent/tool_dispatch`), and the dead skeleton + eval `harness/`
deleted (R5, `lib.rs`). **Treat that audit as resolved history, not a TODO.** The Pi
patterns (dual queues, iterative compaction + split-turn, FileOps) are also in. The next
strategic debt is the **memory layer** (8+ loosely-coordinated stores — kv / memory_graph /
gbrain / memu / memorization / learning → one `MemoryAdapter` / openhuman bucket-seal), a
deferred dedicated effort. (The 2026-05-2x gap-audit / agent-design specs + the strategy
ADRs were removed in the `a11d57dc` repo cleanup; use `@CONTEXT.md` + `@BEHAVIOR.md` for
surviving design context.)

Other agents (Codex, Copilot, …) get equivalent entry files
(`AGENTS.md`, `.github/copilot-instructions.md`) that point
to the same `BEHAVIOR.md` so behavior stays uniform across sessions.

---

## Milestone Work

If the user mentions "推进主线", "continue main line", M2/M3/M4/M5+ work,
C1/C2/C3, Bundle wire-up, milestone closeout, next slice, or queue-next work,
load the `uclaw-milestone-closed-loop` skill if it's available. (Its companion
doc `docs/agents/milestone-closed-loop.md` was removed in the `a11d57dc` cleanup;
`@BEHAVIOR.md` holds the multi-session contract.)

---

# Part 1 — Working Style

## Surfaces to check before assuming

- **Migration version numbers.** New schema work picks the next free integer in `src-tauri/src/db/migrations.rs` AND must coordinate with any open PR that's claimed a number — see *Active migration registry* in `@CONTEXT.md`. Two PRs reusing the same V-number is the most common merge accident in this repo.
- **The agent loop is pure Rust.** No Claude Code SDK / Anthropic SDK in the agent path. Frontend code that looks SDK-shaped (`SDKMessage`, `useSDKRenderer`, etc.) is Proma-migration leftover — verify it actually executes before relying on it.
- **Pi convergence modules**: new agent work should land in focused modules: `agent/steering.rs` (dual queues), `agent/compaction.rs` (iterative + split-turn), `agent/file_ops.rs` (SessionFileOps), `agent/tools/bash.rs` (RollingTailBuffer). Do not add message-injection logic to `SoftInterruptQueue` — it is deprecated in favor of the dual-queue design.
- **Two storage tables per domain.** Chat lives in `messages`; agent lives in `agent_messages` (the visible conversation) **and** `agent_turns` (per-tool-call breakdown). Search/index/migration work must touch the right one — a typical dev DB has ≫ rows in `agent_messages` and `agent_turns`, often 0 in `messages`.

## Match the codebase shape

When extending a feature that already has a flat shape (e.g. the existing `search_conversations` UNION-of-branches pattern), add another branch in the same file rather than introducing a new abstraction layer. uClaw favors flat enumeration over generic dispatchers — match it.

## Adjacent edits that look like scope creep but aren't

- **New Tauri command** → define in `tauri_commands.rs` AND register in the `invoke_handler!` macro in `main.rs`. Forgetting the macro entry compiles fine but fails at runtime.
- **New background service** → register in the `[Stage 3]` block in `main.rs`.
- **New built-in agent tool** → register in `agent/dispatcher.rs` and, if destructive, in `SafetyManager`.
- **Chat-composer behavior change** → uClaw has **two parallel composers** that wrap the same `RichTextInput`: `ui/src/components/chat/ChatInput.tsx` (Chat mode) and `ui/src/components/agent/AgentView.tsx` (Agent mode). Each owns its own `handlePasteFiles` / `handleDrop` / send wiring. Any paste / drop / attachment / submit behavior change must be applied to **both** files. The shared `RichTextInput` is a [PLACEHOLDER] textarea today — a real TipTap port is scheduled for W4 of the Proma preview port.

Call these out in the commit body so they're not mistaken for scope creep.

## Verification commands

- `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` — backend compile, errors only
- `cd src-tauri && cargo test --lib [filter]` — unit tests (inline `#[cfg(test)]`)
- `cd ui && npx tsc --noEmit 2>&1 | head -10` — TS check
- `cd ui && npm test -- --run 2>&1 | tail -10` — Vitest, jsdom

Bisectability: one logical change per commit. Match the plans in `docs/superpowers/plans/*.md`.

## Workflow

Use risk-scaled planning from `BEHAVIOR.md`: full
`superpowers:brainstorming` → `writing-plans` →
`subagent-driven-development` for high-blast-radius work, and a lightweight
inspect → edit → verify loop for small reversible docs, tests, and hotfixes.
When a full plan is needed, put it in
`docs/superpowers/plans/<feature>.md`.

PR shape: one branch per plan, one commit per plan task, one PR with a `## Commits (bisectable)` table.

### Skill entry points

Beyond the superpowers loop, reach for these at the matching stage:

- **Entering ideation** — `to-prd` (PRD on GitHub) or `grill-me` (stress-test a half-formed plan).
- **Aligning with domain** — `grill-with-docs` challenges a plan against `@CONTEXT.md`.
- **Investigation** — `zoom-out` for system-level context on `automation/`, `memu/`, `proactive/`, `memory_graph/`. `prototype` for throwaway design validation.
- **Planning fan-out** — `to-issues` slices a plan into independently-grabbable GitHub issues.
- **Refactor pass** — `improve-codebase-architecture` hunts consolidation / testability wins.
- **Inbox** — `triage` walks incoming GitHub issues through a state machine.
- **Comms** — `handoff` compacts the current conversation; `caveman` switches to ultra-compressed style.

Overlaps: prefer `superpowers:test-driven-development` over `tdd`, `superpowers:systematic-debugging` over `diagnose`, `superpowers:writing-skills` over `write-a-skill` — unless the mattpocock variant's tighter ritual is clearly the better fit.

## Agent skills

### Issue tracker

GitHub Issues live on `novolei/uclaw-pi` (the `gh` default in this repo). The
`docs/agents/issue-tracker.md` + `triage-labels.md` playbooks were removed in the
`a11d57dc` cleanup; the repo currently carries only the default GitHub labels
(`bug`, `enhancement`, `documentation`, …).

### Domain docs

Single-context repo: the design context lives in `@CONTEXT.md` + `@BEHAVIOR.md`
(the `docs/adr/` ADRs + `docs/agents/domain.md` were removed in `a11d57dc`).

## Real bugs found mid-task

If you discover a bug outside the current task's scope with a confident root cause and a low-risk fix, spin it off as its own small PR — don't fold it in (scope creep + bisectability loss) and don't leave it for later (it'll get forgotten). If the root cause isn't clear, surface it in your status report rather than patching symptoms.

---

# Quick links

- **Behavior spec (canonical multi-session contract)** → `@BEHAVIOR.md`
- **Project reference (architecture, build, migration registry)** → `@CONTEXT.md`
- **Strategic baseline (Pi-lightweight product philosophy)** → the *Product philosophy* section above (its source ADRs + the pi-convergence gap-audit spec were removed in the `a11d57dc` cleanup; the audit is resolved — see *Agent framework direction* above)
- **License & derivation procedure** → `LICENSE`, `NOTICE`
- **Pre-commit hooks (block memory_graph::write, dirs::home_dir for .uclaw, missing SPDX)** → `scripts/git-hooks/README.md`
- **Other IDE entry files** → `AGENTS.md` (Codex), `.github/copilot-instructions.md` (Copilot)

<!-- gitnexus:start -->
<!-- gitnexus:keep -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **uclaw-new** (38998 symbols, 64970 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/uclaw-new/context` | Codebase overview, check index freshness |
| `gitnexus://repo/uclaw-new/clusters` | All functional areas |
| `gitnexus://repo/uclaw-new/processes` | All execution flows |
| `gitnexus://repo/uclaw-new/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
