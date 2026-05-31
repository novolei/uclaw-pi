/**
 * LearnedProfileTab — surfaces the openhuman-style FacetCache produced by
 * Memory OS Sprint 1's stability_detector pipeline.
 *
 * Thin shell: all state + the five memoryLearning IPC actions live in
 * useLearnedProfile; the rows/groups are split into learned-profile/ presentation
 * components (ClassGroup → FacetRow, EmptyState) + the class taxonomy in
 * lib/facet-class. Split out of the 537-line components/settings/LearnedProfileTab.tsx
 * during the features/settings migration (code-organization ADR 2026-05-31).
 * Behavior preserved verbatim.
 *
 * Layout:
 *   - Header — explanatory subtitle, total active count, "Rebuild now" + "Refresh".
 *   - Class-grouped list (Identity / Style / Tooling / Veto / Goal / Channel).
 *   - Empty state explaining the cache fills as the user chats.
 *
 * The rebuild button triggers `memory_learning_rebuild_now` (a 30-min cadence runs
 * automatically via ProactiveService). Failing to rebuild with
 * `learning_enabled=false` surfaces inline rather than crashing. Sprint 2.2.
 */
import * as React from 'react'
import { Loader2, RefreshCw, UserCircle2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'
import { useLearnedProfile } from '../hooks/useLearnedProfile'
import { CLASS_RENDER_ORDER } from '../lib/facet-class'
import { ClassGroup } from './learned-profile/ClassGroup'
import { EmptyState } from './learned-profile/EmptyState'

export function LearnedProfileTab(): React.ReactElement {
  const {
    facets,
    loading,
    rebuilding,
    error,
    dismissing,
    grouped,
    activeCount,
    provisionalCount,
    fetchFacets,
    handleRebuild,
    handleDismiss,
    handlePromote,
    handleDemote,
  } = useLearnedProfile()

  return (
    <div className="space-y-6" data-testid="learned-profile-tab">
      {/* Header */}
      <section data-settings-section="学到的偏好">
        <div className="flex items-start justify-between gap-3 mb-3">
          <div className="flex items-start gap-2">
            <UserCircle2 className="size-5 text-muted-foreground mt-0.5" />
            <div>
              <h2 className="text-sm font-medium text-foreground">学到的偏好</h2>
              <p className="text-xs text-muted-foreground mt-1 max-w-prose">
                我从对话中学到的关于你的偏好。每 30 分钟根据稳定性自动重建，
                也会写到 <code className="text-[11px] bg-muted/40 px-1 py-0.5 rounded">~/Documents/workground/brain/PROFILE.md</code>。
                不想要的可以「移除」 — 下次出现足够新证据时还会再次浮现。
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2 flex-shrink-0">
            <Button
              size="sm"
              variant="ghost"
              className="text-xs h-7 gap-1"
              onClick={() => void fetchFacets()}
              disabled={loading}
              title="刷新当前缓存（不重建）"
            >
              <RefreshCw className={cn('size-3', loading && 'animate-spin')} />
              刷新
            </Button>
            <Button
              size="sm"
              variant="outline"
              className="text-xs h-7 gap-1"
              onClick={() => void handleRebuild()}
              disabled={rebuilding}
              title="手动触发稳定性重建（默认 30 分钟一次）"
            >
              <RefreshCw className={cn('size-3', rebuilding && 'animate-spin')} />
              立即重建
            </Button>
          </div>
        </div>

        {/* Summary badges */}
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Badge variant="outline" className="text-[10px] px-1.5 py-0">
            {activeCount} active
          </Badge>
          {provisionalCount > 0 && (
            <Badge variant="outline" className="text-[10px] px-1.5 py-0">
              {provisionalCount} provisional
            </Badge>
          )}
          <span className="text-[10px] text-muted-foreground/70">
            共 {facets.length} 条
          </span>
        </div>

        {/* Error banner */}
        {error && (
          <div className="mt-3 px-3 py-2 bg-destructive/10 text-destructive text-xs rounded">
            {error}
          </div>
        )}
      </section>

      {/* Empty state */}
      {!loading && facets.length === 0 && !error && <EmptyState />}

      {/* Loading state */}
      {loading && facets.length === 0 && (
        <div className="flex items-center justify-center py-10">
          <Loader2 className="size-4 animate-spin text-muted-foreground" />
        </div>
      )}

      {/* Class groups */}
      {!loading && facets.length > 0 && (
        <div className="space-y-5">
          {CLASS_RENDER_ORDER.map((cls) => {
            const items = grouped.get(cls) ?? []
            return (
              <ClassGroup
                key={cls}
                className={cls}
                facets={items}
                dismissing={dismissing}
                onDismiss={handleDismiss}
                onPromote={handlePromote}
                onDemote={handleDemote}
              />
            )
          })}
          {/* Forward-compat: any unknown class (future backend changes) */}
          {Array.from(grouped.entries())
            .filter(([k]) => !CLASS_RENDER_ORDER.includes(k))
            .map(([k, items]) => (
              <ClassGroup
                key={k}
                className={k}
                facets={items}
                dismissing={dismissing}
                onDismiss={handleDismiss}
                onPromote={handlePromote}
                onDemote={handleDemote}
              />
            ))}
        </div>
      )}
    </div>
  )
}
