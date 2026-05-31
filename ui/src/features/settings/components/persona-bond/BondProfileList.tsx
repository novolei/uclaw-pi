// PersonaBondTimeline: 关系档案 list. Extracted verbatim out of the 560-line
// components/settings/PersonaBondTimeline during the features/settings split
// (code-organization ADR 2026-05-31).
import type { BondProfile } from '@/lib/persona-types'

export function BondProfileList({ bond }: { bond?: BondProfile }) {
  const rows = [
    ['协作节奏', bond?.collaborationRhythm ?? []],
    ['挑战契约', bond?.challengeContract ?? []],
    ['支持风格', bond?.supportStyle ?? []],
    ['不喜欢的表达', bond?.communicationDislikes ?? []],
  ] as const

  return (
    <div className="mt-3 grid gap-2 sm:grid-cols-2">
      {rows.map(([label, values]) => (
        <div key={label} className="rounded bg-muted/20 p-2">
          <div className="text-[11px] font-medium text-muted-foreground">{label}</div>
          <div className="mt-1 space-y-1 text-xs leading-5 text-foreground">
            {values.length > 0 ? (
              values.map((value) => <div key={value}>{value}</div>)
            ) : (
              <div className="text-muted-foreground">等待共同经历沉淀。</div>
            )}
          </div>
        </div>
      ))}
    </div>
  )
}
