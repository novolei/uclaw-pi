/**
 * MemoryRecallChip — 本轮参考的记忆
 *
 * 在 Agent turn 下方展示一枚中性芯片「参考了 N 条记忆」，汇总两路来源：
 * MemoryCore 的最相关命中（coreHits）与记忆图谱的分层召回（items / 层级计数）。
 * 点击（或 Enter / Space）打开 Popover：「最相关」列表 + 层级分布；
 * 关键词检索模式下附一行提示，可直达 设置 › 记忆。
 * 视觉语言见 docs/design/memory-ui.md。
 */

import * as React from 'react'
import { useSetAtom } from 'jotai'
import { Brain } from 'lucide-react'
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover'
import { MEMORY_KIND_CLASSES, ProgressBar } from '@/components/memory/primitives'
import type { MemoryCoreHit, MemoryRecallEvent } from '@/atoms/agent-atoms'
import { settingsOpenAtom, settingsTabAtom } from '@/atoms/settings-tab'
import { cn } from '@/lib/utils'

interface MemoryRecallChipProps {
  event: MemoryRecallEvent
  /** 内联模式：不渲染外层 wrapper，由父级 flex 容器统一布局 */
  inline?: boolean
  className?: string
}

type LayerName = 'Boot' | 'Triggered' | 'Relevant' | 'Expanded' | 'Recent'

const LAYER_LABELS: Record<LayerName, string> = {
  Boot: '常驻',
  Triggered: '触发',
  Relevant: '相关',
  Expanded: '扩展',
  Recent: '近期',
}

// Recall layers borrow the memory kind palette (docs/design/memory-ui.md):
// Boot→lesson, Triggered→episode, Relevant→fact, Expanded→entity; Recent stays neutral.
const LAYER_DOT: Record<LayerName, string> = {
  Boot: MEMORY_KIND_CLASSES.lesson.dot,
  Triggered: MEMORY_KIND_CLASSES.episode.dot,
  Relevant: MEMORY_KIND_CLASSES.fact.dot,
  Expanded: MEMORY_KIND_CLASSES.entity.dot,
  Recent: 'bg-muted-foreground/40',
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

const SOURCE_LABELS: Record<string, string> = {
  graph: '记忆',
  item: '记忆',
  chunk: '对话片段',
}

function coreHitsOf(event: MemoryRecallEvent): MemoryCoreHit[] {
  return Array.isArray(event.coreHits) ? event.coreHits : []
}

/**
 * Memories referenced this turn: every core hit plus every graph candidate,
 * minus graph items that a core `graph` hit already covers (matched by title —
 * the core index has no node id to match on).
 */
export function countRecalledMemories(event: MemoryRecallEvent): number {
  const hits = coreHitsOf(event)
  const coreGraphTitles = new Set(
    hits.filter((h) => h.source === 'graph').map((h) => h.title.trim()),
  )
  const duplicates = event.items.filter((i) => coreGraphTitles.has(i.title.trim())).length
  return hits.length + Math.max(0, event.totalCandidates - duplicates)
}

export function hasRecalledMemories(event: MemoryRecallEvent | null | undefined): boolean {
  return event != null && countRecalledMemories(event) > 0
}

function inferItemLayer(itemIdx: number, event: MemoryRecallEvent): LayerName {
  const { bootCount, triggeredCount, relevantCount, expandedCount } = event
  if (itemIdx < bootCount) return 'Boot'
  if (itemIdx < bootCount + triggeredCount) return 'Triggered'
  if (itemIdx < bootCount + triggeredCount + relevantCount) return 'Relevant'
  if (itemIdx < bootCount + triggeredCount + relevantCount + expandedCount) return 'Expanded'
  return 'Recent'
}

function SectionLabel({ id, children }: { id?: string; children: React.ReactNode }) {
  return (
    <p id={id} className="mb-2 text-[11px] font-medium leading-4 text-muted-foreground">
      {children}
    </p>
  )
}

function CoreHitRow({ hit }: { hit: MemoryCoreHit }) {
  // Chunk hits carry no title; the backend falls back to the snippet's first
  // ~24 chars — don't print the same words twice.
  const titleIsSnippetPrefix = hit.snippet.startsWith(hit.title)
  const showTitle = hit.title.trim() !== '' && !(hit.source === 'chunk' && titleIsSnippetPrefix)
  const isRerank = hit.scoreKind === 'rerank'

  return (
    <li className="py-2 first:pt-0 last:pb-0">
      {showTitle && (
        <p className="truncate text-[13px] font-medium leading-5 text-foreground">{hit.title}</p>
      )}
      <p
        className={cn(
          'line-clamp-2 break-words',
          showTitle
            ? 'text-[12px] leading-[18px] text-muted-foreground'
            : 'text-[13px] leading-5 text-foreground',
        )}
      >
        {hit.snippet}
      </p>
      <div className="mt-1 flex items-center gap-2">
        <span className="text-[11px] leading-4 text-muted-foreground">
          {SOURCE_LABELS[hit.source] ?? '记忆'}
        </span>
        {isRerank && (
          <ProgressBar
            value={hit.score}
            label="相关度"
            className="w-12"
            title={`相关度 ${Math.round(Math.min(1, Math.max(0, hit.score)) * 100)}%`}
          />
        )}
      </div>
    </li>
  )
}

export function MemoryRecallChip({
  event,
  inline = false,
  className,
}: MemoryRecallChipProps): React.ReactElement | null {
  const [open, setOpen] = React.useState(false)
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)
  const coreListId = React.useId()
  const contentRef = React.useRef<HTMLDivElement>(null)

  const total = countRecalledMemories(event)
  if (total === 0) return null

  const coreHits = coreHitsOf(event)
  const hasSkills = event.skillsCount > 0
  const layers = (
    [
      { name: 'Boot', count: event.bootCount },
      { name: 'Triggered', count: event.triggeredCount },
      { name: 'Relevant', count: event.relevantCount },
      { name: 'Expanded', count: event.expandedCount },
      { name: 'Recent', count: event.recentCount },
    ] as Array<{ name: LayerName; count: number }>
  ).filter((l) => l.count > 0)
  const graphItems = event.items.slice(0, 12)
  const hasGraph = layers.length > 0 || graphItems.length > 0

  const openMemorySettings = () => {
    setOpen(false)
    setSettingsTab('memoryRecall')
    setSettingsOpen(true)
  }

  const chip = (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className={cn(
            'inline-flex items-center gap-1.5 rounded-full border border-border bg-muted px-2 py-0.5',
            'text-[11px] leading-4 text-muted-foreground',
            'transition-colors duration-[120ms] hover:border-foreground/20 hover:text-foreground',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background',
            'data-[state=open]:border-foreground/20 data-[state=open]:text-foreground',
            'animate-in fade-in duration-[180ms] motion-reduce:animate-none',
          )}
        >
          <Brain className="size-3 shrink-0" aria-hidden />
          {layers.length > 0 && (
            <span className="flex items-center -space-x-0.5" aria-hidden>
              {layers.slice(0, 4).map((l) => (
                <span
                  key={l.name}
                  className={cn('size-1.5 rounded-full ring-1 ring-muted', LAYER_DOT[l.name])}
                />
              ))}
            </span>
          )}
          <span className="tabular-nums">
            参考了 {total} 条记忆
            {hasSkills && ` · ${event.skillsCount} 技能`}
          </span>
        </button>
      </PopoverTrigger>
      <PopoverContent
        side="top"
        align="start"
        className="w-80 max-h-[min(28rem,var(--radix-popover-content-available-height))] overflow-y-auto scrollbar-thin rounded-lg p-0 shadow-elevation-2 duration-[180ms]"
        // Focus the panel, not the settings link, so opening doesn't light a
        // focus ring on it; Tab still reaches the link (Radix traps focus).
        onOpenAutoFocus={(e) => {
          e.preventDefault()
          contentRef.current?.focus()
        }}
        ref={contentRef}
        tabIndex={-1}
      >
        <div className="px-4 pb-3 pt-3">
          <p className="text-[13px] font-medium leading-5 text-foreground">本轮参考的记忆</p>
          <p className="text-[11px] leading-4 text-muted-foreground tabular-nums">
            共 {total} 条{hasSkills && `，含 ${event.skillsCount} 个学得技能`}
          </p>
        </div>

        {coreHits.length > 0 && (
          <div className="border-t border-border/50 px-4 py-3">
            <SectionLabel id={coreListId}>最相关</SectionLabel>
            <ul aria-labelledby={coreListId} className="divide-y divide-border/50">
              {coreHits.map((hit, idx) => (
                <CoreHitRow key={`${hit.source}-${idx}-${hit.title}`} hit={hit} />
              ))}
            </ul>
          </div>
        )}

        {hasGraph && (
          <div className="border-t border-border/50 px-4 py-3">
            <SectionLabel>层级分布</SectionLabel>
            {layers.length > 0 && (
              <div className="flex flex-wrap gap-x-3 gap-y-1">
                {layers.map((l) => (
                  <span
                    key={l.name}
                    className="inline-flex items-center gap-1 text-[11px] leading-4 text-muted-foreground"
                  >
                    <span aria-hidden className={cn('size-1.5 rounded-full', LAYER_DOT[l.name])} />
                    {LAYER_LABELS[l.name]}
                    <span className="tabular-nums text-foreground">{l.count}</span>
                  </span>
                ))}
              </div>
            )}
            {graphItems.length > 0 && (
              <ul className="mt-2 space-y-1.5">
                {graphItems.map((item, idx) => {
                  const layer = inferItemLayer(idx, event)
                  return (
                    <li key={item.nodeId || idx} className="min-w-0">
                      <p className="truncate text-[12px] leading-[18px] text-foreground">{item.title}</p>
                      <p className="text-[11px] leading-4 text-muted-foreground">
                        {LAYER_LABELS[layer]} · {KIND_LABELS[item.kind] ?? item.kind}
                      </p>
                    </li>
                  )
                })}
              </ul>
            )}
          </div>
        )}

        {event.mode === 'keyword' && (
          <p className="sticky bottom-0 border-t border-border/50 bg-popover px-4 py-2.5 text-[11px] leading-4 text-muted-foreground">
            当前为关键词检索，
            <button
              type="button"
              onClick={openMemorySettings}
              className="rounded-sm text-foreground underline decoration-muted-foreground/50 underline-offset-2 transition-colors duration-[120ms] hover:decoration-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              开启本地检索模型
            </button>
            可按意思找回
          </p>
        )}
      </PopoverContent>
    </Popover>
  )

  if (inline) {
    return chip
  }

  return <div className={cn('px-4 pb-2', className)}>{chip}</div>
}
