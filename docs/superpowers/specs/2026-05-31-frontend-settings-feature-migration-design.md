# Design: Frontend `features/` migration — settings pilot

- **Date**: 2026-05-31
- **Status**: Proposed (pilot for the frontend code-organization discipline)
- **ADR**: `docs/adr/2026-05-31-pi-code-organization-discipline.md`
- **Scope**: the **settings** domain only. This is the pilot that establishes the
  repeatable `features/<domain>/` pattern; `agent` / `chat` / others follow as
  their own specs + plans + PRs (one domain per PR).

## Context

The pi-migration just landed the code-organization ADR, and the backend already
moved `settings` / `cost` / `cwd` to `commands/` + `services/`. The frontend is
the remaining pass. Current state (measured 2026-05-31):

- **~30 component domains** under `ui/src/components/` (agent, chat, settings,
  app-shell, automation, browser, canvas, …).
- `ui/src/lib/bridge/` **already** holds thin per-domain IPC bridges (agent, chat,
  settings, workspace, memory, skills, mcp, events — 18–21 lines each). Rule 5 is
  partially in place.
- **But** `ui/src/lib/tauri-bridge.ts` is a **2846-line god-bridge** still carrying
  most IPC, and components are large and flat:
  - `AgentView.tsx` 1926, `AgentMessages.tsx` 1277, `LeftSidebar.tsx` 1267,
    `SDKMessageRenderer.tsx` 1150, **`SystemTab.tsx` 865**,
    **`BrowserRuntimeSettings.tsx` 607**, … (caps: component ≤ ~300).

This pilot brings the ADR's frontend discipline (feature self-containment, size
caps, side-effects-in-hooks, single bridge entry, no cross-feature deep imports)
to the **settings** domain, end to end, as the template for the rest.

## Decisions (from brainstorming, 2026-05-31)

1. **Pilot = the whole settings domain** → `features/settings/` (not just a slice).
2. **Split the god-components** (`SystemTab` 865, `BrowserRuntimeSettings` 607,
   and any other settings component over ~300 lines) under the cap.
3. **Verification** (the agent cannot drive the Tauri native window):
   - **Phased** — each phase is one bisectable commit; gates: `tsc --noEmit`
     clean + `vitest` green.
   - **Add vitest (jsdom) render/behaviour tests** for the migrated components +
     hooks, so as much as possible is auto-verified.
   - **User verifies the settings page** at each phase milestone (the pixel-level
     gate the agent can't perform).

## Target structure — `ui/src/features/settings/`

```
index.ts                 # barrel — the ONLY public surface; other features import
                         #   from here, never from internal files (rule 1).
components/              # presentation only, each ≤ ~300 lines, NO direct invoke
  SettingsTabs.tsx       #   (if present) the tab shell
  SystemTab.tsx          #   thin shell composing the cards below
  system/                #   SystemTab (865) split into focused cards
    DiagnosticsCard.tsx
    ServicesCard.tsx
    HttpApiToggleCard.tsx
    EvalsCard.tsx
  GeneralTab.tsx / AgentSettings.tsx / PromptsSettings.tsx /
  BrowserRuntimeSettings.tsx (607 → split) / AppearanceSettings.tsx /
  AboutSettings.tsx / ProxySetting.tsx / SttSettings.tsx / LearnedProfileTab.tsx …
hooks/                   # side effects (IPC calls, polling, state machines)
  useHttpApiToggle.ts / useSystemDiagnostics.ts / …
atoms/                   # jotai atoms scoped to settings (if any today)
lib/                     # settings-only NON-IPC helpers (formatters, etc.)
```

- **IPC** goes through `ui/src/lib/bridge/settings.ts` (already exists); any
  settings commands still inside the `tauri-bridge.ts` god-bridge are **moved**
  into it. Components/atoms never touch `@tauri-apps/api` directly.
- **Side effects** live in `hooks/`; components are pure presentation.
- **No cross-feature deep imports**: consumers import `features/settings` (barrel).

## Phases

Each phase = one bisectable commit. Per-phase gate: `tsc` clean · `vitest`
(incl. the new render tests) green · no cross-feature deep imports (`grep`) ·
every touched component ≤ ~300 lines · **you verify the settings page renders +
its toggles/actions work** at the milestone.

- **P0 — scaffold + bridge consolidation.** Create `features/settings/` +
  `index.ts` barrel. Move any settings-domain IPC still in `tauri-bridge.ts` into
  `lib/bridge/settings.ts`. No component moves yet. Gate: `tsc`.
- **P1 — SystemTab.** Migrate `SystemTab` (865) into
  `features/settings/components/system/*` (Diagnostics / Services / HttpApiToggle /
  Evals cards) + extract side effects to `hooks/`. Add render tests. **You verify
  the 系统诊断 page** (diagnostics renders, the HTTP-API toggle works).
- **P2 — mid settings tabs.** `GeneralTab` / `AgentSettings` / `PromptsSettings`
  (split any > ~300) + hooks + render tests. You verify those tabs.
- **P3 — BrowserRuntimeSettings + remainder.** `BrowserRuntimeSettings` (607 →
  split) + the remaining settings components (Appearance, About, Proxy, Stt,
  LearnedProfile, …) + tests. You verify.
- **P4 — repoint + delete.** Repoint every consumer import (`components/settings/*`
  → the `features/settings` barrel); delete the old `components/settings/*`. Gate:
  `tsc` + `grep` confirms no remaining deep imports. **You verify the whole
  settings surface.**

## Verification & acceptance

- **Per phase**: `tsc --noEmit` clean; `vitest` (existing + new render/behaviour
  tests for the phase's components/hooks) green; `grep` shows no cross-feature
  deep imports and no direct `@tauri-apps/api` import in components; each touched
  component ≤ ~300 lines; the user confirms the relevant settings page at the
  milestone.
- **Overall (done-when)**: `features/settings/` is the sole home of settings UI;
  `components/settings/` is deleted; all settings IPC is in `lib/bridge/settings.ts`
  (none in `tauri-bridge.ts`); no component over the cap; the user confirms the
  full settings surface works.

## The repeatable pattern (after settings)

Each remaining domain (agent, chat, app-shell, …) copies this template in its own
spec → plan → PR: consolidate its IPC into `lib/bridge/<domain>.ts`, create
`features/<domain>/` (components/hooks/atoms/lib + barrel), split its
god-components, add render tests, repoint + delete the old `components/<domain>/`.
The largest (agent: `AgentView` 1926, `AgentMessages` 1277) come once the pattern
is proven on settings.

## Out of scope

- Other domains (agent, chat, …) — separate PRs after this pilot.
- The backend (already migrated).
- `tauri-specta` / `ts-rs` type generation (ADR rule 5's end-state) — a separate
  follow-up; this pilot keeps the hand-written bridge signatures.
- An E2E (Playwright-on-Tauri) harness — out of scope; verification is
  tsc + vitest + the user's milestone checks.
