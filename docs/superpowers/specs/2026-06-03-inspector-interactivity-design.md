# Memory Inspector Interactivity (成长 tab v2) — Design Spec

> **Status:** Approved (design + scope confirmed 2026-06-03). Next: `writing-plans`.
> **Branch:** `pi/inspector-interactivity`.
> **Predecessor:** the read-only inspector (#56). **No overlap with the parallel S0 work**
> (S0 = `providers/service.rs` + `memory_graph/memory_os_llm.rs`; this = `agent_memory.rs`
> + a tiny `reflection_service.rs` field add + frontend).

## 1. Motivation

A data assessment of the live `uclaw.db` (2026-06-03) found the read-only inspector
surfaced a real problem it couldn't fix: `user_model` says *"30-year-old product manager
at an internet company"* — **wrong** (the user is an engineer). Root cause: a **stored
fact** `memory_nodes(kind='user_profile')` = *"The user works as a product manager at an
internet company"*. Promotion/consolidation faithfully ground in it, so **5 re-grounds
never removed it** — it regrows from the bad fact each time. The prompt is fine; the fact
is wrong.

So the fix is **fact-level**, and the inspector is exactly where to do it: let the user
**see and prune wrong facts**, then **force a re-ground**. This also subsumes the
"consolidation quality" concern — consolidation is working; the upstream fact was wrong.

## 2. Goals / Non-goals

**Goals (approved scope)** — upgrade the 成长 tab from read-only to:
1. **认知/事实 section** — show the raw profile facts (`memory_nodes kind='user_profile'`)
   with a **delete** action (the drift-fix path).
2. **立即整合 button** — manually trigger a re-ground + consolidation so a corrected fact
   set immediately produces a corrected `user_model`.
3. **Delete / restore reflection** — soft-delete (`archived_at`) a bad reflection, with a
   "show archived" toggle to restore.

**Non-goals (this cut)** — no edit-user_model (a direct edit is overwritten by the next
re-ground unless the bad fact is also deleted, so deleting the fact + 立即整合 is the
correct path); no delete-daydream; no facet editing (facets observed accurate).

## 3. Architecture

### Backend

**`reflection_service.rs` (tiny):** add `id` to `ReflectionRow` + the `recent_reflections`
SELECT (the prompt-injection path reads only `.insight`, so this is harmless there). This
lets the inspector address reflections for archive/restore.

**`agent_memory.rs` — `ReflectionDto` gains `id`**; 4 new commands (the first WRITE
commands in this module — keep them best-effort + explicit):

- `list_profile_facts() -> Vec<ProfileFactDto>` — `SELECT id, title, created_at FROM
  memory_nodes WHERE kind='user_profile' AND title IS NOT NULL AND title != '' ORDER BY
  created_at DESC`. `ProfileFactDto { id, title, created_at(→RFC3339-Z via to_iso_utc) }`.
- `archive_reflection(id: String) -> Result<(), String>` — `UPDATE reflections SET
  archived_at = datetime('now') WHERE id = ?1 AND archived_at IS NULL`.
- `restore_reflection(id: String) -> Result<(), String>` — `UPDATE reflections SET
  archived_at = NULL WHERE id = ?1`.
- `trigger_memory_refresh() -> Result<(), String>` — fire-and-forget spawn of
  `run_promotion(&state)` then `run_consolidation(&state)` (both `pub`, best-effort, own
  their gates). Returns immediately (the passes are async + LLM-bound); the frontend
  refetches after a short delay.

**Reuse (no new command):** fact deletion uses the existing `memory_graph_delete_node`
(`tauri_commands.rs:3198`, frontend `memoryGraphDeleteNode`) — a profile fact IS a
`memory_node`. Confirm its input shape and reuse it.

**`list_reflections`** now also returns archived rows when asked: add an
`include_archived: bool` arg (default false) so the "show archived" toggle can fetch them;
the DTO already carries `archivedAt` once we add it (`ReflectionDto.archived_at:
Option<String>`).

Register the 4 new commands in `generate_handler!` (`main.rs`).

### Frontend (`AgentGrowthTab.tsx` + `tauri-bridge.ts`)

- **Bridge:** `listProfileFacts`, `archiveReflection`, `restoreReflection`,
  `triggerMemoryRefresh` + reuse `memoryGraphDeleteNode`. `ProfileFactDto` + `id`/
  `archivedAt` on `ReflectionDto`.
- **认知/事实 section** (new, above 用户模型): `listProfileFacts()` rows, each with a
  trash button → `memoryGraphDeleteNode({id})` → refetch. A short caption: "Agent 学到的
  关于你的事实 — 删掉不准确的".
- **立即整合 button** in the header (next to 刷新): `triggerMemoryRefresh()` → show a
  spinner ~3s → refetch (re-ground is LLM-bound). Tooltip: "重新蒸馏 user_model + 整合反思".
- **反思 section:** each reflection row gets a trash button → `archiveReflection(id)` →
  refetch. A "显示已归档" toggle → `listReflections(20, true)` → archived rows render with a
  restore button → `restoreReflection(id)` → refetch.
- Destructive actions: fact-delete + reflection-archive are low-risk (soft-delete /
  reversible-by-restore for reflections; fact-delete is a real delete → a lightweight
  inline confirm or an undo toast is enough; keep it simple — a confirm on fact-delete).

`useAgentMemory` gains the mutations (or a thin `useAgentMemoryMutations`) + re-exposes
`refresh` after each.

## 4. Data model

No migration. Reuses existing tables (`reflections.archived_at` from V59, `memory_nodes`,
`user_model`). The only schema-adjacent change is `ReflectionRow`/`Dto` gaining `id` +
`archived_at` (read-only field surfacing, no DDL).

## 5. Testing

- BE (`cargo test --lib agent_memory`): `ProfileFactDto`/`ReflectionDto` mapping incl.
  `id`; `archive_reflection` then `recent_reflections` excludes it, `restore_reflection`
  brings it back (round-trip against `:memory:` via the existing `apply_*` helpers).
- FE (`npm test AgentGrowthTab`): the 3 new affordances render (delete-fact button,
  立即整合 button, reflection trash button); mock the bridge; assert a delete calls the
  right bridge fn.
- `tsc --noEmit` clean; `cargo build` 0 errors.

## 6. Files touched

| File | Change |
|---|---|
| `src-tauri/src/memory_graph/reflection_service.rs` | `ReflectionRow.id` + `recent_reflections` SELECT id |
| `src-tauri/src/commands/agent_memory.rs` | `ReflectionDto.id`+`archived_at`; `ProfileFactDto`; 4 commands; `list_reflections(include_archived)` |
| `src-tauri/src/main.rs` | register 4 commands in `generate_handler!` |
| `ui/src/lib/tauri-bridge.ts` | 4 exports + `ProfileFactDto` + `ReflectionDto` id/archivedAt |
| `ui/src/views/Kaleidoscope/modules/Memory/useAgentMemory.ts` | facts + mutations + refetch |
| `ui/src/views/Kaleidoscope/modules/Memory/AgentGrowthTab.tsx` | 认知/事实 section, 立即整合 button, reflection delete/restore |
| `ui/src/views/Kaleidoscope/modules/Memory/AgentGrowthTab.test.tsx` | extend for the new affordances |

## 7. Risks

| Risk | Mitigation |
|---|---|
| Deleting a `memory_node` has side effects (graph edges) | Reuse the existing `memory_graph_delete_node` (already handles node deletion cleanly for the nebula tab). |
| `trigger_memory_refresh` spawns LLM work (cost) | Best-effort, budget-gated (same gates as the turn-triggered passes); a user-initiated single refresh is bounded. |
| Reflection archive needs an id the DTO lacked | Add `id` to `ReflectionRow`/`Dto` (harmless to the injection path, which reads `.insight`). |
| Concurrent edit while a pass runs | All writes are simple UPDATE/DELETE; consolidation is best-effort and re-reads live state. |

## 8. PR plan (bisectable)

- **PR1 (backend):** `ReflectionRow.id`; `agent_memory` DTOs (`id`/`archived_at`/
  `ProfileFactDto`) + 4 commands + `include_archived` + registration. TDD the DTO/round-trip.
- **PR2 (frontend):** bridge + `useAgentMemory` mutations + `AgentGrowthTab` affordances +
  test. Combined into one feature PR (as with the inspector).

(Or one combined feature PR, mirroring the read-only inspector's single-PR decision.)

## 9. Future

- Edit user_model (paired with fact pruning).
- Facet editing.
- Undo toast for fact deletion.
- A "why does the agent think X?" trace (fact → reflection → user_model provenance).
