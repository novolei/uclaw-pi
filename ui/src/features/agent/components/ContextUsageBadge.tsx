/**
 * ContextUsageBadge — 上下文使用量指示器
 *
 * 输入框工具栏上的一个 36×36 圆形按钮：
 * - 内部为 16px 圆环，按 displayTokens / displayWindow 比例渲染
 * - hover / click 弹出 Popover，内含 token 明细 + 手动压缩按钮
 * - 压缩中时按钮位置显示 Loader2 旋转图标
 * - 占用接近压缩阈值（窗口 × 0.775 × 80%）时圆环变琥珀色
 * - 无数据时不显示
 *
 * features/agent migration split (was 419 lines): the usage computation +
 * last-valid-value cache live in `hooks/useContextUsage`; the popover body lives
 * in `ContextUsagePopover`. This shell owns only the compacting spinner, the
 * hover-open state machine, and the ring trigger. No IPC — the badge is driven
 * entirely by props (usage events are wired up by the parent AgentView).
 */

import * as React from 'react'
import { Loader2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Popover, PopoverTrigger } from '@/components/ui/popover'
import { cn } from '@/lib/utils'
import { useContextUsage } from '../hooks/useContextUsage'
import { ContextUsagePopover } from './ContextUsagePopover'

/** Popover hover 关闭延迟（ms），与 AgentThinkingPopover 一致 */
const HOVER_CLOSE_DELAY = 150

interface ContextUsageBadgeProps {
  inputTokens?: number
  outputTokens?: number
  cacheReadTokens?: number
  cacheCreationTokens?: number
  costUsd?: number
  contextWindow?: number
  /** Skill manifest token cost (from agent:context_stats). Shows a "技能" row
   *  in the popover when > 0. */
  skillsTokens?: number
  isCompacting: boolean
  isProcessing: boolean
  onCompact: () => void
}

/** 圆环进度指示器 — 16×16 SVG，描边 2px */
interface UsageRingProps {
  ratio: number
  isWarning: boolean
}
function UsageRing({ ratio, isWarning }: UsageRingProps): React.ReactElement {
  const radius = 8
  const circumference = 2 * Math.PI * radius
  const clamped = Math.max(0, Math.min(1, ratio))
  const dashOffset = circumference * (1 - clamped)

  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 20 20"
      className={cn(
        'shrink-0 transition-colors',
        isWarning ? 'text-amber-500 dark:text-amber-400' : 'text-foreground/70',
      )}
      aria-hidden="true"
    >
      <circle
        cx="10"
        cy="10"
        r={radius}
        fill="none"
        stroke="currentColor"
        strokeOpacity="0.2"
        strokeWidth="2"
      />
      <circle
        cx="10"
        cy="10"
        r={radius}
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeDasharray={circumference}
        strokeDashoffset={dashOffset}
        transform="rotate(-90 10 10)"
        style={{ transition: 'stroke-dashoffset 300ms ease-out' }}
      />
    </svg>
  )
}

export function ContextUsageBadge({
  inputTokens,
  outputTokens,
  cacheReadTokens,
  cacheCreationTokens,
  costUsd,
  contextWindow,
  skillsTokens,
  isCompacting,
  isProcessing,
  onCompact,
}: ContextUsageBadgeProps): React.ReactElement | null {
  // Usage computation + last-valid-value cache (avoids session-switch flicker).
  // Hooks must run before any early-return, so call this unconditionally.
  const metrics = useContextUsage({
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheCreationTokens,
    costUsd,
    contextWindow,
    skillsTokens,
  })

  const [open, setOpen] = React.useState(false)
  const closeTimerRef = React.useRef<number | null>(null)

  const cancelClose = React.useCallback(() => {
    if (closeTimerRef.current != null) {
      window.clearTimeout(closeTimerRef.current)
      closeTimerRef.current = null
    }
  }, [])

  const scheduleClose = React.useCallback(() => {
    cancelClose()
    closeTimerRef.current = window.setTimeout(() => setOpen(false), HOVER_CLOSE_DELAY)
  }, [cancelClose])

  React.useEffect(() => cancelClose, [cancelClose])

  // 压缩中 → 按钮位置显示 spinner
  if (isCompacting) {
    return (
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="size-[36px] rounded-full text-muted-foreground cursor-default"
        disabled
      >
        <Loader2 className="size-4 animate-spin" />
      </Button>
    )
  }

  // 从未有过 usage 数据 → 不显示
  if (!metrics) return null

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className={cn(
            'size-[36px] rounded-full',
            metrics.isWarning ? 'text-amber-600 dark:text-amber-400' : 'text-foreground/60 hover:text-foreground',
          )}
          onMouseEnter={() => {
            cancelClose()
            setOpen(true)
          }}
          onMouseLeave={scheduleClose}
        >
          <UsageRing ratio={metrics.ratio} isWarning={metrics.isWarning} />
        </Button>
      </PopoverTrigger>
      <ContextUsagePopover
        metrics={metrics}
        isProcessing={isProcessing}
        onCompact={onCompact}
        onClose={() => setOpen(false)}
        onMouseEnter={cancelClose}
        onMouseLeave={scheduleClose}
      />
    </Popover>
  )
}
