// Public surface of the agent feature — other features import ONLY from here,
// never from internal files (code-organization ADR 2026-05-31, rule 1). This
// mirrors the proven settings-feature migration
// (docs/superpowers/plans/2026-05-31-frontend-settings-feature-migration.md).
//
// The agent IPC bridge is re-exported as named functions (matching how
// `lib/bridge/agent.ts` shapes the domain — unlike settings' single object).
export * from '../../lib/bridge/agent'

// ── First migration batch (smallest self-contained leaves). Each component is
// added to this barrel in its own bisectable commit as it migrates. ──

// AgentHeader — session title bar (edit-in-place + file-panel toggle). Title
// persist routed through the agent bridge (updateAgentSessionTitle /
// listAgentSessions).
export { AgentHeader } from './components/AgentHeader'

// AgentStatusBar — sticky execution-status one-liner above the input bar. Reads
// the streaming-state atom; the Stop button routes through the agent bridge.
export { AgentStatusBar } from './components/AgentStatusBar'

// MoveSessionDialog — move-session-to-workspace dialog. The move IPC is routed
// through the agent bridge (moveAgentSessionToWorkspace).
export { MoveSessionDialog } from './components/MoveSessionDialog'

// PermissionModeMenu — the 5-mode Radix popover (presentation + keyboard). No
// IPC. `MODE_ITEMS` is re-exported for PermissionModeSelector + tests.
export { PermissionModeMenu, MODE_ITEMS } from './components/PermissionModeMenu'

// PermissionModeSelector — input-bar trigger that opens PermissionModeMenu;
// hydrates + persists SafetyMode through the agent bridge (getSafetyPolicy /
// setSafetyMode).
export { PermissionModeSelector } from './components/PermissionModeSelector'

// StrategyPresetSelector — skill-manifest re-rank bias dropdown. Pure
// atom-driven UI (no IPC).
export { StrategyPresetSelector } from './components/StrategyPresetSelector'

// SessionEvalBadge — post-session self-evaluation score badge. The raw
// @tauri-apps/api/event `listen` moved behind the agent bridge's
// onSessionEvalComplete / onSessionEvalWarning wrappers.
export { SessionEvalBadge } from './components/SessionEvalBadge'

// TaskBadge — single running background-task pill. Pure presentation (no IPC).
export { TaskBadge } from './components/TaskBadge'

// TaskProgressCard — inline aggregated task/todo progress card. Pure
// presentation (no IPC). `TASK_TOOL_NAMES` is re-exported for the renderers
// that gate on it (ToolActivityItem / SDKMessageRenderer).
export { TaskProgressCard, TASK_TOOL_NAMES } from './components/TaskProgressCard'

// ── Second migration batch: the interactive approval/status banners that sit
// above the composer in AgentView. IPC routed through the agent bridge;
// side-effects (event subscriptions / timers) extracted into hooks/. ──

// PermissionBanner — inline FIFO permission-request prompt (allow / deny /
// always-allow). stopAgent + respondPermission routed through the agent bridge.
export { PermissionBanner } from './components/PermissionBanner'

// QueuedMessagesBanner — Codex-style queue card above the composer
// (steer / edit / delete). Pure presentation: actions arrive as callback
// props, no IPC.
export { QueuedMessagesBanner } from './components/QueuedMessagesBanner'

// PlanModeSuggestBanner — advisory plan-mode auto-suggest banner. The
// `agent:plan_mode_suggest` subscription + respondPlanModeSuggest / setSafetyMode
// IPC route through the agent bridge (onPlanModeSuggest wrapper).
export { PlanModeSuggestBanner } from './components/PlanModeSuggestBanner'
