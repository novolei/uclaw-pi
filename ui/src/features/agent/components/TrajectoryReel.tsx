import * as React from 'react'
import { useSessionTrajectory } from '../hooks/useSessionTrajectory'
import { SessionEvalBadge } from './SessionEvalBadge'

function roleIcon(role: string): string {
  const r = role.toLowerCase()
  if (r === 'assistant') return '🤖'
  if (r === 'user') return '👤'
  return '⚙️'
}

function formatDuration(ms: number): string {
  if (ms >= 1000) {
    return `${(ms / 1000).toFixed(1)}s`
  }
  return `${Math.round(ms)}ms`
}

function truncate(text: string, max: number): string {
  if (text.length <= max) return text
  return text.slice(0, max) + '…'
}

interface TrajectoryReelProps {
  sessionId: string
}

export function TrajectoryReel({ sessionId }: TrajectoryReelProps): React.ReactElement {
  const { turns, loading, error } = useSessionTrajectory(sessionId)

  if (loading) {
    return (
      <div className="p-3 text-[11px] text-muted-foreground">
        Loading trajectory…
      </div>
    )
  }

  if (error) {
    return (
      <div className="p-3 text-[11px] text-red-500">
        Failed to load trajectory: {error}
      </div>
    )
  }

  if (turns.length === 0) {
    return (
      <div className="p-3 text-[11px] text-muted-foreground">
        No turns recorded for this session.
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-1 p-3 overflow-y-auto">
      {turns.map((turn) => (
        <div
          key={turn.id}
          className={`flex items-start gap-2 p-2 rounded-md bg-muted/30 ${
            turn.isError ? 'border border-red-500/40' : ''
          }`}
        >
          {/* Icon */}
          <span className="text-[13px] leading-none mt-0.5 shrink-0">{roleIcon(turn.role)}</span>

          {/* Body */}
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-1.5 flex-wrap">
              <span className="text-[10px] text-muted-foreground shrink-0">#{turn.turnIndex}</span>
              <span className={`text-[10px] font-medium ${turn.isError ? 'text-red-500' : 'text-foreground'}`}>
                {turn.role}
              </span>

              {turn.toolName && (
                <>
                  <span className="rounded bg-muted px-1 py-px text-[10px] font-mono text-foreground">
                    {turn.toolName}
                  </span>
                  <span className="text-[10px] text-muted-foreground">
                    {formatDuration(turn.durationMs)}
                  </span>
                </>
              )}

              {turn.isError && (
                <span className="text-[10px] text-red-500 font-medium">error</span>
              )}
            </div>

            {turn.content && (
              <p
                className={`text-[11px] mt-0.5 leading-snug ${
                  turn.isError ? 'text-red-400' : 'text-muted-foreground'
                }`}
              >
                {truncate(turn.content, 120)}
              </p>
            )}
          </div>
        </div>
      ))}

      {/* Self-evaluation badge — appears when agent calls self_eval tool */}
      <SessionEvalBadge sessionId={sessionId} />
    </div>
  )
}
