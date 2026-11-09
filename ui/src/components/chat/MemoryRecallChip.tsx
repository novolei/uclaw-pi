/**
 * MemoryRecallChip — 记忆召回事件指示芯片
 *
 * 在消息气泡下方展示 Agent turn 触发的记忆召回摘要，
 * 包含召回总量、技能激活数和层级分布。
 */

import * as React from 'react'
import { Brain, Sparkles } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover'
import type { MemoryRecallEvent } from '@/atoms/agent-atoms'
import { cn } from '@/lib/utils'

interface MemoryRecallChipProps {
  event: MemoryRecallEvent
  /** 内联模式：不渲染外层 wrapper，由父级 flex 容器统一布局 */
  inline?: boolean
  className?: string
}

// Recall layers borrow the memory kind palette (docs/design/memory-ui.md):
// Boot→lesson, Triggered→episode, Relevant→fact, Expanded→entity; Recent stays neutral.
const LAYER_COLORS: Record<string, string> = {
  Boot: 'bg-memory-lesson-bg text-memory-lesson border-memory-lesson/20',
  Triggered: 'bg-memory-episode-bg text-memory-episode border-memory-episode/20',
  Relevant: 'bg-memory-fact-bg text-memory-fact border-memory-fact/20',
  Expanded: 'bg-memory-entity-bg text-memory-entity border-memory-entity/20',
  Recent: 'bg-muted text-muted-foreground border-border',
}

const KIND_LABELS: Record<string, string> = {
  procedure: '技能',
  user_profile: '偏好',
  episode: '事件',
  knowledge: '知识',
  reference: '参考',
  identity: '身份',
  value: '价值观',
  directive: '指令',
  curated: '精选',
  boot: '引导',
}

function inferItemLayer(
  itemIdx: number,
  event: MemoryRecallEvent,
): string | null {
  const { bootCount, triggeredCount, relevantCount, expandedCount } = event
  if (itemIdx < bootCount) return 'Boot'
  if (itemIdx < bootCount + triggeredCount) return 'Triggered'
  if (itemIdx < bootCount + triggeredCount + relevantCount) return 'Relevant'
  if (itemIdx < bootCount + triggeredCount + relevantCount + expandedCount)
    return 'Expanded'
  return 'Recent'
}

export function MemoryRecallChip({
  event,
  inline = false,
  className,
}: MemoryRecallChipProps): React.ReactElement {
  const hasSkills = event.skillsCount > 0
  const layers = [
    { name: 'Boot', count: event.bootCount },
    { name: 'Triggered', count: event.triggeredCount },
    { name: 'Relevant', count: event.relevantCount },
    { name: 'Expanded', count: event.expandedCount },
    { name: 'Recent', count: event.recentCount },
  ].filter((l) => l.count > 0)

  const chip = (
    <Popover>
      <PopoverTrigger asChild>
        <span
          role="status"
          className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] leading-tight bg-memory-entity-bg text-memory-entity border border-memory-entity/20 cursor-pointer hover:border-memory-entity/40 transition-colors animate-in fade-in duration-200"
        >
          <Brain className="size-3 shrink-0" />
          <span>
            已召回 {event.totalCandidates} 条记忆
            {hasSkills && ` · ${event.skillsCount} 技能`}
          </span>
        </span>
      </PopoverTrigger>
      <PopoverContent
        side="top"
        align="start"
        className="w-72 max-h-80 overflow-y-auto p-0"
      >
        <div className="p-3 border-b">
          <div className="flex items-center gap-2 mb-1">
            <Sparkles className="size-4 text-memory-entity" />
            <span className="text-sm font-semibold">记忆召回详情</span>
          </div>
          <p className="text-xs text-muted-foreground">
            共召回 {event.totalCandidates} 条记忆
            {hasSkills && `，含 ${event.skillsCount} 个学得技能`}
          </p>
        </div>

        {layers.length > 0 && (
          <div className="px-3 py-2 border-b">
            <p className="text-[10px] font-medium text-muted-foreground uppercase tracking-wider mb-1.5">
              层级分布
            </p>
            <div className="flex flex-wrap gap-1">
              {layers.map((l) => (
                <Badge
                  key={l.name}
                  variant="outline"
                  className={cn(
                    'text-[10px] px-1.5 py-0',
                    LAYER_COLORS[l.name] ?? 'bg-muted/30',
                  )}
                >
                  {l.name}: {l.count}
                </Badge>
              ))}
            </div>
          </div>
        )}

        <div className="py-1">
          {event.items.slice(0, 12).map((item, idx) => {
            const layer = inferItemLayer(idx, event)
            const kindLabel = KIND_LABELS[item.kind] ?? item.kind
            return (
              <div
                key={item.nodeId || idx}
                className="flex items-start gap-2 px-3 py-1.5 hover:bg-accent/50 transition-colors"
              >
                <div className="flex-1 min-w-0">
                  <p className="text-xs font-medium truncate">
                    {item.title}
                  </p>
                  <div className="flex items-center gap-1.5 mt-0.5">
                    {layer && (
                      <span className="text-[10px] text-muted-foreground/60">
                        {layer}
                      </span>
                    )}
                    <span className="text-[10px] text-muted-foreground/50">
                      ·
                    </span>
                    <span className="text-[10px] text-muted-foreground/60">
                      {kindLabel}
                    </span>
                  </div>
                </div>
              </div>
            )
          })}
        </div>
      </PopoverContent>
    </Popover>
  )

  if (inline) {
    return chip
  }

  return <div className={cn('px-4 pb-2', className)}>{chip}</div>
}
