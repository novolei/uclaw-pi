# Memory Inspector UI — Design Spec

> **Status:** Approved (design gate passed 2026-06-03). Next: `writing-plans`.
> **Branch:** `pi/memory-inspector`.
> **Predecessors:** P1–P5 memory system (generation + refinement), all merged to main.

## 1. Motivation

P1–P5 built the full Agent-Native memory loop on the pi path: facts, reflections,
`user_model`, daydreams (generation) + consolidation/reflow (refinement). It all runs,
but it lives **only in logs + SQLite** — the user cannot see what the agent has learned
about them, whether reflections are accumulating, how their `user_model` reads, or that
the agent "daydreams". The P4 daydream pass even emits an `agent:daydream` event that
**no frontend code consumes** (confirmed: zero `listen('agent:daydream')` in `ui/`).

Per the 2026-06-03 direction decision, the next step is **make the memory system
visible**: a read-only inspector that surfaces the agent's learned self-model + activity,
and finally renders the live daydream event. This realizes the user-facing value of
P1–P5 and builds trust ("here's what I remember about you").

## 2. Goals / Non-goals

**Goals**
- A **read-only** inspector showing: current `user_model` + its evolution history,
  recent reflections, recent daydreams — plus **live** daydreams as they fire.
- Slot into the **existing** Kaleidoscope `MemoryModule` as one new tab (no new
  top-level navigation).
- Wire the dangling `agent:daydream` event into the frontend (the deferred P4 UI tail).

**Non-goals (this cut)**
- No interactivity — no delete/restore reflection, no "consolidate now" button, no
  `user_model` editing (decided: read-only MVP). These are documented future work.
- No new workspace-surface entry point (the inspector lives in Kaleidoscope `MemoryModule`,
  the established memory home). A workspace shortcut can come later.
- No toast / dock-pulse for daydreams (decided: in-tab live prepend only).
- No archived-reflection / consolidation-diff view beyond the `user_model_history`
  timeline (archived reflections stay backend-only for now).

## 3. Design decisions (locked)

| # | Decision | Choice |
|---|---|---|
| D1 | Interactivity | **Read-only MVP** |
| D2 | Live daydream surfacing | **In-tab live prepend** (`daydreamEventsAtom` ring buffer + one `listen('agent:daydream')`); no toast/pulse |
| D3 | Placement | **One new tab "成长" in `MemoryModule`** (existing 9 tabs are all old `memory_graph`; zero overlap) |
| D4 | Layout | One tab, **3 sections**: 用户模型 (user_model + history) · 反思 (reflections) · 遐想 (daydreams + live) |

## 4. Architecture

### Backend — 4 read commands (new module `commands/agent_memory.rs`)

Each command is a thin wrapper over an existing P5 store helper in
`memory_graph/reflection_service.rs` (`recent_reflections`, `get_user_model`,
`recent_daydreams`, `recent_user_model_history`). Borrow-safe (lock the std `Mutex`,
read, drop). Best-effort: a read error returns an empty list / `None`, never panics.

DTOs (serde, `rename_all = "camelCase"` for clean TS), constructed from the store Rows:

```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflectionDto { pub insight: String, pub confidence: f64, pub created_at: String }

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserModelDto { pub summary: String, pub updated_at: String }

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaydreamDto { pub content: String, pub created_at: String }

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserModelHistoryDto { pub summary: String, pub replaced_at: String }
```

Commands:
- `list_reflections(limit: usize) -> Vec<ReflectionDto>` — wraps `recent_reflections`
  (live only; the `archived_at IS NULL` filter is already in that helper).
- `get_agent_user_model() -> Option<UserModelDto>` — reads the singleton `user_model`
  row. `get_user_model` returns only the summary, so this command runs its own inline
  `SELECT summary, updated_at FROM user_model WHERE id = 'default'` (the table has
  `id, summary, updated_at`) to include the timestamp. `QueryReturnedNoRows -> None`.
  No new store helper needed.
- `list_daydreams(limit: usize) -> Vec<DaydreamDto>` — wraps `recent_daydreams`.
- `list_user_model_history(limit: usize) -> Vec<UserModelHistoryDto>` — wraps
  `recent_user_model_history`.

**Adjacent edits (CLAUDE.md):** register all 4 in the `generate_handler!` macro in
`main.rs` (around line 964); add `mod agent_memory;` to the `commands/` module
declaration. Forgetting the macro entry compiles but fails at runtime.

### Frontend — new `MemoryModule` tab + hook + component + live atom

**Bridge** (`ui/src/lib/tauri-bridge.ts`): 4 typed one-liner exports + their TS types
(`ReflectionDto`, `UserModelDto`, `DaydreamDto`, `UserModelHistoryDto`), e.g.
```ts
export const listReflections = (limit = 20): Promise<ReflectionDto[]> =>
  invoke('list_reflections', { limit });
```

**Live daydream atom** (`ui/src/atoms/agent-atoms.ts`): mirror `proactiveLearningEventsAtom`
— `export const daydreamEventsAtom = atom<DaydreamEvent[]>([])` (ring buffer, capped at
~10). `DaydreamEvent = { content: string; createdAt: string }`.

**Listener** (`ui/src/hooks/useGlobalAgentListeners.ts`, inside `startAgentListeners`):
add `listen<{content:string;created_at?:string}>('agent:daydream', e => { prepend to
daydreamEventsAtom, cap 10 })`, mirroring the existing `agent:memory-recall` /
proactive-learning blocks. Push the unlisten into `cleanupFns`.

**Hook** (`ui/src/views/Kaleidoscope/modules/Memory/useAgentMemory.ts`): mirror
`useLearnedProfile` — owns `loading`/`error` + the 4 fetches (`listReflections`,
`getAgentUserModel`, `listDaydreams`, `listUserModelHistory`) via `useState`+`useEffect`,
exposes a `refresh()`.

**Component** (`ui/src/views/Kaleidoscope/modules/Memory/AgentGrowthTab.tsx`): mirror
`LearnedProfileTab` structure — header with refresh + counts, `Loader2` loading, error
banner, empty states. Three sections:
1. **用户模型** — the `user_model` summary in a card (updated_at as relative time); an
   expandable "演化历史" listing `user_model_history` (each prior summary + `replaced_at`)
   as a `DailySummaryView`-style timeline.
2. **反思** — `reflections` as `FragmentCard`-style rows (insight + a confidence badge +
   relative `created_at`).
3. **遐想** — `daydreams`, with **live events from `daydreamEventsAtom` prepended** above
   the fetched history (deduped/merged by content+time), each a `FragmentCard`-style row.

**Tab registration** (`MemoryModule.tsx`): add `'self'` to the `MemoryTab` union, a
`{ value: 'self', label: '成长', icon: <Brain/Sparkles> }` entry in `TABS[]`, and an
`{activeTab === 'self' && <AgentGrowthTab/>}` branch in the content switch.

## 5. Data model

**No migration.** Read-only over the existing P5 tables (`reflections`, `user_model`,
`daydreams`, `user_model_history`). No schema change.

## 6. Testing

**Backend** (`cargo test --lib`): a unit test per command path is overkill (they're thin
wrappers over already-tested store helpers); instead, one focused test that the DTO
construction from a Row is correct (e.g. `ReflectionDto::from(row)` maps fields), against
an `:memory:` db seeded via the existing `apply_*_schema` + `insert_*` helpers. Build-green
covers the command wiring.

**Frontend** (`cd ui && npm test -- --run`, Vitest + jsdom): a render test for
`AgentGrowthTab` — mock the 4 bridge fns, assert (a) loading spinner first, (b) the three
section headers render, (c) an empty state when all four return empty, (d) a reflection's
confidence badge + a daydream row render from mocked data. Mirror existing component tests
under `ui/src/**/__tests__` or `*.test.tsx`.

**Type check:** `cd ui && npx tsc --noEmit` clean.

## 7. Files touched

| File | Side | Change |
|---|---|---|
| `src-tauri/src/commands/agent_memory.rs` | BE | NEW — 4 commands + 4 DTOs (user_model via inline SELECT) |
| `src-tauri/src/commands/mod.rs` | BE | `pub mod agent_memory;` (mods declared here, e.g. `pub mod gep;` L20) |
| `src-tauri/src/main.rs` | BE | register 4 commands in `generate_handler!` (~L964) |
| `ui/src/lib/tauri-bridge.ts` | FE | 4 typed exports + 4 DTO types |
| `ui/src/atoms/agent-atoms.ts` | FE | `daydreamEventsAtom` + `DaydreamEvent` type |
| `ui/src/hooks/useGlobalAgentListeners.ts` | FE | `listen('agent:daydream')` block |
| `ui/src/views/Kaleidoscope/modules/Memory/useAgentMemory.ts` | FE | NEW hook |
| `ui/src/views/Kaleidoscope/modules/Memory/AgentGrowthTab.tsx` | FE | NEW component |
| `ui/src/views/Kaleidoscope/modules/Memory/MemoryModule.tsx` | FE | register `'self'` tab |

## 8. Risks & mitigations

| Risk | Mitigation |
|---|---|
| New command not in `generate_handler!` → runtime "command not found" | Explicit checklist step + a manual smoke test; CLAUDE.md adjacent-edit callout. |
| `agent:daydream` payload shape mismatch (backend emits `content`+`created_at`) | Type the listener to the exact backend payload (`content`, `created_at`); the live atom stores `{content, createdAt}`. |
| Daydream fires once per ~100 turns → tab looks empty in normal use | The tab fetches `list_daydreams` history (not just live), so it shows past daydreams immediately; live prepend is additive. |
| `user_model` empty before first promotion → blank section | Explicit empty state ("还没有形成用户模型 — 多聊几轮"). |
| Kaleidoscope is a separate surface; discoverability | Out of scope this cut; noted as future (workspace entry point). |

## 9. PR plan (bisectable)

- **PR1 (backend)** — `commands/agent_memory.rs` (4 DTOs + 4 commands) + `mod` decl +
  `generate_handler!` registration + (maybe) `get_user_model_meta` helper + DTO-mapping
  test. Ships an invocable IPC surface. `cargo build`/`cargo test --lib` green.
- **PR2 (frontend)** — bridge exports + `daydreamEventsAtom` + `agent:daydream` listener
  + `useAgentMemory` hook + `AgentGrowthTab` component + `MemoryModule` tab registration +
  component render test. `tsc --noEmit` + `npm test` green. Depends on PR1's commands.

One branch per plan, one commit per task, `## Commits (bisectable)` table per PR.

## 10. Future (out of scope)

- Interactivity: delete/restore a reflection (`archived_at`), "consolidate now" button
  (manual `run_consolidation`), edit `user_model`.
- Archived-reflection / consolidation-diff view (what got merged away).
- Daydream toast / dock-pulse (the `memuConsolidatingAtom` pattern).
- Workspace-surface entry point for discoverability outside Kaleidoscope.
