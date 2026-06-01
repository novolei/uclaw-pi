# skills.sh Marketplace P3 — In-Chat Install Card Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render `skill_marketplace_search` tool results as an elegant in-chat card with per-skill **[全局] / [本工作区]** install buttons that actually install + activate the skill (both scopes working end-to-end).

**Architecture:** A new shared tool-result renderer (`skill-marketplace-search-result.tsx`) registered in the single `ToolResultRenderer` dispatcher — so it renders in **both** Chat and Agent surfaces (they share `ChatToolBlock → ToolResultRenderer`). Install buttons call the existing P1 backend command `install_skill_from_marketplace` through a thin skills-bridge wrapper. Workspace-scoped install is made to **activate** by writing the workspace tag into the skill's `SKILL.md activation.tags` (frontmatter edit) **and** the space's `spaces.skill_tags` — matching the existing tag-intersection `skill_matches_workspace` check. Global install stays untagged (active everywhere).

**Tech Stack:** Rust (Tauri command + `serde_yml` frontmatter edit + rusqlite), React/TS (Jotai atom for active workspace, `invoke` button pattern mirrored from `bash-result.tsx`).

**Scope decisions (locked):**
- **Both scopes ship.** Global (tasks 1–6) works first; workspace-scoping (tasks 7–9) is ordered last so it's bisectable and global still ships if cut.
- **Audit badges deferred** (spec §5 `⚠未审计/✓低/⚠高` + HIGH-risk confirm) → a noted P3 follow-up, not this slice.
- **Uninstall / 已安装管理** stays in P4 (market page).

**Key anchors (verified 2026-06-01):**
- Dispatcher: `ui/src/shared/tool-rendering/tool-renderers/index.tsx:23-47` (switch on `toolName`).
- Button precedent: `ui/src/shared/tool-rendering/tool-renderers/bash-result.tsx:38-68` (`invoke` + `useState` + onClick).
- Tool result shape (P2): `src-tauri/src/agent/tools/builtin/skill_marketplace.rs:123-137` → `{ ok, results:[{id,name,source,installs,installUrl}], … }`.
- Install command (P1): `src-tauri/src/commands/skills_marketplace.rs:37-64` — `install_skill_from_marketplace(id, scope, workspace)`. **No frontend caller yet** (safe to change signature).
- Activation: `src-tauri/src/skills_manifest.rs:115-123` `skill_matches_workspace` (intersection; untagged ⇒ matches all). Skill tags: `src-tauri/src/skills.rs:105` `ActivationCriteria.tags` (from SKILL.md frontmatter only). Space tags: `services/workspace_service.rs:209-248` `get_skill_tags`/`set_skill_tags`; `normalize_skill_tags`.
- Frontend active workspace: `ui/src/atoms/workspace.ts` `activeWorkspaceIdAtom` (string id) + `activeWorkspaceCwdAtom` (path).

---

## File Structure

| File | Create/Modify | Responsibility |
|---|---|---|
| `src-tauri/src/skills_marketplace/install.rs` | Modify | + `add_activation_tag(skill_dir, tag)` — idempotent SKILL.md frontmatter edit |
| `src-tauri/src/commands/skills_marketplace.rs` | Modify | `install_skill_from_marketplace`: `workspace: Option<String>`(path) → `workspace_id: Option<String>`(space id); wire tag-write + DB tag + best-effort symlink |
| `ui/src/lib/bridge/skills.ts` | Modify | + `installSkillFromMarketplace(id, scope, workspaceId?)` wrapper |
| `ui/src/lib/tauri-bridge.ts` | Modify | + the raw `invoke` impl re-exported by the skills bridge (matches existing skills-bridge indirection) |
| `ui/src/shared/tool-rendering/tool-renderers/skill-marketplace-search-result.tsx` | Create | The card: parse `.results[]`, render rows + install buttons + per-row state |
| `ui/src/shared/tool-rendering/tool-renderers/index.tsx` | Modify | Register `case 'skill_marketplace_search'` |

---

## Task 1: Backend — `add_activation_tag` frontmatter helper

**Files:**
- Modify: `src-tauri/src/skills_marketplace/install.rs`
- Test: inline `#[cfg(test)]` in the same file

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `install.rs`:

```rust
#[test]
fn add_activation_tag_adds_to_frontmatter_and_preserves_body() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: demo\ndescription: a demo skill\n---\n# Demo\n\nbody line\n",
    )
    .unwrap();

    add_activation_tag(dir, "ws-alpha").unwrap();
    let out = std::fs::read_to_string(dir.join("SKILL.md")).unwrap();
    assert!(out.contains("ws-alpha"), "tag must be written: {out}");
    assert!(out.contains("# Demo"), "body preserved");
    assert!(out.contains("body line"), "body preserved");

    // Idempotent: a second call does not duplicate the tag.
    add_activation_tag(dir, "ws-alpha").unwrap();
    let out2 = std::fs::read_to_string(dir.join("SKILL.md")).unwrap();
    assert_eq!(out2.matches("ws-alpha").count(), 1, "no duplicate tag");

    // The re-parsed skill carries the tag through the real parser.
    let parsed = crate::skills::parse_skill_md(&out2).unwrap();
    assert!(parsed.activation.tags.iter().any(|t| t == "ws-alpha"));
}
```

> NOTE at execution: confirm the real parser fn name/path — Explore reported parsing via `serde_yml::from_str` in `skills.rs:289`. If a public `parse_skill_md` (or equivalent, e.g. `SkillManifest::from_markdown`) is not exposed, drop the last 2 lines of the test and assert only on the file text; do NOT add a new public parser API just for the test.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib -p uclaw skills_marketplace::install::tests::add_activation_tag`
Expected: FAIL — `add_activation_tag` not defined.

- [ ] **Step 3: Write minimal implementation**

Add to `install.rs` (uses `serde_yml`, already a backend dep via `skills.rs`):

```rust
use std::path::Path;

/// Add `tag` to the `activation.tags` list in `<skill_dir>/SKILL.md`'s YAML
/// frontmatter, idempotently, preserving the body. This is how a workspace-scoped
/// install activates: the tag must appear in BOTH the skill (here) and the
/// space's `skill_tags` for `skill_matches_workspace` (intersection) to match.
/// Only the frontmatter is reserialized; the markdown body is rejoined verbatim.
pub fn add_activation_tag(skill_dir: &Path, tag: &str) -> Result<(), MarketplaceError> {
    let path = skill_dir.join("SKILL.md");
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| MarketplaceError::Install(format!("read SKILL.md: {e}")))?;

    // Split `---\n<frontmatter>\n---\n<body>`. If there's no frontmatter block we
    // can't safely inject — bail (caller treats this as non-fatal best-effort).
    let rest = raw
        .strip_prefix("---\n")
        .ok_or_else(|| MarketplaceError::Install("SKILL.md has no frontmatter".into()))?;
    let end = rest
        .find("\n---")
        .ok_or_else(|| MarketplaceError::Install("unterminated frontmatter".into()))?;
    let front = &rest[..end];
    // Body = everything after the closing fence line.
    let after = &rest[end + 4..]; // skip "\n---"
    let body = after.strip_prefix("\n").unwrap_or(after);

    let mut doc: serde_yml::Value = serde_yml::from_str(front)
        .map_err(|e| MarketplaceError::Install(format!("parse frontmatter: {e}")))?;

    // Ensure activation.tags is a sequence and contains `tag` exactly once.
    let map = doc
        .as_mapping_mut()
        .ok_or_else(|| MarketplaceError::Install("frontmatter is not a mapping".into()))?;
    let activation = map
        .entry(serde_yml::Value::from("activation"))
        .or_insert_with(|| serde_yml::Value::Mapping(serde_yml::Mapping::new()));
    let act_map = activation
        .as_mapping_mut()
        .ok_or_else(|| MarketplaceError::Install("activation is not a mapping".into()))?;
    let tags = act_map
        .entry(serde_yml::Value::from("tags"))
        .or_insert_with(|| serde_yml::Value::Sequence(Vec::new()));
    let seq = tags
        .as_sequence_mut()
        .ok_or_else(|| MarketplaceError::Install("activation.tags is not a list".into()))?;
    if !seq.iter().any(|v| v.as_str() == Some(tag)) {
        seq.push(serde_yml::Value::from(tag));
    }

    let new_front = serde_yml::to_string(&doc)
        .map_err(|e| MarketplaceError::Install(format!("serialize frontmatter: {e}")))?;
    let new_raw = format!("---\n{new_front}---\n{body}");
    std::fs::write(&path, new_raw)
        .map_err(|e| MarketplaceError::Install(format!("write SKILL.md: {e}")))?;
    Ok(())
}
```

> `serde_yml::to_string` already ends with `\n`, so `{new_front}---` puts the fence on its own line.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib -p uclaw skills_marketplace::install::tests::add_activation_tag`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/skills_marketplace/install.rs
git commit -m "feat(skills_marketplace): add_activation_tag — idempotent SKILL.md frontmatter tag write (P3)"
```

---

## Task 2: Backend — wire workspace activation into the install command

**Files:**
- Modify: `src-tauri/src/commands/skills_marketplace.rs`

**Context:** `install_skill_from_marketplace` currently takes `workspace: Option<String>` (a path) and only symlinks. Change it to take `workspace_id: Option<String>` (the space id from `activeWorkspaceIdAtom`). For workspace scope: tag the skill + tag the space + best-effort symlink (path resolved from the `spaces` row). Global scope is unchanged (untagged ⇒ active everywhere). **Do not hold the DB lock across an `await`.**

- [ ] **Step 1: Locate the current workspace branch + any callers/tests**

Run: `rg -n "install_skill_from_marketplace|workspace" src-tauri/src/commands/skills_marketplace.rs` and `rg -n "install_skill_from_marketplace" src-tauri/src ui/src`
Expected: the command def + its `main.rs` handler registration; **no frontend caller** (confirmed by P3 anchor pass). If a test references the old `workspace:` arg, update it in this task.

- [ ] **Step 2: Change the signature + wire activation**

Replace the command body's workspace branch. Final shape (all DB work is sync, after the `client.detail(&id).await`; the lock is taken and dropped without an await in scope):

```rust
#[tauri::command]
pub async fn install_skill_from_marketplace(
    state: State<'_, AppState>,
    id: String,
    scope: InstallScope,
    workspace_id: Option<String>,
) -> Result<String, Error> {
    let client = SkillsShClient::new(read_api_key(&state));
    let detail = client.detail(&id).await.map_err(map_err)?;           // <-- only await
    let slug = install::flatten_slug(&id);
    let skills_root = state.data_dir.join("skills");
    let dir = install::write_skill_files(&skills_root, &slug, &detail).map_err(map_err)?;

    if scope == InstallScope::Workspace {
        if let Some(space_id) = workspace_id.as_deref() {
            // Tag string = normalized space id; the same tag goes on the skill and
            // the space so skill_matches_workspace (intersection) activates it here.
            let tag = crate::commands::workspace::normalize_skill_tags(vec![space_id.to_string()])
                .into_iter()
                .next()
                .unwrap_or_else(|| space_id.to_string());

            // 1) Tag the skill (best-effort — a skill without frontmatter just stays
            //    untagged/global rather than failing the whole install).
            if let Err(e) = install::add_activation_tag(&dir, &tag) {
                tracing::warn!("workspace tag write skipped: {e}");
            }

            // 2) Tag the space + resolve its path — one short sync DB section, no await.
            let ws_path: Option<String> = {
                use crate::services::workspace_service::{DbWorkspace, WorkspaceService};
                let conn = state
                    .db
                    .lock()
                    .map_err(|e| Error::Internal(format!("DB lock: {e}")))?;
                let mut tags = DbWorkspace.get_skill_tags(&conn, space_id);
                if !tags.iter().any(|t| t == &tag) {
                    tags.push(tag.clone());
                    let json = serde_json::to_string(&tags)
                        .map_err(|e| Error::Internal(format!("serialize tags: {e}")))?;
                    let _ = DbWorkspace.set_skill_tags(&conn, space_id, &json);
                }
                conn.query_row(
                    "SELECT path FROM spaces WHERE id = ?1",
                    [space_id],
                    |r| r.get::<_, Option<String>>(0),
                )
                .ok()
                .flatten()
            };

            // 3) Best-effort symlink for file-tree visibility (only if the space has a path).
            if let Some(p) = ws_path {
                if let Err(e) = install::link_into_workspace(std::path::Path::new(&p), &slug, &dir) {
                    tracing::warn!("workspace symlink skipped: {e}");
                }
            }
        }
    }

    // … existing registry add_scan_dir + discover + V25 record (UNCHANGED) …
    // (discover re-parses the now-tagged SKILL.md, so activation is live)

    Ok(format!("installed {slug}"))
}
```

> At execution: keep the existing post-branch body (registry rescan + `record_install`) exactly as P1 wrote it — only the branch above changes. Confirm the real method names on `DbWorkspace` (`get_skill_tags`/`set_skill_tags`) and that `normalize_skill_tags` is `pub` in `commands::workspace`; if it's private, inline the trim+lowercase (`space_id.trim().to_lowercase()`) instead of reaching for it.

- [ ] **Step 3: Build**

Run: `cargo build 2>&1 | grep -E "^error" | head`
Expected: no errors. Fix the registration in `main.rs` only if the arg-name change surfaces there (it won't — args are by-name at the IPC layer).

- [ ] **Step 4: Run the marketplace suite**

Run: `cargo test --lib -p uclaw skills_marketplace`
Expected: all pass (P1's 11 + Task 1's new test).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/skills_marketplace.rs
git commit -m "feat(skills_marketplace): workspace-scoped install activates via tag (skill SKILL.md + spaces.skill_tags) (P3)"
```

---

## Task 3: Frontend — `installSkillFromMarketplace` bridge

**Files:**
- Modify: `ui/src/lib/tauri-bridge.ts` (raw `invoke`)
- Modify: `ui/src/lib/bridge/skills.ts` (re-export, matching the existing skills-bridge indirection)

- [ ] **Step 1: Add the raw bridge fn in `tauri-bridge.ts`**

Near the other skills fns:

```typescript
/** Install a skill from skills.sh. scope 'global' = active everywhere (untagged);
 *  'workspace' = active only in `workspaceId` (tag-scoped). Returns a status string. */
export const installSkillFromMarketplace = (
  id: string,
  scope: 'global' | 'workspace',
  workspaceId?: string,
): Promise<string> =>
  invoke<string>('install_skill_from_marketplace', { id, scope, workspaceId })
```

- [ ] **Step 2: Re-export from the skills bridge**

In `ui/src/lib/bridge/skills.ts`, add `installSkillFromMarketplace` to the `export { … } from '../tauri-bridge'` list.

- [ ] **Step 3: Typecheck**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep -E "tauri-bridge|bridge/skills" || echo clean`
Expected: `clean` (no new errors in these files).

- [ ] **Step 4: Commit**

```bash
git add ui/src/lib/tauri-bridge.ts ui/src/lib/bridge/skills.ts
git commit -m "feat(skills_marketplace): installSkillFromMarketplace skills-bridge wrapper (P3)"
```

---

## Task 4: Frontend — the install card renderer

**Files:**
- Create: `ui/src/shared/tool-rendering/tool-renderers/skill-marketplace-search-result.tsx`

**Context:** Mirrors `bash-result.tsx` (parse `result` JSON, `invoke` + `useState` buttons). Receives the renderer props used by the dispatcher: `{ result: string; isError?: boolean; input?: Record<string, unknown> }`. Parse `result` → `{ results: [{id,name,source,installs,installUrl}] }`. Per row: name + source + installs + a link, and two install buttons. Track per-row state. Workspace button reads `activeWorkspaceIdAtom`; disabled (with hint) when no active workspace.

- [ ] **Step 1: Write the component**

```tsx
// skills.sh marketplace search result card — renders skill_marketplace_search
// candidates with [全局]/[本工作区] install buttons. Shared renderer (chat + agent).
// Mirrors bash-result.tsx's invoke+useState button pattern. P3 of the marketplace
// design (docs/superpowers/specs/2026-06-01-skills-sh-marketplace-design.md).
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import { installSkillFromMarketplace } from '@/lib/bridge/skills'

interface SkillRow {
  id: string
  name: string
  source: string
  installs?: number
  installUrl?: string
}
type RowState =
  | { kind: 'idle' }
  | { kind: 'installing'; scope: 'global' | 'workspace' }
  | { kind: 'installed'; scope: 'global' | 'workspace' }
  | { kind: 'error'; message: string }

export function SkillMarketplaceSearchResultCard({
  result,
  isError,
}: {
  result: string
  isError?: boolean
  input?: Record<string, unknown>
}) {
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const [states, setStates] = React.useState<Record<string, RowState>>({})

  const parsed = React.useMemo(() => {
    try {
      return JSON.parse(result) as { results?: SkillRow[]; note?: string }
    } catch {
      return null
    }
  }, [result])

  if (isError || !parsed) {
    return (
      <div className="text-xs text-red-400/90 bg-red-400/10 rounded-lg px-3 py-2">
        skills.sh 搜索失败{!parsed && '（结果解析失败）'}。本地 skill_search 不受影响。
      </div>
    )
  }
  const rows = parsed.results ?? []
  if (rows.length === 0) {
    return <div className="text-xs text-muted-foreground px-1 py-1">skills.sh 无匹配结果。</div>
  }

  const install = async (row: SkillRow, scope: 'global' | 'workspace') => {
    setStates((s) => ({ ...s, [row.id]: { kind: 'installing', scope } }))
    try {
      await installSkillFromMarketplace(
        row.id,
        scope,
        scope === 'workspace' ? activeWorkspaceId ?? undefined : undefined,
      )
      setStates((s) => ({ ...s, [row.id]: { kind: 'installed', scope } }))
    } catch (e) {
      setStates((s) => ({ ...s, [row.id]: { kind: 'error', message: String(e) } }))
    }
  }

  return (
    <div className="flex flex-col gap-1.5">
      {rows.map((row) => {
        const st = states[row.id] ?? { kind: 'idle' }
        const busy = st.kind === 'installing'
        const done = st.kind === 'installed'
        return (
          <div
            key={row.id}
            className="rounded-lg border border-border px-3 py-2 flex items-center justify-between gap-3"
          >
            <div className="min-w-0">
              <div className="text-sm text-foreground truncate">{row.name}</div>
              <div className="text-xs text-muted-foreground truncate">
                {row.source}
                {typeof row.installs === 'number' && ` · ${row.installs} installs`}
              </div>
              {st.kind === 'error' && (
                <div className="text-xs text-red-400/90 mt-0.5">{st.message}</div>
              )}
            </div>
            {done ? (
              <span className="shrink-0 text-xs px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-400">
                已安装（{st.scope === 'global' ? '全局' : '本工作区'}）
              </span>
            ) : (
              <div className="shrink-0 flex items-center gap-1.5">
                <button
                  type="button"
                  onClick={() => install(row, 'global')}
                  disabled={busy}
                  className="text-xs px-2.5 py-1 rounded-lg bg-muted text-muted-foreground hover:bg-muted/70 transition-colors disabled:opacity-50"
                >
                  {busy && st.scope === 'global' ? '…' : '全局'}
                </button>
                <button
                  type="button"
                  onClick={() => install(row, 'workspace')}
                  disabled={busy || !activeWorkspaceId}
                  title={activeWorkspaceId ? '安装到当前工作区' : '无活动工作区'}
                  className="text-xs px-2.5 py-1 rounded-lg bg-muted text-muted-foreground hover:bg-muted/70 transition-colors disabled:opacity-50"
                >
                  {busy && st.scope === 'workspace' ? '…' : '本工作区'}
                </button>
              </div>
            )}
          </div>
        )
      })}
    </div>
  )
}
```

> At execution: confirm `@/atoms/workspace` exports `activeWorkspaceIdAtom` and that `jotai`'s `useAtomValue` is the project's read hook (Explore confirmed both). If the project pins a different import alias, match the surrounding files.

- [ ] **Step 2: Typecheck**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep "skill-marketplace-search-result" || echo clean`
Expected: `clean`.

- [ ] **Step 3: Commit**

```bash
git add ui/src/shared/tool-rendering/tool-renderers/skill-marketplace-search-result.tsx
git commit -m "feat(skills_marketplace): in-chat install card renderer (P3)"
```

---

## Task 5: Frontend — register the renderer

**Files:**
- Modify: `ui/src/shared/tool-rendering/tool-renderers/index.tsx`

- [ ] **Step 1: Add the import + switch case**

Import near the other renderers, and inside the `ToolResultRenderer` switch (`index.tsx:33-46`):

```tsx
import { SkillMarketplaceSearchResultCard } from './skill-marketplace-search-result'
// …
    case 'skill_marketplace_search':
      return <SkillMarketplaceSearchResultCard result={result} isError={isError} input={input} />
```

> Match the exact prop names the other cases pass (Explore reports `result`, `isError`, `input` — confirm against an adjacent case like `bash`).

- [ ] **Step 2: Typecheck**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep "tool-renderers/index" || echo clean`
Expected: `clean`.

- [ ] **Step 3: Commit**

```bash
git add ui/src/shared/tool-rendering/tool-renderers/index.tsx
git commit -m "feat(skills_marketplace): register skill_marketplace_search card in ToolResultRenderer (P3)"
```

---

## Task 6: Final verification + manual-UI checkpoint

- [ ] **Step 1: Full backend build + marketplace tests**

Run: `cargo build 2>&1 | grep -E "^error" | head` (expect none) and `cargo test --lib -p uclaw skills_marketplace` (expect all pass).

- [ ] **Step 2: TS check (touched files clean)**

Run: `cd ui && npx tsc --noEmit 2>&1 | grep -E "skill-marketplace|tool-renderers/index|bridge/skills|tauri-bridge" || echo "clean (pre-existing repo errors unrelated)"`
Expected: `clean`.

- [ ] **Step 3: ⚠️ Manual UI checkpoint (cannot drive the native window)**

Document in the PR for a human spot-check:
1. Trigger `skill_marketplace_search` in chat (needs a `skills_sh_api_key` set via the #23 Settings card) → card renders rows.
2. Click **全局** → row flips to 已安装（全局）; `skill_search` finds it in any workspace.
3. Switch to a workspace, click **本工作区** on another skill → 已安装（本工作区）; it's active in THAT workspace's chat but the space's `skill_tags` now contains the workspace tag; a different workspace does not see it.
4. Both Chat and Agent surfaces render the same card (shared renderer).

---

## Self-Review

- **Spec coverage:** P3 row of the design table = "聊天内安装卡片(工具结果渲染器 + bridge + install 命令接线)" → Tasks 4/5 (renderer+register), Task 3 (bridge), Tasks 1/2 (the install command's activation). ✓ Audit badges (spec §5) explicitly deferred — noted, not silently dropped.
- **Type consistency:** `install_skill_from_marketplace(id, scope, workspaceId)` — bridge passes `workspaceId` (camel) → Tauri maps to `workspace_id` (snake). `scope: 'global'|'workspace'` ↔ `InstallScope` serde lowercase. Card `SkillRow` fields match the P2 tool's `to_result_json` (`id,name,source,installs,installUrl`). ✓
- **No placeholders:** every step has real code. The two `NOTE at execution` callouts are verification guards (parser fn name, `normalize_skill_tags` visibility), not deferred work.
- **Bisectability:** global path (T1–T6) is independently shippable; workspace-scoping lives in T2 (and T1's helper) — if cut, the card + global still works.
