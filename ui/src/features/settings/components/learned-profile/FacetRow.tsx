/**
 * FacetRow — one learned-preference row: `{name}: {value}` + state badge +
 * stability/evidence/last-seen meta, with a hover-revealed promote/demote/dismiss
 * action cluster. Split out of legacy settings/LearnedProfileTab.tsx during the
 * features/settings migration (code-organization ADR 2026-05-31). Pure presentation.
 * Behavior preserved verbatim (Sprint 2.3 visibility rules unchanged).
 */
import * as React from 'react'
import { Loader2, X, ChevronUp, ChevronDown } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn, formatDateTime } from '@/lib/utils'
import type { FacetDto } from '@/lib/types'
import { stateBadgeTone } from '../../lib/facet-class'

interface FacetRowProps {
  facet: FacetDto
  busy: boolean
  onDismiss: () => void
  onPromote: () => void
  onDemote: () => void
}

export function FacetRow({
  facet,
  busy,
  onDismiss,
  onPromote,
  onDemote,
}: FacetRowProps): React.ReactElement {
  const state = facet.state.toLowerCase()
  const forgotten = state === 'forgotten'
  // Sprint 2.3 — visibility rules: promote when state is anything other
  // than 'active' (i.e. provisional / candidate / forgotten — all can be
  // lifted), demote only from 'active' / 'provisional' (no-op below).
  const canPromote = state !== 'active'
  const canDemote = state === 'active' || state === 'provisional'
  return (
    <li
      className={cn(
        'group flex items-center justify-between gap-3 px-3 py-2 rounded-md bg-muted/20 border border-border/30',
        forgotten && 'opacity-60',
      )}
      data-facet-id={facet.facetId}
    >
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-xs font-medium text-foreground truncate">
            {facet.name}
          </span>
          <span className="text-xs text-muted-foreground">:</span>
          <span
            className={cn(
              'text-xs text-foreground truncate',
              forgotten && 'line-through',
            )}
          >
            {facet.value}
          </span>
        </div>
        <div className="flex items-center gap-2 mt-1 text-[10px] text-muted-foreground/70">
          <span
            className={cn(
              'px-1.5 py-0 border rounded text-[10px]',
              stateBadgeTone(facet.state),
            )}
          >
            {facet.state}
          </span>
          <span>stability {facet.stability.toFixed(2)}</span>
          <span>· evidence {facet.evidenceCount}</span>
          <span>· {formatDateTime(facet.lastSeenAtMs)}</span>
        </div>
      </div>
      {/* Sprint 2.3 — action cluster: promote / demote / dismiss.
          Buttons reveal on row-hover (group-hover:opacity-100) so the
          row stays visually quiet at rest. The currently-busy button
          shows its spinner regardless of hover. */}
      <div
        className={cn(
          'flex items-center gap-0.5 transition-opacity',
          busy ? 'opacity-100' : 'opacity-0 group-hover:opacity-100',
        )}
      >
        {canPromote && (
          <Button
            size="sm"
            variant="ghost"
            className="h-7 w-7 p-0 text-muted-foreground hover:text-green-600 dark:hover:text-green-400"
            onClick={onPromote}
            disabled={busy}
            title="提升为 active — 下次系统提示词会包含它"
            aria-label={`promote-${facet.facetId}`}
          >
            <ChevronUp className="size-3.5" />
          </Button>
        )}
        {canDemote && (
          <Button
            size="sm"
            variant="ghost"
            className="h-7 w-7 p-0 text-muted-foreground hover:text-amber-600 dark:hover:text-amber-400"
            onClick={onDemote}
            disabled={busy}
            title="降级为 provisional — 不再出现在系统提示词里，但保留观察"
            aria-label={`demote-${facet.facetId}`}
          >
            <ChevronDown className="size-3.5" />
          </Button>
        )}
        {!forgotten && (
          <Button
            size="sm"
            variant="ghost"
            className="h-7 w-7 p-0 text-muted-foreground hover:text-destructive"
            onClick={onDismiss}
            disabled={busy}
            title="标记为「忘掉」（下次有新证据时还会再出现）"
            aria-label={`dismiss-${facet.facetId}`}
          >
            {busy ? <Loader2 className="size-3 animate-spin" /> : <X className="size-3" />}
          </Button>
        )}
      </div>
    </li>
  )
}
