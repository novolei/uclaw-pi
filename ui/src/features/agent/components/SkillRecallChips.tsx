/**
 * SkillRecallChips — renders chips for each skill_search / load_skill
 * invocation in the current session. Distinct from SkillCitationChips
 * (which renders post-application "applied skill X" pills).
 *
 * Phase 4 (G13): When multiple skills from the same category appear in
 * search results, a ⚠️ conflict indicator is shown with a tooltip
 * listing the conflicting skill pairs.
 *
 * See docs/superpowers/specs/2026-05-12-skill-recall-design.md §6.
 */
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { Search, BookOpen, AlertTriangle } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { skillRecallsMapAtom, type SkillRecall } from '@/atoms/agent-atoms'
import { settingsOpenAtom, settingsTabAtom } from '@/atoms/settings-tab'

interface SkillRecallChipsProps {
  sessionId: string
  className?: string
}

interface ConflictGroup {
  category: string
  skills: string[]
}

/**
 * Detect category conflicts: when a single search result set contains
 * multiple skills with the same category, they may give contradictory
 * advice.
 */
function detectConflicts(recalls: SkillRecall[]): ConflictGroup[] {
  const byCategory = new Map<string, Set<string>>()

  for (const r of recalls) {
    if (r.kind === 'search' && r.results) {
      for (const result of r.results) {
        if (result.category) {
          const existing = byCategory.get(result.category)
          if (existing) {
            existing.add(result.name)
          } else {
            byCategory.set(result.category, new Set([result.name]))
          }
        }
      }
    }
  }

  return [...byCategory.entries()]
    .filter(([, names]) => names.size > 1)
    .map(([cat, names]) => ({ category: cat, skills: [...names] }))
}

/** 最多同时展示的召回芯片数，超出部分折叠为 "+N 更多" */
const MAX_VISIBLE_CHIPS = 8

/** 工具提示中最多展示的搜索结果数 */
const MAX_TOOLTIP_RESULTS = 8

export function SkillRecallChips({ sessionId, className }: SkillRecallChipsProps): React.ReactElement | null {
  // Hooks rule: call ALL hooks unconditionally before any early-return.
  // Bundle 18 — prior to this fix, `recalls.length === 0 → return null`
  // sat between three hooks above and the `useMemo` below. When the
  // session's first skill_search arrived mid-conversation, `recalls`
  // flipped from empty to non-empty and the next render ran 4 hooks
  // instead of 3, tripping React's "Rendered more hooks than during the
  // previous render." invariant. Moving the early-return below the
  // hooks keeps the hook count stable across renders.
  const recallsMap = useAtomValue(skillRecallsMapAtom)
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)

  const recalls = recallsMap.get(sessionId) ?? []
  const conflicts = React.useMemo(() => detectConflicts(recalls), [recalls])

  // Safe to early-return now that all hooks have run.
  if (recalls.length === 0) return null

  const visibleRecalls = recalls.slice(0, MAX_VISIBLE_CHIPS)
  const overflowCount = recalls.length - MAX_VISIBLE_CHIPS
  const overflowRecalls = overflowCount > 0 ? recalls.slice(MAX_VISIBLE_CHIPS) : []

  const handleClick = (): void => {
    setSettingsTab('tools')
    setSettingsOpen(true)
  }

  return (
    <div className={cn('flex flex-wrap gap-1.5 mt-2 pl-[46px]', className)}>
      <TooltipProvider delayDuration={200}>
        {visibleRecalls.map((r) => (
          <ChipFor key={r.toolCallId} recall={r} onClick={handleClick} />
        ))}
        {overflowCount > 0 && (
          <OverflowChip recalls={overflowRecalls} count={overflowCount} />
        )}
        {conflicts.length > 0 && (
          <ConflictIndicator conflicts={conflicts} />
        )}
      </TooltipProvider>
    </div>
  )
}

/** 溢出芯片：当召回数超过 MAX_VISIBLE_CHIPS 时展示 "+N 更多" */
const OverflowChip = React.memo(function OverflowChip({ recalls, count }: { recalls: SkillRecall[]; count: number }): React.ReactElement {
  const tooltipText = React.useMemo(() => {
    return recalls.map((r) => {
      if (r.kind === 'search') {
        return `• 搜索: ${r.query?.trim() || '未知'} (${r.results?.length ?? 0} 命中)`
      }
      return `• 加载: ${r.name?.trim() || '未知技能'}`
    }).join('\n')
  }, [recalls])

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          aria-label={`还有 ${count} 个召回记录`}
          className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] leading-tight bg-muted/40 text-muted-foreground border border-border/30 cursor-default animate-in fade-in duration-200"
        >
          +{count} 更多
        </span>
      </TooltipTrigger>
      <TooltipContent side="bottom" className="max-w-xs whitespace-pre-line text-[11px]">
        {tooltipText}
      </TooltipContent>
    </Tooltip>
  )
})

const ChipFor = React.memo(function ChipFor({ recall, onClick }: { recall: SkillRecall; onClick: () => void }): React.ReactElement {
  const isSearch = recall.kind === 'search'
  const Icon = isSearch ? Search : BookOpen
  const safeQuery = recall.query?.trim() || '未知'
  const safeName = recall.name?.trim() || '未知技能'
  const label = isSearch
    ? `搜索"${safeQuery}" → ${recall.results?.length ?? 0} 命中`
    : `加载"${safeName}"`

  const tooltipText = React.useMemo(() => {
    if (isSearch) {
      if (!recall.results || recall.results.length === 0) return '0 命中'
      const displayResults = recall.results.slice(0, MAX_TOOLTIP_RESULTS)
      const lines = displayResults.map((r) => `• ${r.name || '未命名'} (${r.provenance || '未知来源'})`)
      if (recall.results.length > MAX_TOOLTIP_RESULTS) {
        lines.push(`... 还有 ${recall.results.length - MAX_TOOLTIP_RESULTS} 个结果`)
      }
      return lines.join('\n')
    }
    return recall.reason?.trim() || '无原因说明'
  }, [isSearch, recall.results, recall.reason])

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          onClick={onClick}
          aria-label={label}
          className={cn(
            'inline-flex items-center gap-1 px-2 py-0.5 rounded-full',
            'text-[11px] leading-tight',
            'bg-secondary/15 text-secondary-foreground border border-secondary/30',
            'hover:bg-secondary/25 hover:border-secondary/50',
            'active:scale-95 transition-all duration-150',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1',
            'animate-in fade-in slide-in-from-bottom-1 duration-200',
          )}
        >
          <Icon className="size-3 shrink-0" />
          <span className="truncate max-w-[280px]">{label}</span>
        </button>
      </TooltipTrigger>
      <TooltipContent side="bottom" className="max-w-xs whitespace-pre-line text-[11px]">
        {tooltipText || '—'}
      </TooltipContent>
    </Tooltip>
  )
})

const ConflictIndicator = React.memo(function ConflictIndicator({ conflicts }: { conflicts: ConflictGroup[] }): React.ReactElement {
  const tooltipText = React.useMemo(() => {
    const lines = ['检测到同类技能可能存在建议冲突：']
    for (const c of conflicts) {
      lines.push(`• [${c.category}] ${c.skills.join(' vs ')}`)
    }
    return lines.join('\n')
  }, [conflicts])

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          role="status"
          aria-label={`${conflicts.length} 个技能类别存在冲突`}
          className="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-full text-[10px] text-amber-600 dark:text-amber-400 bg-amber-500/10 border border-amber-500/25 cursor-help animate-in fade-in duration-200"
        >
          <AlertTriangle className="size-3" />
          冲突
        </span>
      </TooltipTrigger>
      <TooltipContent side="bottom" className="max-w-xs whitespace-pre-line text-[11px]">
        {tooltipText}
      </TooltipContent>
    </Tooltip>
  )
})
