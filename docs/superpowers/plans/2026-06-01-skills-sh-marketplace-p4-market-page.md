# skills.sh Marketplace P4 — 万花筒 Market Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (or executing-plans). Steps use `- [ ]`.

**Goal:** Finish skills.sh by adding a **市场 (marketplace) tab** to the 万花筒 skills page — browse trending / search skills.sh, view a detail drawer (SKILL.md preview + audit badge), and install (global / this workspace) from the page.

**Architecture:** Sliced for verifiability (the UI cannot be auto-driven by the agent):
- **P4a (this PR) — plumbing**, fully `cargo`/`tsc`-verifiable: mirror the Rust marketplace types into TS, add the read-command bridge fns (`search`/`list`/`detail`/`audit`), and add a clean read-only `check_skill_marketplace_update` command + bridge.
- **P4b (next PR) — UI**: the `marketplace` filter tab in `SkillsModule` + a detail drawer (`Sheet`) reusing P3's install flow. Manual UI checkpoint.

**Deferred (noted, not silent):** **uninstall** (needs registry scan-dir removal + the P3 workspace-tag reversal — a separate slice) and a dedicated "已安装管理" beyond the existing enable/disable/detail + the already-shipped `marketplace` provenance badge (`SkillDetail.tsx:29-34`).

**Verified anchors (2026-06-01):**
- Page: `ui/src/views/Kaleidoscope/modules/Skills/SkillsModule.tsx` (tabs: `FilterTab` union + `filterTabs` array @ ~line 207; render @ ~392/436). List: `SkillsList.tsx`. Detail: `SkillDetail.tsx` (PROVENANCE_BADGE already has `marketplace`).
- Drawer precedents: `ui/src/components/ui/sheet.tsx` (`side="right"`); marketplace overlay precedent `ui/src/components/automation/StoreDetail.tsx`.
- Backend commands (registered): `search_skill_marketplace`, `list_skill_marketplace`, `get_skill_marketplace_detail`, `get_skill_marketplace_audit`, `install_skill_from_marketplace` (`commands/skills_marketplace.rs`).
- Service fns: `install::read_install_version(conn, slug) -> Result<Option<String>>`, `install::flatten_slug(id)` (`skills_marketplace/install.rs`).
- Bridge already has `installSkillFromMarketplace(id, scope, workspaceId?)` (`tauri-bridge.ts:848`, re-exported `bridge/skills.ts`).
- Rust serde shapes (mod.rs): `SkillSummary{id,slug,name,source,installs,sourceType,installUrl,url}`, `SkillFile{path,contents}`, `SkillDetail{id,source,slug,hash,files}`, `SkillAudit{audits:[{status,riskLevel,summary}]}`, `InstallScope` lowercase.

---

## P4a — Plumbing (this PR)

### Task 1: `check_skill_marketplace_update` command

**Files:** Modify `src-tauri/src/commands/skills_marketplace.rs`; Modify `src-tauri/src/main.rs` (register).

Clean semantics (unlike `needs_update`, which returns true for not-installed): **update available iff the skill is tracked-installed AND the latest skills.sh hash differs.**

- [ ] **Step 1: Add the command** (after `get_skill_marketplace_audit`):

```rust
/// Whether an installed marketplace skill has a newer version on skills.sh.
/// `true` iff the slug is tracked-installed (V25) AND its stored hash differs from
/// the latest detail hash. Not-installed ⇒ `false` (nothing to update).
#[tauri::command]
pub async fn check_skill_marketplace_update(state: State<'_, AppState>, id: String) -> Result<bool, Error> {
    let detail = SkillsShClient::new(read_api_key(&state)).detail(&id).await.map_err(map_err)?;
    let slug = install::flatten_slug(&id);
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
    let installed = install::read_install_version(&conn, &slug).ok().flatten();
    Ok(installed.is_some_and(|h| h != detail.hash))
}
```

- [ ] **Step 2: Register** in `main.rs`'s `generate_handler!` (next to the other `skills_marketplace::*` entries):

```rust
            uclaw_core::commands::skills_marketplace::check_skill_marketplace_update,
```

> At execution: grep `search_skill_marketplace` in `main.rs` to find the exact registration block; add the line there.

- [ ] **Step 3: Build** — `cargo build 2>&1 | grep -E "^error"` (expect none).
- [ ] **Step 4: Commit** — `git add src-tauri/src/commands/skills_marketplace.rs src-tauri/src/main.rs && git commit -m "feat(skills_marketplace): check_skill_marketplace_update command (P4a)"`

### Task 2: Mirror marketplace TS types

**Files:** Modify `ui/src/lib/types.ts` (near `SkillInfo`, ~line 901).

- [ ] **Step 1: Add the types** (camelCase matches the Rust serde renames):

```ts
/** skills.sh search/list row (mirrors Rust `SkillSummary`). */
export interface MarketplaceSkillSummary {
  id: string
  slug: string
  name: string
  source: string
  installs: number
  sourceType: string
  installUrl: string
  url: string
}
/** One inline file from the detail endpoint (mirrors Rust `SkillFile`). */
export interface MarketplaceSkillFile {
  path: string
  contents: string
}
/** skills.sh detail with inline files (mirrors Rust `SkillDetail`). */
export interface MarketplaceSkillDetail {
  id: string
  source: string
  slug: string
  hash: string
  files: MarketplaceSkillFile[]
}
/** One audit verdict (mirrors Rust `SkillAuditEntry`). */
export interface MarketplaceSkillAuditEntry {
  status: string
  riskLevel: string // "LOW" | "MEDIUM" | "HIGH"
  summary: string
}
/** Audit response (mirrors Rust `SkillAudit`). */
export interface MarketplaceSkillAudit {
  audits: MarketplaceSkillAuditEntry[]
}
```

> Named `Marketplace*` to avoid colliding with any existing `SkillDetail`/`SkillInfo` in this file.

- [ ] **Step 2: tsc** — `cd ui && npx tsc --noEmit 2>&1 | grep "lib/types" || echo clean`.
- [ ] **Step 3: Commit** — `git add ui/src/lib/types.ts && git commit -m "feat(skills_marketplace): mirror marketplace TS types (P4a)"`

### Task 3: Marketplace read bridge fns

**Files:** Modify `ui/src/lib/tauri-bridge.ts` (near `installSkillFromMarketplace`, ~line 848); Modify `ui/src/lib/bridge/skills.ts` (re-export).

- [ ] **Step 1: Add to `tauri-bridge.ts`** (after `installSkillFromMarketplace`):

```ts
/** Search skills.sh by free-text query. */
export const searchSkillsMarketplace = (query: string, limit?: number): Promise<MarketplaceSkillSummary[]> =>
  invoke('search_skill_marketplace', { query, limit })
/** List skills.sh (view = 'trending'|'hot'|'all-time'). */
export const listSkillsMarketplace = (view?: string, page?: number): Promise<MarketplaceSkillSummary[]> =>
  invoke('list_skill_marketplace', { view, page })
/** Fetch a skill's detail (inline files) by id. */
export const getSkillMarketplaceDetail = (id: string): Promise<MarketplaceSkillDetail> =>
  invoke('get_skill_marketplace_detail', { id })
/** Fetch a skill's audit verdicts by id. */
export const getSkillMarketplaceAudit = (id: string): Promise<MarketplaceSkillAudit> =>
  invoke('get_skill_marketplace_audit', { id })
/** Whether an installed marketplace skill has a newer version on skills.sh. */
export const checkSkillMarketplaceUpdate = (id: string): Promise<boolean> =>
  invoke('check_skill_marketplace_update', { id })
```

> Add the `MarketplaceSkill*` type imports to the existing `import type { … } from './types'` block at the top of `tauri-bridge.ts`.

- [ ] **Step 2: Re-export** from `ui/src/lib/bridge/skills.ts` — add the 5 names to the `export { … } from '../tauri-bridge'` list.
- [ ] **Step 3: tsc** — `cd ui && npx tsc --noEmit 2>&1 | grep -E "tauri-bridge|bridge/skills" || echo clean`.
- [ ] **Step 4: Commit** — `git add ui/src/lib/tauri-bridge.ts ui/src/lib/bridge/skills.ts && git commit -m "feat(skills_marketplace): marketplace read bridge fns (search/list/detail/audit/check-update) (P4a)"`

### Task 4: P4a verification + PR
- [ ] `cargo build` clean; `cargo test --lib skills_marketplace` pass; `tsc` clean for touched files.
- [ ] Final review subagent over the P4a diff → PR.

---

## P4b — Market UI (next PR, outline)

Re-verify `SkillsModule.tsx` anchors at P4b execution, then:
1. **`marketplace` filter tab** — add `'marketplace'` to `FilterTab` + `filterTabs`; when active, render a `SkillsMarketplaceTab` (search box + `listSkillsMarketplace('trending')` rows) instead of the learned/builtin list.
2. **`SkillsMarketplaceTab.tsx`** — search input (debounced → `searchSkillsMarketplace`) + trending list; each row = name / source / installs + an **[安装 ▾]** (global/workspace) reusing the P3 flow (`installSkillFromMarketplace` + `activeWorkspaceIdAtom`); a row click opens the detail drawer.
3. **`SkillMarketplaceDetail.tsx`** — a `Sheet side="right"`: `getSkillMarketplaceDetail` → render the `SKILL.md` file preview; `getSkillMarketplaceAudit` → a risk badge (`✓ LOW` / `⚠ MEDIUM` / `⚠ HIGH`); install buttons (scope) with a **HIGH-risk confirm** before install.
4. **Installed marketplace skills** — the `marketplace` provenance badge already renders in `SkillDetail.tsx`; optionally surface `checkSkillMarketplaceUpdate` as a "有更新" hint. (Uninstall deferred.)

⚠️ **Manual UI checkpoint** (agent cannot drive the native window): tab switches to 市场; search returns rows; install → 已安装; drawer shows SKILL.md + audit badge; HIGH-risk confirm appears.

---

## Self-Review (P4a)
- **Coverage:** P4a delivers the data layer the P4b UI needs (types + read bridge + update-check); install is already bridged (P3). ✓
- **Type consistency:** TS `Marketplace*` fields match the Rust serde JSON exactly (camelCase renames `sourceType`/`installUrl`/`riskLevel`). `check_skill_marketplace_update` returns `bool`.
- **No placeholders:** every P4a step has real code. P4b is explicitly an outline to be detailed at execution (UI anchors re-verified then).
- **Deferred clearly:** uninstall + advanced installed-management noted, not silently dropped.
