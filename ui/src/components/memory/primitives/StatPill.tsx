import * as React from 'react'
import { cn } from '@/lib/utils'

export type StatTone = 'neutral' | 'success' | 'warning' | 'danger'

export interface StatPillProps extends React.HTMLAttributes<HTMLSpanElement> {
  label: React.ReactNode
  value: React.ReactNode
  tone?: StatTone
}

const TONE: Record<StatTone, { root: string; value: string }> = {
  neutral: { root: 'bg-muted border-border', value: 'text-foreground' },
  success: { root: 'bg-success-bg border-transparent', value: 'text-success' },
  warning: { root: 'bg-warning-bg border-transparent', value: 'text-warning' },
  danger: { root: 'bg-danger-bg border-transparent', value: 'text-danger' },
}

/**
 * StatPill — a quiet label/value pair ("记忆 1,204", "待索引 3").
 * The value carries the tone; the label stays muted so a row of pills
 * reads as data, not as a row of alerts.
 */
export function StatPill({
  label,
  value,
  tone = 'neutral',
  className,
  ...props
}: StatPillProps): React.ReactElement {
  const t = TONE[tone]
  return (
    <span
      data-tone={tone}
      className={cn(
        'inline-flex h-6 shrink-0 items-baseline gap-1.5 whitespace-nowrap rounded-full border px-2.5 text-[12px] leading-6',
        t.root,
        className,
      )}
      {...props}
    >
      <span className="text-muted-foreground">{label}</span>
      <span className={cn('font-medium tabular-nums', t.value)}>{value}</span>
    </span>
  )
}
