/**
 * AgentGrowthTab — "成长" tab for the Memory module.
 *
 * Three read-only sections:
 *   1. 用户模型 — current model card + collapsible 演化历史 timeline
 *   2. 反思     — reflection cards with confidence badges
 *   3. 遐想     — live ring-buffer events prepended to persisted daydreams,
 *                 de-duplicated by content
 *
 * Mirrors LearnedProfileTab's shell: header + refresh button + count badges,
 * Loader2 spinner, error banner, empty states.
 */
import * as React from 'react'
import { Loader2, RefreshCw, Brain } from 'lucide-react'
import { useAtomValue } from 'jotai'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { cn } from '@/lib/utils'
import { safeParseDate } from '@/lib/utils'
import { daydreamEventsAtom } from '@/atoms/agent-atoms'
import { useAgentMemory } from './useAgentMemory'

// ─── Relative time helper (mirrors FragmentCard pattern) ───────────────────

function relativeTime(value: string): string {
  const date = safeParseDate(value)
  if (!date) return '—'
  const diff = Date.now() - date.getTime()
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}分钟前`
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}小时前`
  if (diff < 604_800_000) return `${Math.floor(diff / 86_400_000)}天前`
  return date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' })
}

// ─── Small inline card (read-only, no onClick needed) ─────────────────────

function InfoCard({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <div
      className={cn(
        'rounded-lg border border-border/60 bg-card px-3.5 py-3 text-sm',
        className,
      )}
    >
      {children}
    </div>
  )
}

// ─── Component ────────────────────────────────────────────────────────────

export function AgentGrowthTab(): React.ReactElement {
  const { loading, error, reflections, userModel, daydreams, history, refresh } = useAgentMemory()
  const liveEvents = useAtomValue(daydreamEventsAtom)

  // Merge live ring-buffer events + persisted daydreams, de-dup by content
  const mergedDaydreams = React.useMemo(() => {
    const seen = new Set<string>()
    const result: { content: string; createdAt: string }[] = []
    for (const ev of liveEvents) {
      if (!seen.has(ev.content)) {
        seen.add(ev.content)
        result.push(ev)
      }
    }
    for (const dd of daydreams) {
      if (!seen.has(dd.content)) {
        seen.add(dd.content)
        result.push(dd)
      }
    }
    return result
  }, [liveEvents, daydreams])

  return (
    <div className="space-y-6 overflow-y-auto h-full" data-testid="agent-growth-tab">
      {/* ── Header ─────────────────────────────────────────────────────── */}
      <section data-settings-section="成长">
        <div className="flex items-start justify-between gap-3 mb-3">
          <div className="flex items-start gap-2">
            <Brain className="size-5 text-muted-foreground mt-0.5" />
            <div>
              <h2 className="text-sm font-medium text-foreground">成长</h2>
              <p className="text-xs text-muted-foreground mt-1 max-w-prose">
                Agent 对你的理解、自我反思与遐想 — 只读快照，实时更新。
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2 flex-shrink-0">
            <Button
              size="sm"
              variant="ghost"
              className="text-xs h-7 gap-1"
              onClick={() => void refresh()}
              disabled={loading}
              title="刷新数据"
            >
              <RefreshCw className={cn('size-3', loading && 'animate-spin')} />
              刷新
            </Button>
          </div>
        </div>

        {/* Count badges */}
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Badge variant="outline" className="text-[10px] px-1.5 py-0">
            {reflections.length} 反思
          </Badge>
          <Badge variant="outline" className="text-[10px] px-1.5 py-0">
            {mergedDaydreams.length} 遐想
          </Badge>
        </div>

        {/* Error banner */}
        {error && (
          <div className="mt-3 px-3 py-2 bg-destructive/10 text-destructive text-xs rounded">
            {error}
          </div>
        )}
      </section>

      {/* ── Loading spinner ─────────────────────────────────────────────── */}
      {loading && (
        <div className="flex items-center justify-center py-10">
          <Loader2 className="size-4 animate-spin text-muted-foreground" />
        </div>
      )}

      {!loading && (
        <div className="space-y-8">
          {/* ── Section 1: 用户模型 ───────────────────────────────────────── */}
          <section>
            <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-3">
              用户模型
            </h3>
            {userModel ? (
              <div className="space-y-3">
                <InfoCard>
                  <p className="text-sm leading-relaxed">{userModel.summary}</p>
                  <p className="text-[11px] text-muted-foreground mt-2">
                    更新于 {relativeTime(userModel.updatedAt)}
                  </p>
                </InfoCard>

                {/* 演化历史 */}
                {history.length > 0 && (
                  <details className="group">
                    <summary className="cursor-pointer text-xs text-muted-foreground hover:text-foreground transition-colors select-none list-none flex items-center gap-1">
                      <span className="group-open:rotate-90 transition-transform inline-block">▶</span>
                      演化历史（{history.length} 版本）
                    </summary>
                    <ol className="mt-2 space-y-2 border-l border-border/50 pl-4">
                      {history.map((h, i) => (
                        <li key={i} className="relative">
                          <div className="absolute -left-[17px] top-1.5 size-2 rounded-full bg-border" />
                          <p className="text-xs text-muted-foreground leading-relaxed line-clamp-3">
                            {h.summary}
                          </p>
                          <p className="text-[10px] text-muted-foreground/60 mt-0.5">
                            {relativeTime(h.replacedAt)}
                          </p>
                        </li>
                      ))}
                    </ol>
                  </details>
                )}
              </div>
            ) : (
              <p className="text-xs text-muted-foreground italic">
                还没有形成用户模型 —— 多聊几轮就有了
              </p>
            )}
          </section>

          {/* ── Section 2: 反思 ──────────────────────────────────────────── */}
          <section>
            <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-3">
              反思
            </h3>
            {reflections.length > 0 ? (
              <div className="space-y-2">
                {reflections.map((r, i) => (
                  <InfoCard key={i}>
                    <div className="flex items-start justify-between gap-2">
                      <p className="text-sm leading-relaxed flex-1">{r.insight}</p>
                      <Badge
                        variant="outline"
                        className="text-[10px] px-1.5 py-0 shrink-0 tabular-nums"
                      >
                        {Math.round(r.confidence * 100)}%
                      </Badge>
                    </div>
                    <p className="text-[11px] text-muted-foreground mt-2">
                      {relativeTime(r.createdAt)}
                    </p>
                  </InfoCard>
                ))}
              </div>
            ) : (
              <p className="text-xs text-muted-foreground italic">暂无反思</p>
            )}
          </section>

          {/* ── Section 3: 遐想 ──────────────────────────────────────────── */}
          <section>
            <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wide mb-3">
              遐想
            </h3>
            {mergedDaydreams.length > 0 ? (
              <div className="space-y-2">
                {mergedDaydreams.map((dd, i) => (
                  <InfoCard key={i}>
                    <p className="text-sm leading-relaxed">{dd.content}</p>
                    <p className="text-[11px] text-muted-foreground mt-2">
                      {relativeTime(dd.createdAt)}
                    </p>
                  </InfoCard>
                ))}
              </div>
            ) : (
              <p className="text-xs text-muted-foreground italic">agent 还没有遐想</p>
            )}
          </section>
        </div>
      )}
    </div>
  )
}
