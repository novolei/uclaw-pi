// Bundle 27-A — Live agent heartbeat + stall banner + interrupted-reply
// recovery banner. One component owns all three states because they
// share the same event source and the same "show only for THIS
// session" guard.
//
// Thin shell (features/agent migration): all four event sources +
// pull-on-mount recovery + interrupt/dismiss IPC live in the
// `useAgentHeartbeat` hook (routed through the agent bridge); the three
// presentational blocks live in `AgentHeartbeatView`. This file just wires
// the hook's state into the view.
//
// Event sources from the backend (see
// src-tauri/src/agent/heartbeat.rs + recovery.rs):
//   - `agent:heartbeat`              — every 5s while a run is active.
//   - `agent:stalled`               — fires once when no activity for ≥30s.
//   - `agent:stall-recovered`       — fires when activity resumes after a stall.
//   - `agent:interrupted-recovered` — boot-time recovery of a dead run's text.

import * as React from 'react'
import { useAgentHeartbeat } from '../hooks/useAgentHeartbeat'
import { AgentHeartbeatView } from './AgentHeartbeatView'

interface AgentHeartbeatBannerProps {
  sessionId: string
}

export function AgentHeartbeatBanner({ sessionId }: AgentHeartbeatBannerProps): React.ReactElement {
  const {
    beat,
    stall,
    recovery,
    recoveryDismissed,
    interrupting,
    handleInterrupt,
    handleKeepWaiting,
    handleDismissRecovery,
  } = useAgentHeartbeat(sessionId)

  return (
    <AgentHeartbeatView
      beat={beat}
      stall={stall}
      recovery={recovery}
      recoveryDismissed={recoveryDismissed}
      interrupting={interrupting}
      onInterrupt={handleInterrupt}
      onKeepWaiting={handleKeepWaiting}
      onDismissRecovery={handleDismissRecovery}
    />
  )
}

export default AgentHeartbeatBanner
