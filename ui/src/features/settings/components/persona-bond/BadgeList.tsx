// PersonaBondTimeline: 勋章 list. Extracted verbatim out of the 560-line
// components/settings/PersonaBondTimeline during the features/settings split
// (code-organization ADR 2026-05-31).
import { EyeOff, Loader2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import type { PersonaBadge } from '@/lib/persona-types'

export function BadgeList({
  badges,
  busyId,
  onHide,
}: {
  badges: PersonaBadge[]
  busyId: string | null
  onHide: (badgeKey: string) => void
}) {
  const visibleBadges = badges.filter((badge) => !badge.hidden)

  if (visibleBadges.length === 0) {
    return <div className="mt-2 text-xs text-muted-foreground">还没有解锁的关系勋章。</div>
  }

  return (
    <div className="mt-3 grid gap-2 sm:grid-cols-2">
      {visibleBadges.map((badge) => (
        <div key={badge.badgeKey} className="rounded-md border border-border/40 bg-muted/20 p-2.5">
          <div className="flex items-start justify-between gap-2">
            <div>
              <div className="text-xs font-medium text-foreground">{badge.label}</div>
              <div className="mt-1 text-xs leading-5 text-muted-foreground">
                {badge.unlockReason}
              </div>
            </div>
            <Button
              aria-label="隐藏勋章"
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-xs text-muted-foreground"
              disabled={busyId === `badge:${badge.badgeKey}`}
              onClick={() => onHide(badge.badgeKey)}
            >
              {busyId === `badge:${badge.badgeKey}` ? (
                <Loader2 className="size-3 animate-spin" />
              ) : (
                <EyeOff className="size-3" />
              )}
            </Button>
          </div>
        </div>
      ))}
    </div>
  )
}
