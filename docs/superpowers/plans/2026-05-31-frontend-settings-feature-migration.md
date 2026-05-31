# Frontend settings-feature migration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate the `settings` UI domain into `ui/src/features/settings/` per the code-organization ADR (2026-05-31), as the pilot that establishes the repeatable `features/<domain>/` pattern.

**Architecture:** A self-contained feature folder (`components/ hooks/ atoms/ lib/` + `index.ts` barrel). Presentation components are thin and ≤ ~300 lines; side effects (IPC, polling) live in hooks; all IPC goes through `lib/bridge/settings.ts` (no direct `@tauri-apps/api` in components). Migrated phase-by-phase, each a bisectable commit gated by `tsc` + `vitest` (incl. new jsdom render tests). Because the agent cannot drive the Tauri native window, each milestone ends with a **USER VERIFY** checkpoint.

**Tech Stack:** React 18 + TypeScript, Vite, Vitest + jsdom (`renderWithProviders` from `@/test-utils/render`), Jotai, Tauri v2 (`@tauri-apps/api/core` `invoke`). `@/` is aliased to `ui/src`.

---

## File structure (created/modified)

```
ui/src/features/settings/
  index.ts                       # CREATE — barrel; the only public surface
  components/
    SystemTab.tsx                # MOVE+THIN — composes the 4 cards
    system/
      DiagnosticsCard.tsx        # CREATE — health + get_system_diagnostics report
      ServicesCard.tsx           # CREATE — memu/gbrain restart/reset (handleBridgeAction)
      HttpApiToggleCard.tsx      # CREATE — the 本地 HTTP API 服务 toggle
      EvalsCard.tsx              # CREATE — eval suites (handleEvalRun/All)
    GeneralTab.tsx AgentSettings.tsx PromptsSettings.tsx
    BrowserRuntimeSettings.tsx   # MOVE (+split, 607 lines)
    AppearanceSettings.tsx AboutSettings.tsx ProxySetting.tsx SttSettings.tsx
    LearnedProfileTab.tsx …      # MOVE (the remaining settings components)
  hooks/
    useSystemDiagnostics.ts      # CREATE — report + loading + runDiagnostics
    useHttpApiToggle.ts          # CREATE — enabled + toggle (was inline in SystemTab)
    useEvalRunner.ts             # CREATE — evalReports + busy + run(kind)/runAll
    useBridgeAction.ts           # CREATE — memu/gbrain/reset/restart actions
  lib/                           # settings-only NON-IPC helpers (formatUptime, etc.)
ui/src/lib/bridge/settings.ts    # MODIFY — add the settings/system IPC moved out of
                                 #   components + tauri-bridge.ts
ui/src/lib/tauri-bridge.ts       # MODIFY — remove the settings-domain commands (P4)
ui/src/components/settings/*     # DELETE in P4 after consumers repoint to the barrel
```

---

## Migration recipe (applied per component in P1–P3)

For each settings component `X`:
1. Move `components/settings/X.tsx` → `features/settings/components/X.tsx` (preserve git history with `git mv`).
2. If `X` is > ~300 lines, split it: extract each visually/functionally distinct section into its own `features/settings/components/<x>/<Card>.tsx`; `X` becomes a thin shell that composes them.
3. Move every side effect out of the component into a `features/settings/hooks/useX*.ts` hook (the component calls the hook; the hook owns `useState`/`useEffect`/IPC).
4. Replace every direct `invoke(...)` with a `settingsBridge.*` call; add the wrapper to `lib/bridge/settings.ts` if missing.
5. Add a `features/settings/components/X.test.tsx` render test (`renderWithProviders(<X/>)`, assert key markers/cards present).
6. Export `X` from `features/settings/index.ts`.
7. Gate: `cd ui && npx tsc --noEmit` clean · `npx vitest run features/settings` green · `grep -rn "@tauri-apps/api" features/settings/components` empty · each file ≤ ~300 lines.
8. Commit (one component or one card-group per commit).

---

## Task 0 — P0: scaffold the feature + barrel

**Files:**
- Create: `ui/src/features/settings/index.ts`
- Test: `ui/src/features/settings/index.test.ts`

- [ ] **Step 1: Write the failing barrel test**

```ts
// ui/src/features/settings/index.test.ts
import { describe, it, expect } from 'vitest'
import * as settings from './index'

describe('features/settings barrel', () => {
  it('exposes the settingsBridge re-export', () => {
    expect(settings.settingsBridge).toBeDefined()
    expect(typeof settings.settingsBridge.getHttpApiEnabled).toBe('function')
  })
})
```

- [ ] **Step 2: Run it, expect FAIL** — `cd ui && npx vitest run features/settings/index.test.ts` → fails (no `./index`).

- [ ] **Step 3: Create the barrel**

```ts
// ui/src/features/settings/index.ts
// Public surface of the settings feature. Other features import ONLY from here.
export { settingsBridge } from '../../lib/bridge/settings'
// Components/hooks are added to this barrel as they migrate (P1–P3).
```

- [ ] **Step 4: Run it, expect PASS.**

- [ ] **Step 5: Commit** — `git add ui/src/features/settings/index.ts ui/src/features/settings/index.test.ts && git commit -m "feat(features/settings): scaffold feature + barrel"`

---

## Task 1 — P1: SystemTab → system/ cards + hooks

SystemTab (865 lines) holds: system info/health, `get_system_diagnostics` (DiagnosticsCard), memu/gbrain bridge actions (ServicesCard), the HTTP-API toggle (HttpApiToggleCard), eval suites (EvalsCard). Extract each into a card + a hook; SystemTab becomes a thin shell.

**Files:**
- Create: `ui/src/features/settings/hooks/useHttpApiToggle.ts`, `.../hooks/useSystemDiagnostics.ts`, `.../hooks/useEvalRunner.ts`, `.../hooks/useBridgeAction.ts`
- Create: `ui/src/features/settings/components/system/{HttpApiToggleCard,DiagnosticsCard,ServicesCard,EvalsCard}.tsx`
- Create: `ui/src/features/settings/components/SystemTab.tsx` (thin shell) + `SystemTab.test.tsx`
- Modify: `ui/src/lib/bridge/settings.ts` (add `getSystemDiagnostics`, the eval commands, the bridge actions)

- [ ] **Step 1: Add the system IPC to the bridge**

```ts
// ui/src/lib/bridge/settings.ts — extend settingsBridge
export const settingsBridge = {
  getHttpApiEnabled: (): Promise<boolean> => invoke<boolean>('get_http_api_enabled'),
  setHttpApiEnabled: (enabled: boolean): Promise<void> =>
    invoke<void>('set_http_api_enabled', { enabled }),
  getSystemDiagnostics: <T = unknown>(): Promise<T> => invoke<T>('get_system_diagnostics'),
  runEval: <T = unknown>(command: string): Promise<T> => invoke<T>(command),
  bridgeAction: (command: string): Promise<void> => invoke<void>(command),
}
```

- [ ] **Step 2: Write the http-toggle hook (moves the logic out of SystemTab)**

```ts
// ui/src/features/settings/hooks/useHttpApiToggle.ts
import * as React from 'react'
import { settingsBridge } from '../../../lib/bridge/settings'

export function useHttpApiToggle(onError?: (m: string) => void) {
  const [enabled, setEnabled] = React.useState<boolean | null>(null)
  React.useEffect(() => {
    settingsBridge.getHttpApiEnabled().then(setEnabled).catch(() => setEnabled(false))
  }, [])
  const toggle = React.useCallback(async () => {
    if (enabled === null) return
    const next = !enabled
    try { await settingsBridge.setHttpApiEnabled(next); setEnabled(next) }
    catch (e) { onError?.(String(e)) }
  }, [enabled, onError])
  return { enabled, toggle }
}
```

- [ ] **Step 3: Write the HttpApiToggleCard test (failing)**

```tsx
// ui/src/features/settings/components/system/HttpApiToggleCard.test.tsx
import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { HttpApiToggleCard } from './HttpApiToggleCard'

vi.mock('../../../../lib/bridge/settings', () => ({
  settingsBridge: { getHttpApiEnabled: vi.fn().mockResolvedValue(false), setHttpApiEnabled: vi.fn().mockResolvedValue(undefined) },
}))

describe('HttpApiToggleCard', () => {
  it('renders the 本地 HTTP API 服务 card', async () => {
    const { findByText } = renderWithProviders(<HttpApiToggleCard />)
    expect(await findByText(/本地 HTTP API 服务/)).toBeTruthy()
  })
})
```

- [ ] **Step 4: Run it, expect FAIL** — `cd ui && npx vitest run features/settings/components/system/HttpApiToggleCard.test.tsx`.

- [ ] **Step 5: Write HttpApiToggleCard** — move the `本地 HTTP API 服务` card JSX out of `components/settings/SystemTab.tsx` into this file; it calls `useHttpApiToggle(onError)`; props: `{ onError?: (m: string) => void }`. (≤ ~80 lines.)

- [ ] **Step 6: Run it, expect PASS.**

- [ ] **Step 7: Repeat steps 2–6 for the other three cards + hooks** following the recipe:
  - `useSystemDiagnostics` (`report`, `loading`, `runDiagnostics` via `settingsBridge.getSystemDiagnostics`) → `DiagnosticsCard` (health summary + report; move `formatUptime`/`formatMemory` to `features/settings/lib/format.ts`).
  - `useBridgeAction` (busy flags + `settingsBridge.bridgeAction`) → `ServicesCard` (memu/gbrain restart/reset buttons).
  - `useEvalRunner` (`evalReports`, `busy`, `run(kind)`, `runAll` via `settingsBridge.runEval` + `evalCommands`) → `EvalsCard`.
  Each card gets its own `*.test.tsx` (render + key marker) and its own commit.

- [ ] **Step 8: Write the thin SystemTab shell + test**

```tsx
// ui/src/features/settings/components/SystemTab.tsx
import * as React from 'react'
import { DiagnosticsCard } from './system/DiagnosticsCard'
import { ServicesCard } from './system/ServicesCard'
import { HttpApiToggleCard } from './system/HttpApiToggleCard'
import { EvalsCard } from './system/EvalsCard'

export function SystemTab() {
  const [error, setError] = React.useState<string | null>(null)
  return (
    <div className="flex flex-col gap-4 p-4 max-w-2xl" data-settings-section="系统诊断">
      {error && <div className="text-xs text-red-400 bg-red-400/10 rounded-lg px-3 py-2">{error}</div>}
      <DiagnosticsCard onError={setError} />
      <HttpApiToggleCard onError={setError} />
      <ServicesCard onError={setError} />
      <EvalsCard onError={setError} />
    </div>
  )
}
```

```tsx
// ui/src/features/settings/components/SystemTab.test.tsx
import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { SystemTab } from './SystemTab'
vi.mock('../../../lib/bridge/settings', () => ({
  settingsBridge: {
    getHttpApiEnabled: vi.fn().mockResolvedValue(false),
    setHttpApiEnabled: vi.fn().mockResolvedValue(undefined),
    getSystemDiagnostics: vi.fn().mockResolvedValue(null),
    runEval: vi.fn(), bridgeAction: vi.fn(),
  },
}))
describe('SystemTab', () => {
  it('renders the 系统诊断 shell with its cards', () => {
    const { container } = renderWithProviders(<SystemTab />)
    expect(container.querySelector('[data-settings-section="系统诊断"]')).toBeTruthy()
  })
})
```

- [ ] **Step 9: Export from the barrel** — add `export { SystemTab } from './components/SystemTab'` to `features/settings/index.ts`.

- [ ] **Step 10: Repoint the SystemTab consumer** — find who renders the old `components/settings/SystemTab` (`grep -rn "settings/SystemTab" ui/src`) and import from `@/features/settings`; delete `components/settings/SystemTab.tsx`.

- [ ] **Step 11: Gate** — `cd ui && npx tsc --noEmit` clean · `npx vitest run features/settings` green · `grep -rn "@tauri-apps/api" ui/src/features/settings/components` empty · every new file ≤ ~300 lines.

- [ ] **Step 12: Commit** — `git add ui/src/features/settings ui/src/lib/bridge/settings.ts && git commit -m "feat(features/settings): migrate+split SystemTab into system/ cards + hooks"`

- [ ] **Step 13: 🚦 USER VERIFY (P1 milestone)** — STOP and ask the user to open **设置 → 系统诊断**: the diagnostics report, the **本地 HTTP API 服务** toggle (toggles + persists), service actions, and eval suites all render + work. Do not proceed to P2 until confirmed.

---

## Task 2 — P2: GeneralTab / AgentSettings / PromptsSettings

Apply the **Migration recipe** to each, one commit each. Specifics:
- `GeneralTab.tsx` — already has `[data-settings-section]` markers + a test; move it + its test under `features/settings/`, move any `invoke` into `settingsBridge`, keep the existing test assertions (`通用偏好`, `主题与字体`). 
- `AgentSettings.tsx`, `PromptsSettings.tsx` — move; extract side effects into `hooks/`; split if > ~300 lines.
- Each: add/keep a render test; export from the barrel; repoint consumers; gate (recipe step 7); commit.

- [ ] **Step 1–N:** for each of the three, run recipe steps 1–8 (test → move/split → hook → bridge → render test → barrel → gate → commit).
- [ ] **Step final: 🚦 USER VERIFY (P2 milestone)** — user opens those three settings tabs; confirm render + actions. 

---

## Task 3 — P3: BrowserRuntimeSettings (split 607) + remaining settings components

- [ ] **Step 1:** Apply the recipe to `BrowserRuntimeSettings.tsx` (607 → split into focused cards under `components/browser-runtime/`; move side effects to `hooks/`; it already has a test — keep it green). Commit.
- [ ] **Step 2:** Apply the recipe to the remaining settings components (`AppearanceSettings`, `AboutSettings`, `ProxySetting`, `SttSettings`, `LearnedProfileTab`, and any others under `components/settings/`): move, extract hooks, bridge the IPC, render test, barrel, gate. One commit per component (or small group).
- [ ] **Step 3: 🚦 USER VERIFY (P3 milestone)** — user spot-checks the migrated tabs. 

---

## Task 4 — P4: repoint imports + delete old + bridge cleanup

- [ ] **Step 1:** `grep -rn "components/settings/" ui/src` → repoint every remaining consumer import to `@/features/settings` (the barrel). No deep imports into feature internals.
- [ ] **Step 2:** Delete the now-empty `ui/src/components/settings/` (all moved).
- [ ] **Step 3:** Move any residual settings-domain commands out of `lib/tauri-bridge.ts` into `lib/bridge/settings.ts` (the god-bridge should hold no settings IPC).
- [ ] **Step 4: Gate** — `cd ui && npx tsc --noEmit` clean · `npx vitest run` green · `grep -rn "components/settings/" ui/src` empty · `grep -rn "@tauri-apps/api" ui/src/features/settings/components` empty.
- [ ] **Step 5: Commit** — `git commit -m "refactor(features/settings): repoint consumers + delete old components/settings + bridge cleanup"`
- [ ] **Step 6: 🚦 USER VERIFY (final)** — user exercises the entire settings surface; confirm no regressions.

---

## Self-review

- **Spec coverage:** P0 (scaffold+barrel), P1 (SystemTab split — the spec's headline), P2–P3 (whole-domain + god-component split incl. BrowserRuntimeSettings 607), P4 (repoint+delete+bridge cleanup), per-phase gates + USER VERIFY milestones, the recipe = the "repeatable pattern" — all spec sections map to tasks. ✓
- **Placeholders:** the recipe + P0/P1 carry concrete code (barrel, hook, cards, tests, bridge). P2–P3 reference the recipe with per-component specifics (sizes, existing tests, markers) rather than re-printing identical steps (DRY). No "TBD/handle edge cases". ✓
- **Type consistency:** `settingsBridge` method names (`getHttpApiEnabled`/`setHttpApiEnabled`/`getSystemDiagnostics`/`runEval`/`bridgeAction`) are defined in P1 step 1 and used consistently in the hooks. Hooks return the shapes the cards consume. ✓

## Execution handoff

This plan is for a **dedicated PR round** (the user's choice). When ready, execute via
superpowers:subagent-driven-development (fresh subagent per task, review between) or
superpowers:executing-plans (inline, checkpoints). The `🚦 USER VERIFY` steps are hard
stops — the agent cannot drive the Tauri native window, so the user confirms each milestone.
After settings is proven, `agent` / `chat` / other domains copy this plan's recipe, each in
its own spec + plan + PR.
