import * as React from 'react'
import { cn } from '@/lib/utils'
import { MEMORY_KIND_CLASSES, MEMORY_KIND_LABELS, type MemoryKind } from './kinds'

export interface KindBadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  kind: MemoryKind
  /**
   * `label` — tinted pill (default): use where the kind is the point.
   * `dot` — colored dot + muted label: use in dense lists / metadata rows.
   */
  variant?: 'label' | 'dot'
  size?: 'sm' | 'md'
  /** Overrides the default Chinese label. */
  label?: string
}

const SIZE = {
  label: {
    sm: 'h-[18px] gap-1 px-1.5 text-[11px]',
    md: 'h-5 gap-1.5 px-2 text-[12px]',
  },
  dot: {
    sm: 'gap-1 text-[11px]',
    md: 'gap-1.5 text-[12px]',
  },
} as const

/**
 * KindBadge — the one way memory surfaces show a memory's kind.
 *
 * @example <KindBadge kind="preference" />
 * @example <KindBadge kind="lesson" variant="dot" size="sm" />
 */
export function KindBadge({
  kind,
  variant = 'label',
  size = 'sm',
  label,
  className,
  ...props
}: KindBadgeProps): React.ReactElement {
  const c = MEMORY_KIND_CLASSES[kind]
  const text = label ?? MEMORY_KIND_LABELS[kind]

  return (
    <span
      data-kind={kind}
      data-variant={variant}
      className={cn(
        'inline-flex shrink-0 select-none items-center whitespace-nowrap font-medium leading-none',
        variant === 'label'
          ? cn('rounded-full border', c.surface, c.text, c.border)
          : 'text-muted-foreground',
        SIZE[variant][size],
        className,
      )}
      {...props}
    >
      <span
        aria-hidden
        className={cn(
          'shrink-0 rounded-full',
          variant === 'label' ? 'size-[5px]' : 'size-1.5',
          c.dot,
        )}
      />
      {text}
    </span>
  )
}
