/**
 * Agent-session bridge (§2A.3). Re-exports the agent commands from the legacy
 * monolith `lib/tauri-bridge.ts` under a domain-scoped module so components can
 * `import { sendAgentMessage } from 'lib/bridge/agent'`. Command names + payload
 * shapes are unchanged (the frontend contract is preserved); the monolith is
 * split into these facades incrementally, then implementations move in.
 */
export {
  sendAgentMessage,
  stopAgent,
  createAgentSession,
  listAgentSessions,
  getAgentSessionMessages,
  agentSteer,
  agentFollowUp,
  queueAgentMessage,
  estimateSessionContext,
  migrateChatToAgent,
  approveToolCall,
  // ── Session-management commands surfaced for the migrated `features/agent`
  // leaves (AgentHeader title edit, MoveSessionDialog workspace move). Thin
  // re-exports — signatures + `.catch` fallbacks are unchanged. ──
  updateAgentSessionTitle,
  moveAgentSessionToWorkspace,
  // ── Safety-mode commands surfaced for the migrated PermissionModeSelector
  // (the input-bar 5-mode picker, backed by the real SafetyManager). ──
  getSafetyPolicy,
  setSafetyMode,
  // ── Interactive-banner response commands surfaced for the migrated
  // approval banners (PermissionBanner, AskUserBanner, ExitPlanModeBanner,
  // PlanModeSuggestBanner). Thin re-exports — command names, payload shapes,
  // and `.catch` fallbacks are unchanged. ──
  respondPermission,
  respondAskUser,
  respondExitPlanMode,
  respondPlanModeSuggest,
  // ── Read-side session data surfaced for the migrated TrajectoryReel
  // (per-turn breakdown reel). Thin re-export — command name + payload shape
  // unchanged. ──
  getSessionTrajectory,
  // ── Best-effort observability bump surfaced for the migrated
  // SkillCitationChips (records that an applied-skill citation was rendered).
  // Thin re-export — command name + payload shape unchanged. ──
  recordSkillCited,
  // ── Skill catalog reads surfaced for the migrated SkillSuggestionBar
  // (local fuzzy-match suggestions while typing). Thin re-exports — command
  // names + payload shapes unchanged. ──
  listSkills,
  listLearnedSkills,
  // ── Open-in-system-browser surfaced for the migrated BrowserPreviewOverlay
  // (the "open in OS browser" affordance on the live preview). Thin re-export —
  // delegates to the shell-open command, signature unchanged. ──
  openExternal,
  // ── Tool-result image IO surfaced for the migrated ToolActivityItem
  // (ActivityDetails renders generated images: read the attachment bytes, then
  // save-as). Thin re-exports — command names + payload shapes unchanged. ──
  readAttachment,
  saveImageAs,
} from '../tauri-bridge'

// Per-turn record shape returned by `get_session_trajectory`, surfaced so the
// migrated TrajectoryReel imports it from this bridge instead of the monolith.
export type { TurnRecord } from '../tauri-bridge'

// The wire enum the permission-mode picker maps over (`ask`/`acceptedits`/
// `plan`/`supervised`/`yolo`). Re-exported as a type so migrated components
// import it from this bridge instead of reaching back into the monolith.
export type { SafetyModeWire } from '../tauri-bridge'

// Payload shape for `respond_exit_plan_mode`, surfaced so the migrated
// ExitPlanModeBanner imports it from this bridge instead of the monolith.
export type { RespondExitPlanModeInput } from '../tauri-bridge'

// One Agent-Teams channel message, surfaced so the migrated Teams components
// (AgentTeamsPanel, ChannelFeed) import it from this bridge instead of reaching
// back into the monolith. Shape unchanged.
export type { TeamChannelMessage } from '../tauri-bridge'

// ── Session self-evaluation event stream (was `@tauri-apps/api/event` `listen`
// directly inside SessionEvalBadge). The component subscribes through these
// wrappers so no `features/agent` component imports `@tauri-apps/api`. ──
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import type { TeamChannelMessage } from '../tauri-bridge'

/** Payload emitted on `session:eval-complete` / `session:eval-warning`. */
export interface SessionEvalPayload {
  sessionId: string
  score: number
  reasoning: string
  learnings?: string[]
}

/** Subscribe to the agent's post-session self-evaluation result. */
export const onSessionEvalComplete = (
  handler: (payload: SessionEvalPayload) => void,
): Promise<UnlistenFn> =>
  listen<SessionEvalPayload>('session:eval-complete', (e) => handler(e.payload))

/** Subscribe to the low-score self-evaluation warning signal. */
export const onSessionEvalWarning = (
  handler: (payload: SessionEvalPayload) => void,
): Promise<UnlistenFn> =>
  listen<SessionEvalPayload>('session:eval-warning', (e) => handler(e.payload))

// ── Live-heartbeat / stall / interrupted-recovery wiring (Bundle 27-A).
// Was a tangle of raw `@tauri-apps/api/event` `listen` + `@tauri-apps/api/core`
// `invoke` calls directly inside AgentHeartbeatBanner. The migrated
// `useAgentHeartbeat` hook subscribes + invokes through these wrappers so no
// `features/agent` component imports `@tauri-apps/api`. Event names + command
// names + payload shapes are unchanged. ──

/** `agent:heartbeat` — emitted every ~5s while a run is active. */
export interface HeartbeatPayload {
  conversationId: string
  iteration: number
  stage: string
  lastActivityMsAgo: number
  partialChars: number
  timestamp: number
}

/** `agent:stalled` — fires once when no activity for ≥30s. */
export interface StalledPayload {
  conversationId: string
  iteration: number
  stage: string
  stalledForMs: number
  partialChars: number
  timestamp: number
}

/** `agent:interrupted-recovered` — boot-time recovery of a dead run's text. */
export interface RecoveryPayload {
  conversationId: string
  spaceId: string
  iteration: number
  stage: string
  startedAt: number
  lastActivityAt: number
  partialText: string
  partialChars: number
  deadPid: number
}

/** Payload returned by `interrupt_current_agent_run` (recovered partial text). */
export interface InterruptRunResult {
  partialText: string
  iteration: number
  stage: string
  stalledForMs: number
  startedAt: number
}

/** Subscribe to live heartbeat ticks for an active run. */
export const onAgentHeartbeat = (
  handler: (payload: HeartbeatPayload) => void,
): Promise<UnlistenFn> =>
  listen<HeartbeatPayload>('agent:heartbeat', (e) => handler(e.payload))

/** Subscribe to the "run stalled" signal. */
export const onAgentStalled = (
  handler: (payload: StalledPayload) => void,
): Promise<UnlistenFn> =>
  listen<StalledPayload>('agent:stalled', (e) => handler(e.payload))

/** Subscribe to the "stall recovered" signal (activity resumed). */
export const onAgentStallRecovered = (
  handler: (payload: { conversationId: string }) => void,
): Promise<UnlistenFn> =>
  listen<{ conversationId: string }>('agent:stall-recovered', (e) => handler(e.payload))

/** Subscribe to boot-time interrupted-run recovery (push path). */
export const onAgentInterruptedRecovered = (
  handler: (payload: RecoveryPayload) => void,
): Promise<UnlistenFn> =>
  listen<RecoveryPayload>('agent:interrupted-recovered', (e) => handler(e.payload))

/** Subscribe to stream completion (clears the heartbeat indicator). */
export const onChatStreamComplete = (
  handler: (payload: { conversationId: string }) => void,
): Promise<UnlistenFn> =>
  listen<{ conversationId: string }>('chat:stream-complete', (e) => handler(e.payload))

/**
 * Pull-model recovery: ask the backend whether a pending recovery exists for
 * THIS session. One-shot — first caller with the matching session_id wins.
 * Returns the recovery payload or `null`/`undefined` when nothing pending.
 */
export const consumePendingRecovery = (
  sessionId: string,
): Promise<RecoveryPayload | null | undefined> =>
  invoke<RecoveryPayload | null | undefined>('consume_pending_recovery', { sessionId })

/** Tell the backend to drop a buffered recovery payload (banner dismissed). */
export const dismissPendingRecovery = (): Promise<void> =>
  invoke<void>('dismiss_pending_recovery')

/**
 * Cancel the current run and return the recovered partial text. Treated by the
 * UI like an `agent:interrupted-recovered` event so the banner converges.
 */
export const interruptCurrentAgentRun = (
  sessionId: string,
): Promise<InterruptRunResult> =>
  invoke<InterruptRunResult>('interrupt_current_agent_run', { sessionId })

// ── Plan-mode auto-suggest event stream (was `@tauri-apps/api/event` `listen`
// directly inside PlanModeSuggestBanner). The backend emits snake_case keys
// via `serde_json::json!`, so the payload type lives with the atom; this
// wrapper just forwards the raw payload. ──

/** Subscribe to the `agent:plan_mode_suggest` advisory signal. */
export const onPlanModeSuggest = <P>(
  handler: (payload: P) => void,
): Promise<UnlistenFn> =>
  listen<P>('agent:plan_mode_suggest', (e) => handler(e.payload))

// ── Live plan-file updates (was `@tauri-apps/api/event` `listen('plan:updated')`
// directly inside PlanViewer). The migrated PlanViewer subscribes through this
// wrapper so no `features/agent` component imports `@tauri-apps/api`. The
// backend emits `{ filename, content }` from the plan_write tool. ──

/** Payload emitted on `plan:updated` when the agent rewrites a plan file. */
export interface PlanUpdatedPayload {
  filename: string
  content: string
}

/** Subscribe to live plan-file rewrites for the open plan. */
export const onPlanUpdated = (
  handler: (payload: PlanUpdatedPayload) => void,
): Promise<UnlistenFn> =>
  listen<PlanUpdatedPayload>('plan:updated', (e) => handler(e.payload))

// ── Agent-Teams channel stream (was `@tauri-apps/api/event` `listen` directly
// inside AgentTeamsPanel). The migrated panel subscribes through this wrapper so
// no `features/agent` component imports `@tauri-apps/api`. Event name + payload
// shape are unchanged. ──

/** Subscribe to live Agent-Teams channel messages for the active team run. */
export const onTeamMessage = (
  handler: (payload: TeamChannelMessage) => void,
): Promise<UnlistenFn> =>
  listen<TeamChannelMessage>('agent:team-message', (e) => handler(e.payload))
