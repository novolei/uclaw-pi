// Clamped numeric input shared by the MemoryRecall cards. Extracted verbatim out
// of the inline NumberInput in the 474-line legacy settings/MemoryRecallSettings
// during the features/settings split. Clamp-on-change + re-clamp-on-blur behavior
// preserved exactly.
import { clamp } from '../../lib/memory-recall'

export function NumberInput({
  value,
  min,
  max,
  step = 1,
  onChange,
}: {
  value: number
  min: number
  max: number
  step?: number
  onChange: (v: number) => void
}): React.ReactElement {
  return (
    <input
      type="number"
      value={value}
      min={min}
      max={max}
      step={step}
      onChange={(e) => {
        const raw = e.target.value
        if (raw === '' || raw === '-') return // allow clearing, treat as unchanged
        const parsed = Number(raw)
        if (!isNaN(parsed)) {
          onChange(clamp(parsed, min, max))
        }
      }}
      onBlur={(e) => {
        // Re-clamp on blur to catch edge cases
        const parsed = Number(e.target.value)
        if (!isNaN(parsed)) {
          const clamped = clamp(parsed, min, max)
          if (clamped !== parsed) onChange(clamped)
        } else {
          onChange(min) // fallback
        }
      }}
      className="w-24 h-7 text-xs text-right rounded-md border border-border bg-muted/40 px-2 focus:outline-none focus:ring-1 focus:ring-ring focus:border-ring"
    />
  )
}
