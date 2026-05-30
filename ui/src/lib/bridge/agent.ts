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
} from '../tauri-bridge'
