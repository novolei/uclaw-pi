/**
 * ClassGroup — one facet-class section (label + description + the facet rows, or a
 * "(还没学到)" placeholder when empty). Split out of
 * legacy settings/LearnedProfileTab.tsx during the features/settings migration
 * (code-organization ADR 2026-05-31). Pure presentation. Behavior preserved verbatim.
 */
import * as React from 'react'
import type { FacetDto } from '@/lib/types'
import { CLASS_LABEL, CLASS_DESCRIPTION } from '../../lib/facet-class'
import { FacetRow } from './FacetRow'

interface ClassGroupProps {
  className: string
  facets: FacetDto[]
  dismissing: Set<string>
  onDismiss: (id: string) => void
  onPromote: (id: string) => void
  onDemote: (id: string) => void
}

export function ClassGroup({
  className,
  facets,
  dismissing,
  onDismiss,
  onPromote,
  onDemote,
}: ClassGroupProps): React.ReactElement {
  const label = CLASS_LABEL[className] ?? className
  const description = CLASS_DESCRIPTION[className]
  return (
    <section data-class-group={className}>
      <div className="mb-2">
        <h3 className="text-xs font-medium text-foreground">{label}</h3>
        {description && (
          <p className="text-[10px] text-muted-foreground/70 mt-0.5">
            {description}
          </p>
        )}
      </div>
      {facets.length === 0 ? (
        <p className="text-[11px] text-muted-foreground/50 italic px-2 py-1">
          （还没学到）
        </p>
      ) : (
        <ul className="space-y-1">
          {facets.map((f) => (
            <FacetRow
              key={f.facetId}
              facet={f}
              busy={dismissing.has(f.facetId)}
              onDismiss={() => onDismiss(f.facetId)}
              onPromote={() => onPromote(f.facetId)}
              onDemote={() => onDemote(f.facetId)}
            />
          ))}
        </ul>
      )}
    </section>
  )
}
