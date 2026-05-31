/**
 * ContextUsagePopover — the presentational body of the ContextUsageBadge
 * popover. Extracted during the features/agent migration (split of the 419-line
 * ContextUsageBadge) so the badge shell stays ≤300 lines and this view stays
 * purely presentational (props in, JSX out — no state, no IPC).
 *
 * Renders four optional sections from the derived metrics: 输入构成 / 缓存效率 /
 * 上下文窗口 / 本轮成本, plus the manual-compact button. The hover-close wiring
 * (onMouseEnter/onMouseLeave) and open state stay with the badge shell; this
 * view only forwards the mouse handlers onto the PopoverContent.
 */

import * as React from 'react'
import { Minimize2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { PopoverContent } from '@/components/ui/popover'
import { cn } from '@/lib/utils'
import type { ContextUsageMetrics } from '../hooks/useContextUsage'

/** 格式化 token 数为可读字符串（如 1234 → "1.2k"） */
function formatTokens(tokens: number): string {
  if (tokens >= 1_000_000) {
    return `${(tokens / 1_000_000).toFixed(1)}M`
  }
  if (tokens >= 1_000) {
    return `${(tokens / 1_000).toFixed(1)}k`
  }
  return `${tokens}`
}

/** 货币格式：$0.0023 / $0.123 / $1.23 — 自适应小数位 */
function formatCost(usd: number): string {
  if (usd >= 1) return `$${usd.toFixed(2)}`
  if (usd >= 0.01) return `$${usd.toFixed(3)}`
  return `$${usd.toFixed(4)}`
}

/** Popover 里的一行 key/value */
interface DetailRowProps {
  label: string
  // Widened so the "context" row can embed a "1M" badge alongside the
  // numeric value — earlier this was string-only.
  value: React.ReactNode
  emphasized?: boolean
}
function DetailRow({ label, value, emphasized }: DetailRowProps): React.ReactElement {
  return (
    <div className="flex items-center justify-between gap-4 text-xs">
      <span className="text-foreground/70">{label}</span>
      <span className={cn('tabular-nums', emphasized ? 'font-medium text-foreground' : 'text-foreground/90')}>
        {value}
      </span>
    </div>
  )
}

export interface ContextUsagePopoverProps {
  metrics: ContextUsageMetrics
  isProcessing: boolean
  onCompact: () => void
  onClose: () => void
  onMouseEnter: () => void
  onMouseLeave: () => void
}

export function ContextUsagePopover({
  metrics,
  isProcessing,
  onCompact,
  onClose,
  onMouseEnter,
  onMouseLeave,
}: ContextUsagePopoverProps): React.ReactElement {
  const {
    isWarning,
    displayTokens,
    displayWindow,
    pureInput,
    displayCacheCreation,
    displayCacheRead,
    displayOutput,
    displaySkills,
    displayCost,
    totalSent,
    cacheHitRatio,
    cacheSavedInput,
    percent,
  } = metrics

  const handleCompactClick = (): void => {
    if (isProcessing) return
    onCompact()
    onClose()
  }

  return (
    <PopoverContent
      side="top"
      align="center"
      sideOffset={8}
      className="w-auto min-w-[260px] p-2.5"
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      onOpenAutoFocus={(e) => e.preventDefault()}
    >
      <div className="flex flex-col gap-2">
        {/* 输入构成 */}
        <div className="flex flex-col gap-1">
          <div className="text-[10px] uppercase tracking-widest text-muted-foreground/70 font-semibold">
            输入构成
          </div>
          {pureInput > 0 && <DetailRow label="新输入" value={pureInput.toLocaleString()} />}
          {displayCacheCreation ? (
            <DetailRow label="缓存写入" value={displayCacheCreation.toLocaleString()} />
          ) : null}
          {displayCacheRead ? (
            <DetailRow
              label="缓存读取"
              value={
                <span className="inline-flex items-center gap-1.5">
                  <span>{displayCacheRead.toLocaleString()}</span>
                  {cacheHitRatio > 0 && (
                    <span
                      className="rounded-sm bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 px-1 py-0 text-[9.5px] font-semibold tabular-nums"
                      title={`本轮 ${cacheHitRatio}% 输入命中缓存，约省 ${cacheSavedInput.toLocaleString()} 输入 token`}
                    >
                      {cacheHitRatio}% 命中
                    </span>
                  )}
                </span>
              }
            />
          ) : null}
          {displayOutput ? <DetailRow label="输出" value={displayOutput.toLocaleString()} /> : null}
          {displaySkills != null && displaySkills > 0 ? (
            <DetailRow label="技能 manifest" value={displaySkills.toLocaleString()} />
          ) : null}
        </div>

        {/* 缓存效率 — 始终显示，冷启动为 0% 方便监控 */}
        <>
          <div className="h-px bg-border" />
          <div className="flex flex-col gap-1">
            <div className="text-[10px] uppercase tracking-widest text-muted-foreground/70 font-semibold">
              缓存效率
            </div>
            <DetailRow
              label="命中 / 总发送"
              value={
                <span className="inline-flex items-center gap-1.5">
                  <span className="tabular-nums">
                    {(displayCacheRead ?? 0).toLocaleString()} / {totalSent.toLocaleString()}
                  </span>
                  <span
                    className={cn(
                      'rounded-sm px-1 py-0 text-[9.5px] font-semibold tabular-nums',
                      cacheHitRatio >= 50
                        ? 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400'
                        : cacheHitRatio > 0
                        ? 'bg-amber-500/10 text-amber-600 dark:text-amber-400'
                        : 'bg-muted text-muted-foreground/60',
                    )}
                  >
                    {cacheHitRatio}%
                  </span>
                </span>
              }
              emphasized
            />
            {(displayCacheCreation ?? 0) > 0 && (
              <DetailRow
                label="缓存写入"
                value={
                  <span className="text-muted-foreground/80 tabular-nums">
                    {(displayCacheCreation ?? 0).toLocaleString()}
                  </span>
                }
              />
            )}
            {cacheSavedInput > 0 && (
              <DetailRow
                label="节省约"
                value={
                  <span className="text-emerald-600 dark:text-emerald-400 tabular-nums">
                    ~{cacheSavedInput.toLocaleString()} token
                  </span>
                }
              />
            )}
          </div>
        </>

        {/* 上下文窗口 */}
        {displayWindow ? (
          <>
            <div className="h-px bg-border" />
            <div className="flex flex-col gap-1">
              <div className="text-[10px] uppercase tracking-widest text-muted-foreground/70 font-semibold">
                上下文窗口
              </div>
              <DetailRow
                label="已用 / 总量"
                value={
                  <span className="inline-flex items-center gap-1.5">
                    {formatTokens(displayTokens)} / {formatTokens(displayWindow)}
                    {displayWindow >= 1_000_000 && (
                      <span
                        className="rounded-sm bg-primary/15 text-primary px-1 py-0 text-[9.5px] font-semibold uppercase tracking-wider"
                        title="1M context window beta enabled for this model"
                      >
                        1M
                      </span>
                    )}
                  </span>
                }
                emphasized
              />
              {percent != null && (
                <DetailRow
                  label="占用"
                  value={`${percent}%`}
                  emphasized={isWarning}
                />
              )}
            </div>
          </>
        ) : null}

        {/* 费用 */}
        {displayCost != null && displayCost > 0 ? (
          <>
            <div className="h-px bg-border" />
            <div className="flex flex-col gap-1">
              <div className="text-[10px] uppercase tracking-widest text-muted-foreground/70 font-semibold">
                本轮成本
              </div>
              <DetailRow label="USD" value={formatCost(displayCost)} emphasized />
            </div>
          </>
        ) : null}

        <div className="h-px bg-border my-0.5" />
        <Button
          type="button"
          variant={isWarning ? 'default' : 'outline'}
          size="sm"
          className={cn(
            'h-7 text-xs gap-1.5',
            isWarning && 'bg-amber-500 hover:bg-amber-600 text-white',
          )}
          onClick={handleCompactClick}
          disabled={isProcessing}
        >
          <Minimize2 className="size-3.5" />
          {isProcessing ? '对话进行中' : '手动压缩'}
        </Button>
      </div>
    </PopoverContent>
  )
}
