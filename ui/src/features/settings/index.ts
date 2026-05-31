// Public surface of the settings feature — other features import ONLY from here,
// never from internal files (code-organization ADR 2026-05-31, rule 1).
//
// Components/hooks are added to this barrel as they migrate (plan P1–P3:
// docs/superpowers/plans/2026-05-31-frontend-settings-feature-migration.md).
export { settingsBridge } from '../../lib/bridge/settings'

// P1 — SystemTab migrated + split into system/ cards + hooks.
export { SystemTab } from './components/SystemTab'

// P2 — GeneralTab migrated (thin composer; sub-sections migrate later).
export { GeneralTab } from './components/GeneralTab'

// P2 — AgentSettings migrated (presentation; plan-mode persist in a hook).
export { AgentSettings } from './components/AgentSettings'

// P2 — PromptsSettings migrated (presentation; load/save IPC in a hook).
export { PromptsSettings } from './components/PromptsSettings'

// P3 — BrowserRuntimeSettings migrated (thin shell; side effects in a hook,
// split into browser-runtime/ cards + lib/browser-runtime-format helpers).
export { BrowserRuntimeSettings } from './components/BrowserRuntimeSettings'

// P3 — AppearanceSettings migrated (theme cards; atom-helper side effects, no IPC).
export { AppearanceSettings } from './components/AppearanceSettings'
