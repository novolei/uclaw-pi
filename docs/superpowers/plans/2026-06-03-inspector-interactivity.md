# Memory Inspector Interactivity — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. One implementer subagent per task; controller reviews at each task. Steps use checkbox.

**Goal:** Upgrade the read-only 成长 tab to manage memory: show + delete profile facts, 立即整合 (re-ground), delete/restore reflections. **One combined feature PR** (branch `pi/inspector-interactivity`, has the spec commit).

**Architecture:** New WRITE commands in `agent_memory.rs` (reusing `memory_graph_delete_node` for facts, `run_promotion`/`run_consolidation` for refresh) + frontend affordances in `AgentGrowthTab`. **No `reflection_service.rs` / `service.rs` / `memory_os_llm.rs` changes** — `list_reflections` does its own query (decoupled from the injection-path `recent_reflections`), so zero overlap with the parallel S0 work. No migration.

**Verify:** BE `cargo build 2>&1|grep ^error` empty · `cargo test --lib agent_memory` pass. FE `npx tsc --noEmit` (no NEW errors) · `npm test -- --run AgentGrowthTab` pass. Cargo.lock never staged.

---

## Task 1 (BE): `list_reflections` (id + archived) + `ProfileFactDto`/`list_profile_facts` (TDD)

**File:** `src-tauri/src/commands/agent_memory.rs`

- [ ] **Step 1** — `ReflectionDto` gains `pub id: String` and `pub archived_at: Option<String>`. Since `list_reflections` will now do its own query (to return `id` + archived rows), **remove** the `impl From<ReflectionRow> for ReflectionDto` and the `reflection_dto_maps_from_row` test (replaced by a round-trip test below). Keep the `Daydream`/`UserModelHistory` From impls.

- [ ] **Step 2** — rewrite `list_reflections` with an `include_archived` arg + its own query (apply `to_iso_utc` to both timestamps):
```rust
#[tauri::command]
pub async fn list_reflections(
    state: tauri::State<'_, crate::app::AppState>,
    limit: usize,
    include_archived: Option<bool>,
) -> Result<Vec<ReflectionDto>, String> {
    let include = include_archived.unwrap_or(false);
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let sql = if include {
        "SELECT id, insight, confidence, created_at, archived_at FROM reflections \
         ORDER BY created_at DESC LIMIT ?1"
    } else {
        "SELECT id, insight, confidence, created_at, archived_at FROM reflections \
         WHERE archived_at IS NULL ORDER BY created_at DESC LIMIT ?1"
    };
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![limit as i64], |r| {
            Ok(ReflectionDto {
                id: r.get(0)?,
                insight: r.get(1)?,
                confidence: r.get(2)?,
                created_at: to_iso_utc(r.get::<_, String>(3)?),
                archived_at: r.get::<_, Option<String>>(4)?.map(to_iso_utc),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}
```

- [ ] **Step 3** — add `ProfileFactDto` + `list_profile_facts` (the bad "product manager" fact lives in `memory_nodes(kind='user_profile')`):
```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileFactDto { pub id: String, pub title: String, pub created_at: String }

#[tauri::command]
pub async fn list_profile_facts(
    state: tauri::State<'_, crate::app::AppState>,
) -> Result<Vec<ProfileFactDto>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, title, created_at FROM memory_nodes \
         WHERE kind = 'user_profile' AND title IS NOT NULL AND title != '' \
         ORDER BY created_at DESC LIMIT 100",
    ).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok(ProfileFactDto {
            id: r.get(0)?, title: r.get(1)?, created_at: to_iso_utc(r.get::<_, String>(2)?),
        }))
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}
```
> Confirm `memory_nodes` columns are `id, kind, title, created_at` (grep the V-migration / `store.rs`). Adjust if the column is `name` not `title` (the daydream seed query used `title`, so `title` is correct).

- [ ] **Step 4** — TDD test (in `#[cfg(test)] mod tests`, against `:memory:` with `apply_reflections_schema` which already has `archived_at`):
```rust
    #[test]
    fn list_reflections_query_returns_id_and_archived_flag() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::memory_graph::reflection_service::apply_reflections_schema(&conn);
        use crate::memory_graph::reflection_service::insert_reflection;
        insert_reflection(&conn, "r1", "live one", 0.9, 0).unwrap();
        insert_reflection(&conn, "r2", "archived one", 0.8, 0).unwrap();
        conn.execute("UPDATE reflections SET archived_at = datetime('now') WHERE id='r2'", []).unwrap();
        // live-only
        let mut stmt = conn.prepare("SELECT id, archived_at FROM reflections WHERE archived_at IS NULL").unwrap();
        let live: Vec<(String, Option<String>)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().flatten().collect();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].0, "r1");
        assert!(live[0].1.is_none());
    }
```
(This validates the query shape the command relies on; the command itself needs `AppState` so it's build-green.)

- [ ] **Step 5** — `cargo test --lib agent_memory` pass; `cargo build 2>&1|grep ^error` empty. Commit:
```bash
git add src-tauri/src/commands/agent_memory.rs
git commit -m "feat(agent_memory): list_reflections returns id+archived (+include_archived) + list_profile_facts"
```

## Task 2 (BE): mutation commands + register (build-green)

**Files:** `src-tauri/src/commands/agent_memory.rs`, `src-tauri/src/main.rs`

- [ ] **Step 1** — add 3 commands:
```rust
#[tauri::command]
pub async fn archive_reflection(
    state: tauri::State<'_, crate::app::AppState>, id: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE reflections SET archived_at = datetime('now') WHERE id = ?1 AND archived_at IS NULL",
        rusqlite::params![id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn restore_reflection(
    state: tauri::State<'_, crate::app::AppState>, id: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE reflections SET archived_at = NULL WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Fire-and-forget re-ground (run_promotion: facts → user_model, fixes drift after a bad
/// fact is deleted) + consolidation (dedup, gated). Returns immediately; the frontend
/// refetches after a short delay.
#[tauri::command]
pub async fn trigger_memory_refresh(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn(async move {
        use tauri::Manager;
        if let Some(state) = app.try_state::<crate::app::AppState>() {
            crate::memory_graph::reflection_service::run_promotion(&state).await;
            crate::memory_graph::reflection_service::run_consolidation(&state).await;
        }
    });
    Ok(())
}
```
> `run_promotion`/`run_consolidation` are `pub async fn (&AppState)` (confirmed L581/L671). `trigger_memory_refresh` takes `AppHandle` (not `State`) so the spawned task can `try_state` without holding a non-Send guard across the await.

- [ ] **Step 2** — register the new commands in `main.rs` `generate_handler!` (next to the existing `agent_memory::*` lines ~L1231): `archive_reflection`, `restore_reflection`, `trigger_memory_refresh`, `list_profile_facts`.

- [ ] **Step 3** — `cargo build 2>&1|grep ^error` empty; `cargo test --lib agent_memory` pass; no new `agent_memory` warnings. `gitnexus_detect_changes()`. Commit:
```bash
git add src-tauri/src/commands/agent_memory.rs src-tauri/src/main.rs
git commit -m "feat(agent_memory): archive/restore reflection + trigger_memory_refresh commands + register"
```

## Task 3 (FE): bridge + `AgentGrowthTab` affordances + test

**Files:** `tauri-bridge.ts`, `useAgentMemory.ts`, `AgentGrowthTab.tsx`, `AgentGrowthTab.test.tsx`

- [ ] **Step 1 — bridge** (`ui/src/lib/tauri-bridge.ts`): add `ProfileFactDto { id; title; createdAt }`; extend `ReflectionDto` with `id: string` + `archivedAt: string | null`; exports:
```ts
export const listProfileFacts = (): Promise<ProfileFactDto[]> => invoke('list_profile_facts');
export const archiveReflection = (id: string): Promise<void> => invoke('archive_reflection', { id });
export const restoreReflection = (id: string): Promise<void> => invoke('restore_reflection', { id });
export const triggerMemoryRefresh = (): Promise<void> => invoke('trigger_memory_refresh');
```
Update `listReflections` signature: `(limit = 20, includeArchived = false) => invoke('list_reflections', { limit, includeArchived })`. Reuse the existing `memoryGraphDeleteNode({ node_id })` for fact deletion (already exported).

- [ ] **Step 2 — hook** (`useAgentMemory.ts`): add `facts: ProfileFactDto[]` (from `listProfileFacts()`), fetch it in the `Promise.all`. Add a `showArchived` state + when true pass `true` to `listReflections`. Expose mutation helpers that call the bridge then `refresh()`: `deleteFact(id)` (→ `memoryGraphDeleteNode({node_id:id})`), `archiveRefl(id)`, `restoreRefl(id)`, `refreshMemory()` (→ `triggerMemoryRefresh()` then, after ~3s, `refresh()`), and `toggleArchived()`.

- [ ] **Step 3 — component** (`AgentGrowthTab.tsx`): READ the current file first.
  - Header: add a **立即整合** button next to 刷新 → `refreshMemory()` (spinner ~3s, tooltip "重新蒸馏 user_model + 整合反思").
  - New **认知/事实** section above 用户模型: caption "Agent 学到的关于你的事实 — 删掉不准确的"; each `facts` row = title + relative time + a trash icon button → confirm → `deleteFact(id)`. Empty → muted "暂无事实".
  - **反思** section: each row gets a trash button → `archiveRefl(id)`. Add a "显示已归档" toggle (`toggleArchived`); archived rows render dimmed with a restore button → `restoreRefl(id)`. (Distinguish archived via `r.archivedAt != null`.)
  - Reuse the existing `InfoCard`/`Badge`/`relativeTime`/lucide icons (`Trash2`, `RefreshCw`, `Sparkles`). Keep confirms lightweight (window.confirm or an inline state).

- [ ] **Step 4 — test** (`AgentGrowthTab.test.tsx`): extend — mock the new bridge fns; assert the 立即整合 button + a fact row's delete button + a reflection trash button render; assert clicking the fact delete calls `memoryGraphDeleteNode`. Mirror the existing mock setup.

- [ ] **Step 5** — `npx tsc --noEmit 2>&1 | grep -iE "AgentGrowthTab|useAgentMemory|tauri-bridge"` empty; `npm test -- --run AgentGrowthTab` pass. Commit:
```bash
git add ui/src/lib/tauri-bridge.ts ui/src/views/Kaleidoscope/modules/Memory/useAgentMemory.ts ui/src/views/Kaleidoscope/modules/Memory/AgentGrowthTab.tsx ui/src/views/Kaleidoscope/modules/Memory/AgentGrowthTab.test.tsx
git commit -m "feat(memory-inspector): 成长 tab interactivity — fact delete, 立即整合, reflection archive/restore"
```

**→ Done.** Open one feature PR.

## Self-Review
- Scope: facts show+delete ✓ · 立即整合 ✓ · reflection archive/restore ✓ (edit-user_model / daydream-delete excluded per decision).
- Decoupling: `list_reflections` own query → no `reflection_service.rs` change → zero S0 overlap. ✓
- Reuse: `memory_graph_delete_node` (facts), `run_promotion`/`run_consolidation` (refresh). ✓
- Type consistency: `ReflectionDto{id, archivedAt}`, `ProfileFactDto{id,title,createdAt}` BE↔FE; `listReflections(limit, includeArchived)`. ✓
- Adjacent edit (CLAUDE.md): 4 new commands registered in `generate_handler!`. ✓

## Execution Handoff
Subagent-Driven. Task 1 → Task 2 (BE) → Task 3 (FE); controller reviews each; one combined PR.
