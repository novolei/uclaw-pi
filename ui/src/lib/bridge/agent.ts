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
} from '../tauri-bridge'

// The wire enum the permission-mode picker maps over (`ask`/`acceptedits`/
// `plan`/`supervised`/`yolo`). Re-exported as a type so migrated components
// import it from this bridge instead of reaching back into the monolith.
export type { SafetyModeWire } from '../tauri-bridge'

// ── Session self-evaluation event stream (was `@tauri-apps/api/event` `listen`
// directly inside SessionEvalBadge). The component subscribes through these
// wrappers so no `features/agent` component imports `@tauri-apps/api`. ──
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

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
