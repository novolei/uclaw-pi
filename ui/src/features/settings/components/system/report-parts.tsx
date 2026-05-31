// Presentational sub-components for the diagnostics report. Moved verbatim out
// of `components/settings/SystemTab.tsx` during the P1 split so `DiagnosticsCard`
// stays under the size cap. Pure presentation — no IPC, no side effects.
import * as React from 'react'
import { cn } from '@/lib/utils'
import type { GbrainInitStatus } from '../../lib/diagnostics-types'

export function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <p className="text-[10px] uppercase tracking-wider text-muted-foreground font-medium">{title}</p>
      {children}
    </div>
  )
}

export function Grid4({ children }: { children: React.ReactNode }) {
  return <div className="grid grid-cols-2 gap-x-8 gap-y-2">{children}</div>
}

export function InfoCell({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between py-1.5 border-b border-border/40">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="text-xs text-foreground font-mono">{value}</span>
    </div>
  )
}

export function BridgeCard({ name, subtitle, running, detail, diagnostics = [] }: {
  name: string; subtitle: string; running: boolean; detail: string; diagnostics?: Array<string | null>
}) {
  const visibleDiagnostics = diagnostics.filter(Boolean) as string[]
  return (
    <div className="rounded-lg bg-muted/40 px-3 py-2.5">
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-2">
          <span className={cn('size-2 rounded-full flex-shrink-0', running ? 'bg-green-500' : 'bg-muted-foreground/40')} />
          <span className="text-sm font-medium text-foreground">{name}</span>
          <span className="text-xs text-muted-foreground">({subtitle})</span>
        </div>
        <span className={cn('text-xs text-right', running ? 'text-green-400' : 'text-muted-foreground')}>{detail}</span>
      </div>
      {visibleDiagnostics.length > 0 && (
        <div className="mt-2 space-y-0.5 border-t border-border/40 pt-2">
          {visibleDiagnostics.map((line, idx) => (
            <div key={idx} className="break-all text-[11px] leading-4 text-muted-foreground">
              {line}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

// Sprint 2.2.5b — surface the gbrain init outcome with actionable copy.
// Each status branch picks an appropriate dot color + 1-line message +
// optional remediation hint.
export function GbrainInitRow({ status }: { status: GbrainInitStatus }) {
  let dotClass = 'bg-muted-foreground/40'
  let label = '初始化未尝试'
  let detail = ''
  let hint: string | null = null

  switch (status.status) {
    case 'in_progress':
      dotClass = 'bg-yellow-400 animate-pulse'
      label = '初始化进行中'
      detail = '首次启动 — PGlite 正在跑 ~63 次迁移 (30-60s)'
      break
    case 'succeeded':
      dotClass = 'bg-green-500'
      label = '初始化成功'
      detail = `首次初始化耗时 ${(status.duration_ms / 1000).toFixed(1)}s`
      break
    case 'skipped_already_initialized':
      dotClass = 'bg-green-500'
      label = '已初始化'
      detail = 'PGlite 数据库已就绪'
      break
    case 'failed':
      dotClass = 'bg-red-500'
      label = '初始化失败'
      detail = status.error
      hint = '运行 scripts/init-gbrain.sh 或删除 ~/.uclaw/gbrain/ 后重启'
      break
    case 'bundle_missing':
      dotClass = 'bg-red-500'
      label = 'bundle 缺失'
      detail = 'bunembed/bun 或 gbrain-source 未找到'
      hint = '运行 scripts/setup-bun-runtime.sh + scripts/setup-gbrain-source.sh'
      break
    case 'not_attempted':
      // Caller filters this out, but TS demands exhaustive match.
      break
  }

  return (
    <div className="mt-2 rounded-lg bg-muted/30 px-3 py-2 text-xs">
      <div className="flex items-center gap-2">
        <span className={cn('size-2 rounded-full flex-shrink-0', dotClass)} />
        <span className="font-medium text-foreground">gbrain init</span>
        <span className="text-muted-foreground">— {label}</span>
      </div>
      {detail && (
        <div className="mt-1 pl-4 text-muted-foreground">{detail}</div>
      )}
      {hint && (
        <div className="mt-1 pl-4 text-amber-400">{hint}</div>
      )}
    </div>
  )
}
