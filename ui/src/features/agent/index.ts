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

// TaskBadge — single running background-task pill. Pure presentation (no IPC).
export { TaskBadge } from './components/TaskBadge'

// TaskProgressCard — inline aggregated task/todo progress card. Pure
// presentation (no IPC). `TASK_TOOL_NAMES` is re-exported for the renderers
// that gate on it (ToolActivityItem / SDKMessageRenderer).
export { TaskProgressCard, TASK_TOOL_NAMES } from './components/TaskProgressCard'
