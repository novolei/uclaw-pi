// Shared bordered panel used across the PersonaBondTimeline sections. Extracted
// verbatim out of the 560-line legacy settings/PersonaBondTimeline during the
// features/settings split (code-organization ADR 2026-05-31).
import * as React from 'react'

export function Panel({
  title,
  icon,
  children,
}: {
  title: string
  icon: React.ReactNode
  children: React.ReactNode
}) {
  return (
    <div className="rounded-md border border-border/50 p-3">
      <div className="flex items-center gap-2 text-xs font-medium text-foreground">
        {icon}
        {title}
      </div>
      {children}
    </div>
  )
}
