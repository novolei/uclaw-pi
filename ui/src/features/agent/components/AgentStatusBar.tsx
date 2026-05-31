/**
 * AgentStatusBar — sticky one-liner above the input bar showing
 * the agent's current execution status. Always visible (doesn't
 * scroll with conversation), so the user knows whether the agent
 * is still working even when scrolled away from the streaming bubble.
 *
 * Renders only when the session has an active stream. Auto-fades
 * for 3s after completion to confirm "done", then disappears.
 *
 * Reads everything from `agentStreamingStatesAtom` — no new IPC.
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Loader2, Square, Wrench, Brain, AlertTriangle, CheckCircle2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import { agentStreamingStatesAtom, type AgentStreamState } from '@/atoms/agent-atoms'
import { stopAgent } from '@/lib/bridge/agent'

export interface AgentStatusBarProps {
  sessionId: string
}

interface StatusSnapshot {
  phase: 'thinking' | 'tool' | 'retrying' | 'idle' | 'complete'
  toolName?: string
  toolCount: number
  elapsedMs: number
  costUsd?: number
  inputTokens?: number
  outputTokens?: number
  retryAttempt?: { current: number; max: number }
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`
  const sec = ms / 1000
  if (sec < 60) return `${sec.toFixed(1)}s`
  const m = Math.floor(sec / 60)
  const s = Math.floor(sec % 60)
  return `${m}m ${s}s`
}

function formatTokens(n?: number): string {
  if (!n) return '0'
  if (n < 1000) return String(n)
  return `${(n / 1000).toFixed(1)}k`
}

function buildSnapshot(s: AgentStreamState, startedAt: number | undefined, now: number): StatusSnapshot {
  const elapsedMs = startedAt ? now - startedAt : 0
  const inFlight = s.toolActivities.find((t) => !t.done)
  const common = {
    toolCount: s.toolActivities.length,
    elapsedMs,
    costUsd: s.costUsd,
    inputTokens: s.inputTokens,
    outputTokens: s.outputTokens,
  }
  if (s.retrying && !s.retrying.failed) {
    return {
      ...common,
      phase: 'retrying',
      retryAttempt: { current: s.retrying.currentAttempt, max: s.retrying.maxAttempts },
    }
  }
  if (inFlight) {
    return {
      ...common,
      phase: 'tool',
      toolName: inFlight.displayName || inFlight.toolName,
    }
  }
  if (s.reasoning) {
    return { ...common, phase: 'thinking' }
  }
  return { ...common, phase: 'idle' }
}

export function AgentStatusBar({ sessionId }: AgentStatusBarProps): React.ReactElement | null {
  const streamingStates = useAtomValue(agentStreamingStatesAtom)
  const streamState = streamingStates.get(sessionId)
  const running = streamState?.running ?? false
  const startedAt = streamState?.startedAt

  // Tick once a second so the duration display refreshes.
  const [now, setNow] = React.useState(() => Date.now())
  React.useEffect(() => {
    if (!running) return
    const t = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(t)
  }, [running])

  // Track "just-completed" state so we can briefly show a "done" pill (3s fade).
  const [justCompleted, setJustCompleted] = React.useState<StatusSnapshot | null>(null)
  const wasRunningRef = React.useRef(false)
  React.useEffect(() => {
    if (wasRunningRef.current && !running && streamState && startedAt) {
      setJustCompleted({
        phase: 'complete',
        toolCount: streamState.toolActivities.length,
        elapsedMs: Date.now() - startedAt,
        costUsd: streamState.costUsd,
        inputTokens: streamState.inputTokens,
        outputTokens: streamState.outputTokens,
      })
      const t = setTimeout(() => setJustCompleted(null), 3000)
      return () => clearTimeout(t)
    }
    wasRunningRef.current = running
  }, [running, streamState, startedAt])

  if (!running && !justCompleted) return null
  const snap: StatusSnapshot = justCompleted ?? buildSnapshot(streamState!, startedAt, now)

  return <StatusBarRow sessionId={sessionId} snap={snap} isComplete={!!justCompleted} />
}

function StatusBarRow({
  sessionId,
  snap,
  isComplete,
}: {
  sessionId: string
  snap: StatusSnapshot
  isComplete: boolean
}): React.ReactElement {
  const handleStop = React.useCallback(async () => {
    try {
      await stopAgent(sessionId)
    } catch (e) {
      console.error('[AgentStatusBar] stop failed:', e)
    }
  }, [sessionId])

  const { Icon, label, accentClass, bgClass, iconAnimate } = phaseTheme(snap, isComplete)

  return (
    <div
      className={cn(
        'flex items-center gap-2 px-3 py-1.5 text-[12px] border-t border-border/40',
        'animate-in fade-in slide-in-from-bottom-1 duration-200',
        bgClass,
      )}
    >
      <Icon className={cn('size-3.5 shrink-0', accentClass, iconAnimate)} />
      <span className={cn('font-medium', accentClass)}>{label}</span>
      {snap.toolName && (
        <code className="px-1.5 py-0.5 rounded bg-foreground/5 font-mono text-[11.5px] text-foreground/85 truncate max-w-[300px]">
          {snap.toolName}
        </code>
      )}
      <span className="text-muted-foreground/60">·</span>
      <span className="text-muted-foreground tabular-nums">{formatDuration(snap.elapsedMs)}</span>
      {snap.toolCount > 0 && (
        <>
          <span className="text-muted-foreground/60">·</span>
          <span className="text-muted-foreground tabular-nums">{snap.toolCount} 工具</span>
        </>
      )}
      {snap.retryAttempt && (
        <>
          <span className="text-muted-foreground/60">·</span>
          <span className="text-amber-700 dark:text-amber-400 tabular-nums">
            重试 {snap.retryAttempt.current}/{snap.retryAttempt.max}
          </span>
        </>
      )}
      {(snap.inputTokens || snap.outputTokens) && (
        <>
          <span className="text-muted-foreground/60">·</span>
          <span className="text-muted-foreground/80 tabular-nums text-[11px]">
            ↑{formatTokens(snap.inputTokens)} ↓{formatTokens(snap.outputTokens)}
          </span>
        </>
      )}
      {snap.costUsd != null && snap.costUsd > 0 && (
        <>
          <span className="text-muted-foreground/60">·</span>
          <span className="text-muted-foreground/80 tabular-nums text-[11px]">
            ${snap.costUsd.toFixed(4)}
          </span>
        </>
      )}

      <div className="flex-1" />

      {!isComplete && (
        <button
          type="button"
          onClick={() => void handleStop()}
          className={cn(
            'flex items-center gap-1 px-2 py-0.5 rounded text-[11px]',
            'bg-foreground/5 hover:bg-red-500/15 text-muted-foreground hover:text-red-600',
            'transition-colors',
          )}
          title="停止当前任务"
        >
          <Square className="size-3" fill="currentColor" />
          <span>停止</span>
        </button>
      )}
    </div>
  )
}

function phaseTheme(snap: StatusSnapshot, isComplete: boolean): {
  Icon: React.ComponentType<{ className?: string }>
  label: string
  accentClass: string
  bgClass: string
  iconAnimate?: string
} {
  if (isComplete) {
    return {
      Icon: CheckCircle2,
      label: '已完成',
      accentClass: 'text-green-700 dark:text-green-400',
      bgClass: 'bg-green-500/8',
    }
  }
  switch (snap.phase) {
    case 'retrying':
      return {
        Icon: AlertTriangle,
        label: '重试中',
        accentClass: 'text-amber-700 dark:text-amber-400',
        bgClass: 'bg-amber-500/8',
      }
    case 'tool':
      return {
        Icon: Wrench,
        label: '执行工具',
        accentClass: 'text-primary',
        bgClass: 'bg-primary/5',
      }
    case 'thinking':
      return {
        Icon: Brain,
        label: '思考中',
        accentClass: 'text-purple-700 dark:text-purple-400',
        bgClass: 'bg-purple-500/8',
      }
    case 'idle':
    default:
      return {
        Icon: Loader2,
        label: '准备中',
        accentClass: 'text-muted-foreground',
        bgClass: 'bg-muted/40',
        iconAnimate: 'animate-spin',
      }
  }
}
