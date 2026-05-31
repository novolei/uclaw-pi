// 评估套件 — the autonomous regression eval suites (browser / memory / agent /
// self-improvement gates). Moved verbatim out of `components/settings/SystemTab.tsx`
// during the P1 split; side effects live in `useEvalRunner`. Never calls IPC directly.
import * as React from 'react'
import { Activity, PlayCircle, RefreshCw } from 'lucide-react'
import { cn } from '@/lib/utils'
import { useEvalRunner } from '../../hooks/useEvalRunner'
import { evalCommands, type EvalSuiteReport } from '../../lib/diagnostics-types'

export function EvalsCard({ onError }: { onError?: (m: string) => void }) {
  const { evalReports, busy, run, runAll } = useEvalRunner(onError)
  return (
    <div className="flex flex-col gap-2">
      <p className="text-[10px] uppercase tracking-wider text-muted-foreground font-medium">评估套件</p>
      <div className="rounded-lg border border-border/50 bg-muted/20">
        <div className="flex items-center justify-between gap-3 border-b border-border/50 px-3 py-2">
          <div className="flex min-w-0 items-center gap-2">
            <Activity size={14} className="text-muted-foreground" />
            <div className="min-w-0">
              <div className="text-sm font-medium text-foreground">自治回归套件</div>
              <div className="text-[11px] text-muted-foreground">
                运行 Browser、Memory、Agent 与自我改进 gates
              </div>
            </div>
          </div>
          <div className="flex shrink-0 flex-wrap justify-end gap-2">
            <EvalButton label="All" busy={busy === 'all'} onClick={runAll} disabled={Boolean(busy)} />
            <EvalButton label="Browser" busy={busy === 'browser'} onClick={() => run('browser')} disabled={Boolean(busy)} />
            <EvalButton label="Memory" busy={busy === 'memory'} onClick={() => run('memory')} disabled={Boolean(busy)} />
            <EvalButton label="Agent" busy={busy === 'agent'} onClick={() => run('agent')} disabled={Boolean(busy)} />
            <EvalButton label="Self" busy={busy === 'self'} onClick={() => run('self')} disabled={Boolean(busy)} />
          </div>
        </div>
        <div className="space-y-2 p-3">
          <EvalSummary name="browser parity" report={evalReports.browser} />
          <EvalSummary name="memory/gbrain" report={evalReports.memory} />
          <EvalSummary name="agent control-plane" report={evalReports.agent} />
          <EvalSummary name="self-improvement gates" report={evalReports.self} />
          {!evalReports.browser && !evalReports.memory && !evalReports.agent && !evalReports.self && (
            <div className="text-xs text-muted-foreground">
              尚未运行。结果会显示通过率、平均分和失败 case 的具体检查项。
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

function EvalButton({ label, busy, disabled, onClick }: {
  label: string; busy: boolean; disabled?: boolean; onClick: () => void
}) {
  return (
    <button
      onClick={onClick}
      disabled={busy || disabled}
      className="flex min-h-8 cursor-pointer items-center gap-1.5 rounded-md border border-border/60 bg-background px-2.5 text-xs text-foreground transition-colors hover:bg-accent disabled:cursor-default disabled:opacity-50"
    >
      {busy ? <RefreshCw size={12} className="animate-spin" /> : <PlayCircle size={12} />}
      {label}
    </button>
  )
}

function EvalSummary({ name, report }: { name: string; report: EvalSuiteReport | null }) {
  if (!report) return null
  const failed = report.scorecards.filter(card => !card.passed)
  return (
    <div className="rounded-md bg-background/70 px-3 py-2">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="truncate text-sm font-medium text-foreground">{name}</div>
          <div className="text-[11px] text-muted-foreground">
            {report.scorecards.length} cases · {report.runIds.length} runs
          </div>
        </div>
        <div className="shrink-0 text-right">
          <div className={cn('text-xs font-medium', report.passed ? 'text-green-400' : 'text-red-400')}>
            {report.passed ? '通过' : '失败'}
          </div>
          <div className="font-mono text-[11px] text-muted-foreground">
            {(report.averageScore * 100).toFixed(0)}%
          </div>
        </div>
      </div>
      <div className="mt-2 overflow-hidden rounded border border-border/40">
        {report.scorecards.map(card => (
          <div
            key={card.caseId}
            className="grid grid-cols-[1fr_auto] gap-2 border-b border-border/40 px-2 py-1.5 last:border-b-0"
          >
            <div className="min-w-0">
              <div className="truncate text-xs text-foreground">{card.title}</div>
              {!card.passed && (
                <div className="mt-0.5 text-[11px] text-red-400">
                  {card.checks.filter(check => !check.passed).map(check => check.id).join(', ')}
                </div>
              )}
            </div>
            <div className={cn('font-mono text-[11px]', card.passed ? 'text-green-400' : 'text-red-400')}>
              {(card.score * 100).toFixed(0)}%
            </div>
          </div>
        ))}
      </div>
      {failed.length > 0 && (
        <div className="mt-2 text-[11px] leading-4 text-muted-foreground">
          首个失败：{failed[0].checks.find(check => !check.passed)?.message ?? failed[0].title}
        </div>
      )}
    </div>
  )
}
