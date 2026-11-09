import * as React from 'react'
import { cn } from '@/lib/utils'

/**
 * MemorySkeleton — placeholder block for content that is loading.
 *
 * Size it to the real content's box (e.g. `h-3 w-2/3` for a title line)
 * so nothing shifts when data lands. A faint sheen sweeps across; with
 * `prefers-reduced-motion` it is a still muted block. Purely decorative —
 * announce loading on the container (`aria-busy`) instead.
 */
export function MemorySkeleton({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>): React.ReactElement {
  return (
    <div
      aria-hidden
      data-part="skeleton"
      className={cn(
        'rounded-md bg-muted',
        'bg-[linear-gradient(90deg,transparent_0%,hsl(var(--foreground)/0.05)_50%,transparent_100%)] bg-[length:200%_100%] bg-no-repeat',
        'motion-safe:animate-memory-shimmer',
        className,
      )}
      {...props}
    />
  )
}
