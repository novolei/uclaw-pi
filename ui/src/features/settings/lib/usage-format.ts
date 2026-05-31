// Pure USD/date formatting helpers + the donut palette for the 用量与预算 tab.
// Extracted out of components/settings/UsageSettings.tsx during the
// features/settings migration (code-organization ADR 2026-05-31) so the split
// sub-components (BudgetHeader, UsageCharts) share one copy. Behavior preserved
// verbatim — identical rounding thresholds.

export const PALETTE = [
  'hsl(220 70% 55%)', 'hsl(160 65% 45%)', 'hsl(30 80% 55%)',
  'hsl(280 60% 60%)', 'hsl(0 70% 60%)', 'hsl(180 60% 45%)',
]

export function formatUsd(v: number): string {
  if (v < 0.01) return `$${v.toFixed(4)}`
  return `$${v.toFixed(2)}`
}

export function formatUsdShort(v: number): string {
  if (v < 0.01) return `$${v.toFixed(4)}`
  if (v < 1) return `$${v.toFixed(3)}`
  return `$${v.toFixed(2)}`
}

export function formatDateChip(epochMs: number): string {
  const d = new Date(epochMs)
  return `${d.getMonth() + 1}/${d.getDate()}`
}
