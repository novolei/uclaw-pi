/**
 * UsageSettings — Settings → 用量与预算 tab.
 *
 * Thin shell: all data + the live re-fetch wiring live in useUsageData; the
 * sections are split into usage/ presentation components (BudgetHeader,
 * WorkspaceRollupSection, UsageCharts) + lib/usage-format helpers. Split out of
 * the 393-line legacy settings/UsageSettings.tsx during the features/settings
 * migration (code-organization ADR 2026-05-31). Behavior preserved verbatim.
 *
 * Sections (top to bottom):
 *   - BudgetHeader: month-to-date spend + progress bar vs. configured budget
 *   - WorkspaceRollupSection: per-workspace spend for the current month
 *   - UsageCharts: KPI cards + daily bar + per-model donut + per-session table
 */
import * as React from 'react'
import { useUsageData } from '../hooks/useUsageData'
import { BudgetHeader } from './usage/BudgetHeader'
import { WorkspaceRollupSection } from './usage/WorkspaceRollupSection'
import { UsageCharts } from './usage/UsageCharts'

export function UsageSettings(): React.ReactElement {
  const {
    daily, models, sessions, loading, totals,
    monthTotal, wsRollup, budget, saveBudget,
  } = useUsageData()

  return (
    <div className="space-y-6 pb-8">
      <BudgetHeader total={monthTotal ?? 0} budget={budget} onSave={(v) => void saveBudget(v)} />
      <WorkspaceRollupSection items={wsRollup} />
      <UsageCharts daily={daily} models={models} sessions={sessions} loading={loading} totals={totals} />
    </div>
  )
}
