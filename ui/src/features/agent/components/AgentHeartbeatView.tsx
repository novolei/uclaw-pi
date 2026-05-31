/**
 * AgentHeartbeatView — the presentational half of AgentHeartbeatBanner
 * (Bundle 27-A), extracted during the features/agent migration. Renders the
 * three minimal, status-bar-style blocks from the state the useAgentHeartbeat
 * hook computes:
 *
 *   - boot-time recovery banner (neutral, dismissible)
 *   - stall banner (yellow, actionable: 中断并保存 / 继续等待)
 *   - live heartbeat chip (small, only while a run is active)
 *
 * Pure presentation: all data + actions arrive as props. The inline keyframes
 * for the dot pulse are kept here so the view doesn't depend on global CSS.
 */

import * as React from 'react'
import type { Beat, StallState } from '../hooks/useAgentHeartbeat'
import type { RecoveryPayload } from '@/lib/bridge/agent'

// Translate stage labels into a brief human-readable hint so the user
// understands WHERE the agent is currently working. Falls through to
// raw stage for anything not pre-mapped.
function stageHint(stage: string): string {
  if (stage.startsWith('tool_call:')) {
    const tool = stage.slice('tool_call:'.length)
    return `正在调用工具 ${tool}`
  }
  switch (stage) {
    case 'starting':
      return '准备中'
    case 'llm_call':
      return '正在请求 LLM'
    case 'llm_stream':
      return '正在接收 LLM 流式响应'
    case 'thinking':
      return '正在推理'
    case 'tool_call':
      return '正在调用工具'
    case 'done':
      return '已完成'
    default:
      return stage
  }
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`
  const s = Math.floor(ms / 1000)
  if (s < 60) return `${s}s`
  const m = Math.floor(s / 60)
  return `${m}m${s % 60}s`
}

export interface AgentHeartbeatViewProps {
  beat: Beat
  stall: StallState
  recovery: RecoveryPayload | null
  recoveryDismissed: boolean
  interrupting: boolean
  onInterrupt: () => void
  onKeepWaiting: () => void
  onDismissRecovery: () => void
}

export function AgentHeartbeatView({
  beat,
  stall,
  recovery,
  recoveryDismissed,
  interrupting,
  onInterrupt,
  onKeepWaiting,
  onDismissRecovery,
}: AgentHeartbeatViewProps): React.ReactElement {
  return (
    <>
      {/* Boot-time recovery banner — neutral, dismissible */}
      {recovery && !recoveryDismissed && (
        <div
          role="status"
          style={{
            margin: '8px 12px',
            padding: '10px 12px',
            background: 'var(--color-surface-2, #f3f4f6)',
            border: '1px solid var(--color-border, #e5e7eb)',
            borderLeft: '3px solid #6b7280',
            borderRadius: 6,
            fontSize: 13,
            lineHeight: 1.5,
          }}
        >
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'flex-start',
              gap: 8,
            }}
          >
            <strong>上一轮被异常中断 — 已恢复部分回复</strong>
            <button
              type="button"
              onClick={onDismissRecovery}
              style={{
                background: 'transparent',
                border: 'none',
                cursor: 'pointer',
                fontSize: 18,
                lineHeight: 1,
                opacity: 0.6,
              }}
              aria-label="关闭恢复提示"
            >
              ×
            </button>
          </div>
          <div style={{ marginTop: 4, opacity: 0.7, fontSize: 11 }}>
            iter={recovery.iteration} · stage={recovery.stage} · pid=
            {recovery.deadPid} · {recovery.partialChars} chars
          </div>
          <pre
            style={{
              marginTop: 8,
              padding: 8,
              background: 'var(--color-surface, #fafafa)',
              border: '1px solid var(--color-border, #e5e7eb)',
              borderRadius: 4,
              fontSize: 12,
              maxHeight: 160,
              overflow: 'auto',
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
              fontFamily:
                'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
            }}
          >
            {recovery.partialText}
          </pre>
        </div>
      )}

      {/* Stall banner — yellow, actionable */}
      {stall.kind === 'stalled' && (
        <div
          role="alert"
          style={{
            margin: '8px 12px',
            padding: '10px 12px',
            background: '#fffbeb',
            border: '1px solid #fde68a',
            borderLeft: '3px solid #f59e0b',
            borderRadius: 6,
            fontSize: 13,
            lineHeight: 1.5,
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            gap: 12,
          }}
        >
          <div>
            <strong>Agent 似乎卡住了</strong>
            <span style={{ marginLeft: 6, opacity: 0.85 }}>
              ({stageHint(stall.data.stage)} · 已 {formatDuration(stall.data.stalledForMs)} 无活动
              {stall.data.partialChars > 0
                ? ` · 已收到 ${stall.data.partialChars} chars`
                : ''}
              )
            </span>
          </div>
          <div style={{ display: 'flex', gap: 8 }}>
            <button
              type="button"
              onClick={onInterrupt}
              disabled={interrupting}
              style={{
                padding: '4px 10px',
                fontSize: 12,
                background: '#f59e0b',
                color: '#fff',
                border: 'none',
                borderRadius: 4,
                cursor: interrupting ? 'wait' : 'pointer',
                opacity: interrupting ? 0.6 : 1,
              }}
            >
              {interrupting ? '中断中…' : '中断并保存'}
            </button>
            <button
              type="button"
              onClick={onKeepWaiting}
              style={{
                padding: '4px 10px',
                fontSize: 12,
                background: 'transparent',
                color: '#92400e',
                border: '1px solid #fbbf24',
                borderRadius: 4,
                cursor: 'pointer',
              }}
            >
              继续等待
            </button>
          </div>
        </div>
      )}

      {/* Live heartbeat indicator — small, only when a run is active */}
      {beat && stall.kind === 'none' && (
        <div
          aria-live="polite"
          style={{
            margin: '4px 12px',
            padding: '4px 10px',
            background: 'transparent',
            color: 'var(--color-text-2, #6b7280)',
            fontSize: 11,
            lineHeight: 1.4,
            display: 'flex',
            alignItems: 'center',
            gap: 6,
          }}
        >
          <span
            aria-hidden="true"
            style={{
              display: 'inline-block',
              width: 6,
              height: 6,
              borderRadius: '50%',
              background:
                beat.lastActivityMsAgo > 10_000 ? '#f59e0b' : '#10b981',
              animation:
                beat.lastActivityMsAgo > 10_000
                  ? 'none'
                  : 'uclaw-heartbeat-pulse 1.6s ease-in-out infinite',
            }}
          />
          <span>
            iter {beat.iteration} · {stageHint(beat.stage)}
            {beat.lastActivityMsAgo > 2_000
              ? ` · ${formatDuration(beat.lastActivityMsAgo)} ago`
              : ''}
            {beat.partialChars > 0 ? ` · ${beat.partialChars} chars` : ''}
          </span>
        </div>
      )}

      {/* Inline keyframes so we don't depend on global CSS for the dot
          pulse animation. Scoped per-instance, harmless if duplicated. */}
      <style>{`
        @keyframes uclaw-heartbeat-pulse {
          0%, 100% { opacity: 1; transform: scale(1); }
          50% { opacity: 0.4; transform: scale(0.85); }
        }
      `}</style>
    </>
  )
}
