/**
 * BudgetHeader — month-to-date spend + progress bar vs. the configured budget,
 * with an inline edit form (set / modify / clear). Split out of the 393-line
 * legacy settings/UsageSettings.tsx during the features/settings migration
 * (code-organization ADR 2026-05-31). Behavior preserved verbatim.
 */
import * as React from 'react'
import { formatUsdShort } from '../../lib/usage-format'

export function BudgetHeader({
  total, budget, onSave,
}: {
  total: number
  budget: number | null
  onSave: (v: number | null) => void
}): React.ReactElement {
  const [editing, setEditing] = React.useState(false)
  const [draft, setDraft] = React.useState<string>(budget?.toString() ?? '')

  if (budget == null) {
    return (
      <div className="rounded-xl border border-border/60 bg-card p-4">
        <div className="text-[12.5px] font-medium text-foreground/80">本月已使用 {formatUsdShort(total)}</div>
        <div className="mt-1 text-[11px] text-muted-foreground/70">设置月度预算后，达到 80% / 100% 会收到提醒。</div>
        {editing ? (
          <form
            onSubmit={(e) => {
              e.preventDefault()
              const v = parseFloat(draft)
              if (Number.isFinite(v) && v > 0) {
                onSave(v)
                setEditing(false)
              }
            }}
            className="mt-3 flex items-center gap-2"
          >
            <span className="text-[12px] text-muted-foreground/80">$</span>
            <input
              autoFocus
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              type="number" min="0" step="0.01" inputMode="decimal"
              className="w-24 rounded-md border border-border/60 bg-background px-2 py-1 text-[12.5px] outline-none focus:border-primary"
            />
            <button type="submit" className="rounded-md bg-primary px-2.5 py-1 text-[11.5px] text-primary-foreground hover:bg-primary/90">
              保存
            </button>
            <button type="button" onClick={() => setEditing(false)} className="text-[11.5px] text-muted-foreground/70 hover:text-foreground/80">
              取消
            </button>
          </form>
        ) : (
          <button
            type="button"
            onClick={() => setEditing(true)}
            className="mt-3 rounded-md border border-dashed border-border/70 bg-transparent px-3 py-1.5 text-[11.5px] text-muted-foreground/85 hover:border-primary/50 hover:text-foreground/90"
          >
            设置月度预算
          </button>
        )}
      </div>
    )
  }

  const pct = Math.min(total / budget, 1.5)
  const isOver = total > budget
  const isWarn = !isOver && total / budget >= 0.8

  return (
    <div className="rounded-xl border border-border/60 bg-card p-4">
      <div className="flex items-baseline justify-between">
        <div>
          <div className="text-[14px] font-semibold text-foreground/90">本月用量</div>
          <div className="mt-0.5 text-[11.5px] text-muted-foreground/70">
            {formatUsdShort(total)} / {formatUsdShort(budget)} ·{' '}
            <span className={isOver ? 'text-destructive font-medium' : isWarn ? 'text-amber-500 font-medium' : ''}>
              {Math.round((total / budget) * 100)}%
            </span>
          </div>
        </div>
        {editing ? (
          <form
            onSubmit={(e) => {
              e.preventDefault()
              if (draft === '') { onSave(null); setEditing(false); return }
              const v = parseFloat(draft)
              if (Number.isFinite(v) && v > 0) { onSave(v); setEditing(false) }
            }}
            className="flex items-center gap-1.5"
          >
            <span className="text-[12px] text-muted-foreground/80">$</span>
            <input
              autoFocus
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              type="number" min="0" step="0.01" inputMode="decimal"
              className="w-20 rounded-md border border-border/60 bg-background px-2 py-1 text-[12.5px] outline-none focus:border-primary"
              placeholder="预算"
            />
            <button type="submit" className="text-[11.5px] text-primary hover:underline">保存</button>
            <button type="button" onClick={() => setEditing(false)} className="text-[11.5px] text-muted-foreground/70 hover:text-foreground/80">×</button>
          </form>
        ) : (
          <button
            type="button"
            onClick={() => { setDraft(budget.toString()); setEditing(true) }}
            className="text-[11px] text-muted-foreground/70 hover:text-foreground/80 underline-offset-2 hover:underline"
          >
            修改预算
          </button>
        )}
      </div>
      <div className="mt-3 h-2 w-full overflow-hidden rounded-full bg-muted">
        <div
          className={`h-full rounded-full transition-all duration-500 ${
            isOver ? 'bg-destructive' : isWarn ? 'bg-amber-500' : 'bg-primary'
          }`}
          style={{ width: `${(pct / 1.5) * 100}%` }}
        />
      </div>
    </div>
  )
}
