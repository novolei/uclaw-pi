/**
 * useAgentHeartbeat — all the event/IPC wiring behind AgentHeartbeatBanner
 * (Bundle 27-A: live heartbeat chip + stall banner + interrupted-reply recovery).
 * Extracted during the features/agent migration so the banner stays a thin
 * presentational shell.
 *
 * Subscribes (per-session, guarded on conversationId) to:
 *   - agent:heartbeat            — every ~5s while a run is active.
 *   - agent:stalled              — fires once when no activity for ≥30s.
 *   - agent:stall-recovered      — fires when activity resumes after a stall.
 *   - agent:interrupted-recovered— boot-time recovery of a dead run's text.
 *   - chat:stream-complete       — clears the heartbeat indicator on run end.
 * Plus the pull-on-mount recovery probe (consume_pending_recovery) and the
 * interrupt + dismiss commands. All IPC routes through the agent bridge — no
 * `@tauri-apps/api` here.
 */

import * as React from 'react'
import type { UnlistenFn } from '@tauri-apps/api/event'
import {
  onAgentHeartbeat,
  onAgentStalled,
  onAgentStallRecovered,
  onAgentInterruptedRecovered,
  onChatStreamComplete,
  consumePendingRecovery,
  dismissPendingRecovery,
  interruptCurrentAgentRun,
  type HeartbeatPayload,
  type StalledPayload,
  type RecoveryPayload,
} from '@/lib/bridge/agent'

export type Beat = HeartbeatPayload | null
export type StallState =
  | { kind: 'none' }
  | { kind: 'stalled'; data: StalledPayload }

export interface UseAgentHeartbeat {
  beat: Beat
  stall: StallState
  recovery: RecoveryPayload | null
  recoveryDismissed: boolean
  interrupting: boolean
  /** [中断并保存] — cancel the run + surface the recovered partial text. */
  handleInterrupt: () => Promise<void>
  /** [继续等待] — dismiss the stall banner without acting. */
  handleKeepWaiting: () => void
  /** Dismiss the boot-time recovery banner + tell the backend to drop it. */
  handleDismissRecovery: () => void
}

export function useAgentHeartbeat(sessionId: string): UseAgentHeartbeat {
  const [beat, setBeat] = React.useState<Beat>(null)
  const [stall, setStall] = React.useState<StallState>({ kind: 'none' })
  const [recovery, setRecovery] = React.useState<RecoveryPayload | null>(null)
  // Whether the recovery banner is dismissed for this session.
  const [recoveryDismissed, setRecoveryDismissed] = React.useState(false)
  // Pending action — disables buttons while invoking the backend.
  const [interrupting, setInterrupting] = React.useState(false)

  // Listen to all events. Each useEffect returns the unlisten fn so React's
  // cleanup handles tear-down on session change / unmount.
  React.useEffect(() => {
    let unlisten: UnlistenFn | null = null
    onAgentHeartbeat((payload) => {
      if (payload.conversationId !== sessionId) return
      setBeat(payload)
    }).then((un) => {
      unlisten = un
    })
    return () => {
      if (unlisten) unlisten()
    }
  }, [sessionId])

  React.useEffect(() => {
    let unlisten: UnlistenFn | null = null
    onAgentStalled((payload) => {
      if (payload.conversationId !== sessionId) return
      setStall({ kind: 'stalled', data: payload })
    }).then((un) => {
      unlisten = un
    })
    return () => {
      if (unlisten) unlisten()
    }
  }, [sessionId])

  React.useEffect(() => {
    let unlisten: UnlistenFn | null = null
    onAgentStallRecovered((payload) => {
      if (payload.conversationId !== sessionId) return
      setStall({ kind: 'none' })
    }).then((un) => {
      unlisten = un
    })
    return () => {
      if (unlisten) unlisten()
    }
  }, [sessionId])

  // Listen to recovery events globally, then match on conversationId.
  // Combined push (event) + pull (invoke on mount) — see Bundle 27-A2.
  // The event-only push was unreliable because boot-time emit (500ms
  // after Tauri setup) can race with React mount in dev mode. The
  // pull-on-mount path queries backend AppState directly, so it
  // works even if the banner mounts AFTER the event fired.
  React.useEffect(() => {
    let unlisten: UnlistenFn | null = null
    onAgentInterruptedRecovered((payload) => {
      if (payload.conversationId !== sessionId) return
      setRecovery(payload)
      setRecoveryDismissed(false)
    }).then((un) => {
      unlisten = un
    })
    return () => {
      if (unlisten) unlisten()
    }
  }, [sessionId])

  // Bundle 27-A2 — pull-model recovery. On mount (or when sessionId
  // changes), ask the backend "is there a pending recovery for THIS
  // session?". If yes, render the banner. The backend's consume_*
  // command is one-shot — first caller with the matching session_id
  // wins.
  React.useEffect(() => {
    let cancelled = false
    consumePendingRecovery(sessionId)
      .then((payload) => {
        if (cancelled || !payload) return
        // payload shape matches RecoveryPayload (camelCase from JSON).
        setRecovery(payload)
        setRecoveryDismissed(false)
      })
      .catch((err) => {
        // eslint-disable-next-line no-console
        console.error('[Bundle 27-A2] consume_pending_recovery failed', err)
      })
    return () => {
      cancelled = true
    }
  }, [sessionId])

  // Listen for stream completion — clear the heartbeat indicator
  // immediately when the agent run ends, rather than waiting for the
  // 15s stale-fade timer. Bundle 27-A initial draft only had the
  // stale-fade; users found the 15s lag confusing because the
  // streaming text was already done. `chat:stream-complete` is the
  // canonical "run finished" signal from dispatcher::emit_done.
  React.useEffect(() => {
    let unlisten: UnlistenFn | null = null
    onChatStreamComplete((payload) => {
      if (payload.conversationId !== sessionId) return
      setBeat(null)
      setStall({ kind: 'none' })
    }).then((un) => {
      unlisten = un
    })
    return () => {
      if (unlisten) unlisten()
    }
  }, [sessionId])

  // Heartbeat auto-fades when stale (no event in > 15s = run probably
  // ended; backend's `agent:stream-complete` would normally clear it
  // by emitting `done` stage first, but we belt-and-suspenders here).
  React.useEffect(() => {
    if (!beat) return
    const handle = setTimeout(() => setBeat(null), 15_000)
    return () => clearTimeout(handle)
  }, [beat])

  const handleInterrupt = React.useCallback(async () => {
    if (interrupting) return
    setInterrupting(true)
    try {
      // Backend cancels the run + returns the recovered partial text
      // payload. We treat it the same as an `agent:interrupted-
      // recovered` event so the UI converges.
      const payload = await interruptCurrentAgentRun(sessionId)
      if (payload?.partialText) {
        setRecovery({
          conversationId: sessionId,
          spaceId: '',
          iteration: payload.iteration,
          stage: payload.stage,
          startedAt: payload.startedAt,
          lastActivityAt: 0,
          partialText: payload.partialText,
          partialChars: payload.partialText.length,
          deadPid: 0,
        })
        setRecoveryDismissed(false)
      }
      setStall({ kind: 'none' })
    } catch (err) {
      // Surface to console — Tauri command errors are rare and we
      // don't want to block the user; the run will eventually
      // terminate via the stop_agent path even without this.
      // eslint-disable-next-line no-console
      console.error('[Bundle 27-A] interrupt_current_agent_run failed', err)
    } finally {
      setInterrupting(false)
    }
  }, [interrupting, sessionId])

  const handleKeepWaiting = React.useCallback(() => {
    setStall({ kind: 'none' })
  }, [])

  const handleDismissRecovery = React.useCallback(() => {
    setRecoveryDismissed(true)
    // Bundle 27-A2 — also tell backend to drop the payload so it doesn't
    // reappear on next mount.
    dismissPendingRecovery().catch((err) => {
      // eslint-disable-next-line no-console
      console.error('[Bundle 27-A2] dismiss_pending_recovery failed', err)
    })
  }, [])

  return {
    beat,
    stall,
    recovery,
    recoveryDismissed,
    interrupting,
    handleInterrupt,
    handleKeepWaiting,
    handleDismissRecovery,
  }
}
