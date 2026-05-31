/**
 * SessionEvalBadge — shows the agent's self-evaluation score after a session ends.
 *
 * Listens to:
 *  - session:eval-complete  → renders score bar + reasoning
 *  - session:eval-warning   → adds a yellow warning indicator
 */

import * as React from 'react'
import { AlertTriangle, Star } from 'lucide-react'
import {
  onSessionEvalComplete,
  onSessionEvalWarning,
  type SessionEvalPayload,
} from '@/lib/bridge/agent'

interface SessionEvalBadgeProps {
  sessionId: string
}

function ScoreBar({ score }: { score: number }): React.ReactElement {
  const pct = Math.round(score * 100)
  const color = score >= 0.75
    ? 'bg-green-500'
    : score >= 0.5
    ? 'bg-yellow-500'
    : 'bg-red-500'

  return (
    <div className="flex items-center gap-2">
      <div className="flex-1 h-1.5 rounded-full bg-muted overflow-hidden">
        <div
          className={`h-full rounded-full transition-all duration-500 ${color}`}
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className={`text-[11px] font-semibold tabular-nums ${
        score >= 0.75 ? 'text-green-500' : score >= 0.5 ? 'text-yellow-500' : 'text-red-500'
      }`}>
        {pct}%
      </span>
    </div>
  )
}

export function SessionEvalBadge({ sessionId }: SessionEvalBadgeProps): React.ReactElement | null {
  const [eval_, setEval] = React.useState<SessionEvalPayload | null>(null)
  const [warning, setWarning] = React.useState(false)
  const [expanded, setExpanded] = React.useState(false)

  React.useEffect(() => {
    let cancelled = false
    const unlistens: Array<() => void> = []

    onSessionEvalComplete((payload) => {
      if (payload.sessionId === sessionId) {
        setEval(payload)
        if (payload.score < 0.5) setWarning(true)
      }
    }).then((fn) => {
      if (cancelled) fn()
      else unlistens.push(fn)
    })

    onSessionEvalWarning((payload) => {
      if (payload.sessionId === sessionId) setWarning(true)
    }).then((fn) => {
      if (cancelled) fn()
      else unlistens.push(fn)
    })

    return () => {
      cancelled = true
      unlistens.forEach((fn) => fn())
    }
  }, [sessionId])

  if (!eval_) return null

  return (
    <div className={`mt-2 rounded-lg border p-2.5 ${
      warning ? 'border-yellow-500/40 bg-yellow-500/5' : 'border-border/50 bg-muted/20'
    }`}>
      <button
        onClick={() => setExpanded((v) => !v)}
        className="w-full flex items-center gap-2 text-left"
      >
        {warning
          ? <AlertTriangle size={12} className="text-yellow-500 flex-shrink-0" />
          : <Star size={12} className="text-green-500 flex-shrink-0" />
        }
        <span className="text-[11px] font-medium flex-1">
          Self-Evaluation
        </span>
        <ScoreBar score={eval_.score} />
      </button>

      {expanded && (
        <div className="mt-2 space-y-1.5">
          <p className="text-[11px] text-muted-foreground leading-snug">
            {eval_.reasoning}
          </p>
          {eval_.learnings && eval_.learnings.length > 0 && (
            <div>
              <p className="text-[10px] font-medium text-foreground/60 mb-1">Learnings:</p>
              <ul className="space-y-0.5">
                {eval_.learnings.map((l, i) => (
                  <li key={i} className="text-[10px] text-muted-foreground flex gap-1.5">
                    <span className="text-foreground/30 flex-shrink-0">·</span>
                    <span>{l}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </div>
  )
}
