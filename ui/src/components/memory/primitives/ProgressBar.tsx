import * as React from 'react'
import { cn } from '@/lib/utils'

export interface ProgressBarProps extends Omit<React.HTMLAttributes<HTMLDivElement>, 'role'> {
  /** Fraction complete, 0..1. Ignored when `indeterminate`. Clamped; NaN → 0. */
  value?: number
  /** Unknown total (e.g. waiting for the first byte): a slow sweep instead of a fill. */
  indeterminate?: boolean
  /** Accessible name — what is progressing ("下载记忆模型"). */
  label?: string
  size?: 'sm' | 'md'
}

function clamp01(v: number | undefined): number {
  if (v === undefined || Number.isNaN(v)) return 0
  return Math.min(1, Math.max(0, v))
}

/**
 * ProgressBar — hairline progress for downloads / indexing.
 *
 * Determinate fill glides (transform only, 240 ms ease-out) so bursty
 * progress events read as continuous. Indeterminate sweeps; with
 * `prefers-reduced-motion` the sweep is replaced by a static, dimmed fill.
 */
export function ProgressBar({
  value,
  indeterminate = false,
  label,
  size = 'sm',
  className,
  ...props
}: ProgressBarProps): React.ReactElement {
  const pct = Math.round(clamp01(value) * 100)

  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={indeterminate ? undefined : pct}
      aria-valuetext={indeterminate ? undefined : `${pct}%`}
      aria-busy={indeterminate || undefined}
      data-state={indeterminate ? 'indeterminate' : 'determinate'}
      className={cn(
        'relative w-full overflow-hidden rounded-full bg-progress-track',
        size === 'sm' ? 'h-1' : 'h-1.5',
        className,
      )}
      {...props}
    >
      {indeterminate ? (
        <div
          data-part="indicator"
          className={cn(
            'absolute inset-y-0 left-0 w-2/5 rounded-full bg-progress-fill',
            'motion-safe:animate-memory-indeterminate',
            'motion-reduce:w-full motion-reduce:opacity-40',
          )}
        />
      ) : (
        <div
          data-part="indicator"
          className={cn(
            'absolute inset-0 rounded-full bg-progress-fill',
            'transition-transform duration-[240ms] ease-out motion-reduce:transition-none',
          )}
          style={{ transform: `translateX(-${100 - pct}%)` }}
        />
      )}
    </div>
  )
}
