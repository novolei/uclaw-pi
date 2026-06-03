# Memory Inspector UI — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. One implementer subagent per task; controller reviews at each task + PR boundary. Steps use checkbox (`- [ ]`).

**Goal:** A read-only inspector surfacing the agent's learned self-model (user_model + evolution history, reflections, daydreams + live `agent:daydream`) as one new "成长" tab in the Kaleidoscope `MemoryModule`.

**Architecture:** 4 thin backend read commands wrapping existing P5 store helpers (PR1) + a frontend tab mirroring `LearnedProfileTab` with a Jotai live-daydream atom (PR2). No migration. Spec: `docs/superpowers/specs/2026-06-03-memory-inspector-design.md`.

**Tech Stack:** Rust (Tauri v2) + React/TS (Jotai, shadcn/Tailwind). Branch `pi/memory-inspector` (has the spec commit).

**Verification:**
- BE: `cd src-tauri && cargo build 2>&1 | grep -E "^error"` (empty) · `cargo test --lib agent_memory` (pass) · warnings not increased.
- FE: `cd ui && npx tsc --noEmit 2>&1 | head` (clean) · `cd ui && npm test -- --run 2>&1 | tail -15` (pass).
- `Cargo.lock` NEVER staged · explicit-path `git add` only.

---

## File Structure

| File | PR | Responsibility |
|---|---|---|
| `src-tauri/src/commands/agent_memory.rs` | PR1 | NEW — 4 DTOs + `From<Row>` impls + 4 `#[tauri::command]` read fns |
| `src-tauri/src/commands/mod.rs` | PR1 | `pub mod agent_memory;` |
| `src-tauri/src/main.rs` | PR1 | register 4 commands in `generate_handler!` (~L964–1167) |
| `ui/src/lib/tauri-bridge.ts` | PR2 | 4 DTO types + 4 invoke exports |
| `ui/src/atoms/agent-atoms.ts` | PR2 | `daydreamEventsAtom` + `DaydreamEvent` |
| `ui/src/hooks/useGlobalAgentListeners.ts` | PR2 | `listen('agent:daydream')` block |
| `ui/src/views/Kaleidoscope/modules/Memory/useAgentMemory.ts` | PR2 | NEW hook (mirror `useLearnedProfile`) |
| `ui/src/views/Kaleidoscope/modules/Memory/AgentGrowthTab.tsx` | PR2 | NEW component (mirror `LearnedProfileTab`) |
| `ui/src/views/Kaleidoscope/modules/Memory/MemoryModule.tsx` | PR2 | register `'self'` tab |

---

## PR1 — backend read commands

### Task 1: `agent_memory.rs` DTOs + `From<Row>` impls (TDD)

**Files:** Create `src-tauri/src/commands/agent_memory.rs`; modify `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Confirm Row shapes.** `grep -nE "pub struct ReflectionRow|pub struct DaydreamRow|pub struct UserModelHistoryRow" src-tauri/src/memory_graph/reflection_service.rs` and confirm fields: `ReflectionRow{insight,confidence,created_at}`, `DaydreamRow{content,created_at}`, `UserModelHistoryRow{summary,replaced_at}` — all `pub` with `pub` fields. If any differs, adapt the `From` impls.

- [ ] **Step 2: Create the file** `src-tauri/src/commands/agent_memory.rs` with DTOs + `From` impls (commands come in Task 2):
```rust
//! Read-only IPC for the agent's learned self-model (P3–P5 memory). Thin wrappers
//! over the `reflection_service` store helpers — generation + consolidation live
//! in the ReflectionService; these only READ, for the MemoryModule "成长" tab.

use crate::memory_graph::reflection_service::{
    DaydreamRow, ReflectionRow, UserModelHistoryRow,
};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflectionDto {
    pub insight: String,
    pub confidence: f64,
    pub created_at: String,
}
impl From<ReflectionRow> for ReflectionDto {
    fn from(r: ReflectionRow) -> Self {
        Self { insight: r.insight, confidence: r.confidence, created_at: r.created_at }
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserModelDto {
    pub summary: String,
    pub updated_at: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaydreamDto {
    pub content: String,
    pub created_at: String,
}
impl From<DaydreamRow> for DaydreamDto {
    fn from(r: DaydreamRow) -> Self {
        Self { content: r.content, created_at: r.created_at }
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserModelHistoryDto {
    pub summary: String,
    pub replaced_at: String,
}
impl From<UserModelHistoryRow> for UserModelHistoryDto {
    fn from(r: UserModelHistoryRow) -> Self {
        Self { summary: r.summary, replaced_at: r.replaced_at }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflection_dto_maps_from_row() {
        let row = ReflectionRow { insight: "x".into(), confidence: 0.9, created_at: "t".into() };
        let dto: ReflectionDto = row.into();
        assert_eq!(dto.insight, "x");
        assert!((dto.confidence - 0.9).abs() < 1e-9);
        assert_eq!(dto.created_at, "t");
    }

    #[test]
    fn daydream_and_history_dto_map_from_row() {
        let d: DaydreamDto = DaydreamRow { content: "c".into(), created_at: "t".into() }.into();
        assert_eq!(d.content, "c");
        let h: UserModelHistoryDto =
            UserModelHistoryRow { summary: "s".into(), replaced_at: "t".into() }.into();
        assert_eq!(h.summary, "s");
        assert_eq!(h.replaced_at, "t");
    }
}
```

- [ ] **Step 3: Declare the module** — add `pub mod agent_memory;` to `src-tauri/src/commands/mod.rs` (next to `pub mod gep;` at ~L20, keep alphabetical-ish ordering if the file is ordered).

- [ ] **Step 4: Run the test** — `cd src-tauri && cargo test --lib agent_memory 2>&1 | tail -10`. Expect PASS (2 tests). If `ReflectionRow` etc. aren't constructible (private fields), that's a Row-shape mismatch — re-check Step 1.

- [ ] **Step 5: Build** — `cargo build 2>&1 | grep -E "^error"` empty. Note: the DTOs are `pub` and the `From` impls are used by the tests; the commands (Task 2) will use them too, so no dead-code warnings expected this task (if a transient `dead_code` appears on `UserModelDto`, it's because no command uses it YET — Task 2 fixes it; acceptable mid-PR but verify it's gone after Task 2).

- [ ] **Step 6: Commit:**
```bash
git add src-tauri/src/commands/agent_memory.rs src-tauri/src/commands/mod.rs
git commit -m "feat(agent_memory): DTOs + From<Row> impls for the memory inspector IPC (TDD)"
```

### Task 2: the 4 read commands + `generate_handler!` registration (build-green)

**Files:** Modify `src-tauri/src/commands/agent_memory.rs`, `src-tauri/src/main.rs`

- [ ] **Step 1: Add the 4 commands** to `agent_memory.rs` (after the DTOs, before `mod tests`). Each scopes the `state.db` std-Mutex guard tightly (lock → read → drop) and maps errors to `String` (Tauri command convention):
```rust
#[tauri::command]
pub async fn list_reflections(
    state: tauri::State<'_, crate::app::AppState>,
    limit: usize,
) -> Result<Vec<ReflectionDto>, String> {
    let rows = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        crate::memory_graph::reflection_service::recent_reflections(&conn, limit)
            .map_err(|e| e.to_string())?
    };
    Ok(rows.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn get_agent_user_model(
    state: tauri::State<'_, crate::app::AppState>,
) -> Result<Option<UserModelDto>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match conn.query_row(
        "SELECT summary, updated_at FROM user_model WHERE id = 'default'",
        [],
        |r| Ok(UserModelDto { summary: r.get(0)?, updated_at: r.get(1)? }),
    ) {
        Ok(dto) => Ok(Some(dto)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn list_daydreams(
    state: tauri::State<'_, crate::app::AppState>,
    limit: usize,
) -> Result<Vec<DaydreamDto>, String> {
    let rows = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        crate::memory_graph::reflection_service::recent_daydreams(&conn, limit)
            .map_err(|e| e.to_string())?
    };
    Ok(rows.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn list_user_model_history(
    state: tauri::State<'_, crate::app::AppState>,
    limit: usize,
) -> Result<Vec<UserModelHistoryDto>, String> {
    let rows = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        crate::memory_graph::reflection_service::recent_user_model_history(&conn, limit)
            .map_err(|e| e.to_string())?
    };
    Ok(rows.into_iter().map(Into::into).collect())
}
```
> If `state.db` isn't the field name or `AppState`'s path differs, grep an existing command (e.g. `get_memory_recall_config` in `tauri_commands.rs`) for the exact `tauri::State<...>` + `state.db.lock()` shape and match it.

- [ ] **Step 2: Register in `generate_handler!`** — `main.rs`, find the `generate_handler![` block (~L964) and the existing `uclaw_core::tauri_commands::get_memory_recall_config,` line (~L1167). Add 4 lines nearby:
```rust
            uclaw_core::commands::agent_memory::list_reflections,
            uclaw_core::commands::agent_memory::get_agent_user_model,
            uclaw_core::commands::agent_memory::list_daydreams,
            uclaw_core::commands::agent_memory::list_user_model_history,
```
> Match the exact path prefix the neighbors use (`uclaw_core::commands::gep::list_genes` is a model — confirm whether it's `uclaw_core::commands::...` or `crate::commands::...` in that file and match it).

- [ ] **Step 3: Impact + build** — `gitnexus_impact({target:"main", direction:"downstream"})` is not meaningful for a macro; instead just verify: `cargo build 2>&1 | grep -E "^error"` empty; `cargo test --lib agent_memory 2>&1 | grep "test result"` pass; `cargo build 2>&1 | grep -E "^warning: .*(never|unused)" | grep -i agent_memory` empty (all DTOs/commands now used).

- [ ] **Step 4: Commit:**
```bash
git add src-tauri/src/commands/agent_memory.rs src-tauri/src/main.rs
git commit -m "feat(agent_memory): 4 read commands (reflections/user_model/daydreams/history) + register in generate_handler!"
```

**→ PR1 complete.** Open PR with a `## Commits (bisectable)` table (Tasks 1–2). Manual smoke (optional): invoke `list_reflections` from devtools.

---

## PR2 — frontend "成长" tab

> Frontend tasks follow established patterns. **Read the named mirror file first**, then implement to match its structure/conventions. Exact contracts + integration points are given; match local style (imports, Tailwind classes) from the mirror.

### Task 1: bridge exports + live-daydream atom + listener (data plumbing)

**Files:** Modify `ui/src/lib/tauri-bridge.ts`, `ui/src/atoms/agent-atoms.ts`, `ui/src/hooks/useGlobalAgentListeners.ts`

- [ ] **Step 1: Bridge** — in `tauri-bridge.ts`, add the DTO types + 4 exports (match the existing one-liner `export const x = (): Promise<T> => invoke('cmd', args)` style; note camelCase fields from the backend's `rename_all`):
```ts
export interface ReflectionDto { insight: string; confidence: number; createdAt: string }
export interface UserModelDto { summary: string; updatedAt: string }
export interface DaydreamDto { content: string; createdAt: string }
export interface UserModelHistoryDto { summary: string; replacedAt: string }

export const listReflections = (limit = 20): Promise<ReflectionDto[]> =>
  invoke('list_reflections', { limit });
export const getAgentUserModel = (): Promise<UserModelDto | null> =>
  invoke('get_agent_user_model');
export const listDaydreams = (limit = 20): Promise<DaydreamDto[]> =>
  invoke('list_daydreams', { limit });
export const listUserModelHistory = (limit = 20): Promise<UserModelHistoryDto[]> =>
  invoke('list_user_model_history', { limit });
```

- [ ] **Step 2: Atom** — in `agent-atoms.ts`, mirror `proactiveLearningEventsAtom` (find it; it's an `atom<...[]>([])` ring buffer). Add:
```ts
export interface DaydreamEvent { content: string; createdAt: string }
export const daydreamEventsAtom = atom<DaydreamEvent[]>([]);
```

- [ ] **Step 3: Listener** — in `useGlobalAgentListeners.ts`, find the `startAgentListeners` body and the existing `listen<...>('agent:memory-recall', ...)` block (it shows the exact `store.set(...)` + `cleanupFns.push(await listen(...))` shape). Add an analogous block:
```ts
  cleanupFns.push(
    await listen<{ content: string; created_at?: string }>('agent:daydream', (e) => {
      const ev: DaydreamEvent = {
        content: e.payload.content,
        createdAt: e.payload.created_at ?? new Date().toISOString(),
      };
      store.set(daydreamEventsAtom, (prev) => [ev, ...prev].slice(0, 10));
    }),
  );
```
> Import `daydreamEventsAtom` + `DaydreamEvent` from `agent-atoms`. Match the file's exact `store` reference + `listen` import. (The backend emits `{ content, created_at }`.)

- [ ] **Step 4: Verify** — `cd ui && npx tsc --noEmit 2>&1 | head` clean. Commit:
```bash
git add ui/src/lib/tauri-bridge.ts ui/src/atoms/agent-atoms.ts ui/src/hooks/useGlobalAgentListeners.ts
git commit -m "feat(memory-inspector): bridge exports + daydreamEventsAtom + agent:daydream listener"
```

### Task 2: `useAgentMemory` hook + `AgentGrowthTab` component + tab registration

**Files:** Create `useAgentMemory.ts`, `AgentGrowthTab.tsx` (under `ui/src/views/Kaleidoscope/modules/Memory/`); modify `MemoryModule.tsx`

- [ ] **Step 1: Read the mirrors** — `ui/src/features/settings/hooks/useLearnedProfile.ts` (hook: useState loading/error/data + useEffect fetch + refresh) and `ui/src/features/settings/components/LearnedProfileTab.tsx` (component: header+refresh, Loader2 loading, error banner, grouped rows, empty state) and `ui/src/components/memory/FragmentCard.tsx` (card props/shape). Implement to match.

- [ ] **Step 2: Hook** `useAgentMemory.ts` — mirror `useLearnedProfile`: own `loading`, `error`, and the four results (`reflections`, `userModel`, `daydreams`, `history`); fetch all four in parallel on mount (`Promise.all([listReflections(), getAgentUserModel(), listDaydreams(), listUserModelHistory()])`); expose `refresh()`. Types from `tauri-bridge`.

- [ ] **Step 3: Component** `AgentGrowthTab.tsx` — mirror `LearnedProfileTab`'s shell (header with a refresh button + counts, `Loader2` while loading, error banner). Render **3 sections**:
  1. **用户模型** — if `userModel`, a card with `userModel.summary` + relative `updatedAt`; below it a collapsible "演化历史" rendering `history` (each `{summary, replacedAt}`) as a simple timeline (reuse `DailySummaryView` style or a minimal `<ol>`). Empty → "还没有形成用户模型 — 多聊几轮".
  2. **反思** — `reflections.map(...)` as `FragmentCard`-style rows: `insight`, a confidence badge (e.g. `Badge` with `Math.round(confidence*100)%`), relative `createdAt`. Empty → muted "暂无反思".
  3. **遐想** — merge **live** `useAtomValue(daydreamEventsAtom)` (prepended, deduped by `content`) above fetched `daydreams`; each a `FragmentCard`-style row (content + relative time). Empty → muted "agent 还没有遐想".
  Use existing relative-time + `Badge`/`Card` primitives the mirrors use. Keep it read-only (no buttons beyond refresh).

- [ ] **Step 4: Register the tab** in `MemoryModule.tsx`: add `'self'` to the `MemoryTab` union; add `{ value: 'self', label: '成长', icon: Brain }` to `TABS[]` (import `Brain` from `lucide-react`, or `Sparkles` if `Brain` is taken); add `{activeTab === 'self' && <AgentGrowthTab />}` to the content switch. Import `AgentGrowthTab`.

- [ ] **Step 5: Verify** — `cd ui && npx tsc --noEmit 2>&1 | head` clean; `cd ui && npm test -- --run 2>&1 | tail -15` (existing tests still pass). Commit:
```bash
git add ui/src/views/Kaleidoscope/modules/Memory/useAgentMemory.ts ui/src/views/Kaleidoscope/modules/Memory/AgentGrowthTab.tsx ui/src/views/Kaleidoscope/modules/Memory/MemoryModule.tsx
git commit -m "feat(memory-inspector): 成长 tab — user_model + history, reflections, daydreams (live)"
```

### Task 3: component render test

**Files:** Create `ui/src/views/Kaleidoscope/modules/Memory/AgentGrowthTab.test.tsx` (match the repo's test-file location convention — grep for an existing `*.test.tsx` near a component and mirror its placement/setup)

- [ ] **Step 1: Find a mirror test** — `find ui/src -name "*.test.tsx" | head` and read one that renders a component with mocked data, to copy the render/mock setup (vitest + @testing-library/react + how they mock `tauri-bridge` / `invoke`).

- [ ] **Step 2: Write the test** — mock the 4 bridge fns; assert: (a) a loading indicator shows before data resolves, (b) after resolve, the three section headings (用户模型 / 反思 / 遐想) render, (c) a mocked reflection's confidence badge text appears, (d) all-empty mocks → the empty-state copy renders. Mirror the setup from Step 1.

- [ ] **Step 3: Verify** — `cd ui && npm test -- --run AgentGrowthTab 2>&1 | tail -15` PASS; full `npm test -- --run` green; `npx tsc --noEmit` clean.

- [ ] **Step 4: Commit:**
```bash
git add ui/src/views/Kaleidoscope/modules/Memory/AgentGrowthTab.test.tsx
git commit -m "test(memory-inspector): AgentGrowthTab render — sections, confidence badge, empty state"
```

**→ PR2 complete.** Open PR with a `## Commits (bisectable)` table (Tasks 1–3). Manual test: open Kaleidoscope → Memory → 成长; chat past a daydream trigger and watch it live-prepend.

---

## Self-Review
- **Spec coverage:** read-only ✓ · 成长 tab in MemoryModule ✓ · 3 sections ✓ · 4 backend commands ✓ · live daydream listener (the deferred P4 tail) ✓ · no migration ✓.
- **Type consistency:** backend `rename_all = "camelCase"` ⇒ TS `createdAt/updatedAt/replacedAt` (matches bridge DTOs). `get_agent_user_model -> Option` ⇒ `Promise<UserModelDto | null>`. `From<Row>` field names match the confirmed Row shapes.
- **Adjacent edits (CLAUDE.md):** new Tauri commands registered in `generate_handler!` (PR1 Task 2 Step 2) — the easy-to-forget runtime step is explicit.
- **Placeholders:** backend has complete code; frontend gives exact contracts + integration points + mirror files (the legitimate pattern for convention-bound UI work).

## Execution Handoff
Subagent-Driven. PR1 (2 tasks, Rust) then PR2 (3 tasks, TS). Controller reviews build/tsc/tests + spec compliance at each task; opens PR1 after its Task 2, PR2 after its Task 3.
