/**
 * useSessionTrajectory — loads the per-turn breakdown for a session and tracks
 * loading / error state. Extracted from TrajectoryReel during the features/agent
 * migration so the reel stays presentational.
 *
 * Behavior (unchanged from the inline version): on mount / sessionId change it
 * resets to loading, clears the turns, fetches via the agent bridge
 * (`getSessionTrajectory` — no `@tauri-apps/api` here), and guards the async
 * resolution with a `cancelled` flag so a session switch mid-flight never
 * applies stale data.
 */

import * as React from 'react'
import { getSessionTrajectory, type TurnRecord } from '@/lib/bridge/agent'

export interface UseSessionTrajectory {
  turns: TurnRecord[]
  loading: boolean
  error: string | null
}

export function useSessionTrajectory(sessionId: string): UseSessionTrajectory {
  const [turns, setTurns] = React.useState<TurnRecord[]>([])
  const [loading, setLoading] = React.useState(true)
  const [error, setError] = React.useState<string | null>(null)

  React.useEffect(() => {
    let cancelled = false

    setLoading(true)
    setError(null)
    setTurns([])

    getSessionTrajectory(sessionId)
      .then((data) => {
        if (!cancelled) {
          setTurns(data)
          setLoading(false)
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          const msg = err instanceof Error ? err.message : String(err)
          setError(msg)
          setLoading(false)
        }
      })

    return () => {
      cancelled = true
    }
  }, [sessionId])

  return { turns, loading, error }
}
