/**
 * WorkspaceRollupSection — per-workspace spend for the current month (a labelled
 * bar list; renders null when empty). Split out of the 393-line
 * legacy settings/UsageSettings.tsx during the features/settings migration
 * (code-organization ADR 2026-05-31). Behavior preserved verbatim.
 */
import * as React from 'react'
import type { WorkspaceCostRollup } from '@/lib/types'
import { getWorkspaceIcon } from '@/lib/workspace-icons'
import { formatUsdShort } from '../../lib/usage-format'

export function WorkspaceRollupSection({ items }: { items: WorkspaceCostRollup[] }): React.ReactElement | null {
  if (items.length === 0) return null
  const max = Math.max(...items.map((i) => i.totalCostUsd), 0.0001)
  return (
    <section>
      <h3 className="mb-2 text-[12px] font-semibold uppercase tracking-wide text-muted-foreground/80">按工作区（本月）</h3>
      <div className="space-y-1.5">
        {items.map((r) => {
          const Icon = getWorkspaceIcon(r.workspaceIcon)
          return (
            <div key={r.workspaceId} className="flex items-center gap-2.5 rounded-md border border-border/40 bg-card/60 px-3 py-2">
              <span className="inline-flex items-center justify-center size-5 rounded bg-primary/15 text-primary shrink-0">
                <Icon className="size-3.5" />
              </span>
              <span className="flex-1 truncate text-[12.5px] text-foreground/85">{r.workspaceName || '默认工作区'}</span>
              <div className="relative h-1.5 w-24 overflow-hidden rounded-full bg-muted">
                <div className="h-full rounded-full bg-primary/70" style={{ width: `${(r.totalCostUsd / max) * 100}%` }} />
              </div>
              <span className="w-16 shrink-0 text-right text-[12px] tabular-nums text-foreground/80">{formatUsdShort(r.totalCostUsd)}</span>
            </div>
          )
        })}
      </div>
    </section>
  )
}
