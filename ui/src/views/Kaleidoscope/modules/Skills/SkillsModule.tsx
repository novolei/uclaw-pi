/**
 * SkillsModule — 万花筒「技能」模块。
 *
 * 左可折叠分组列表(SkillsList)+ 右详情(SkillDetail)双栏。数据来自两个
 * 来源 —— 学得技能(listLearnedSkills)+ 内置技能(listSkills)—— 在此 merge
 * 成 UnifiedSkill 列表。维护操作(回填关键词 / 整合技能 / 重新加载)与删除确认
 * 也由本容器持有。整体迁自原 components/settings/SkillsSettings.tsx。
 */
import * as React from 'react'
import { toast } from 'sonner'
import { Search, Plus, List, Columns, ArrowLeft, X, Loader2 } from 'lucide-react'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import {
  listLearnedSkills,
  toggleLearnedSkill,
  deleteLearnedSkill,
  proposeSkillConsolidation,
  cancelSkillConsolidation,
  backfillSkillKeywords,
  listSkills,
  toggleSkill,
  forkSkillToUser,
  reloadSkills,
  type SkillConsolidationProposal,
} from '@/lib/tauri-bridge'
import type { LearnedSkill, SkillInfo } from '@/lib/types'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { ModuleHeader } from '../../shared/ModuleHeader'
import { SkillsList } from './SkillsList'
import { SkillDetail } from './SkillDetail'
import { CreateSkillDialog } from './CreateSkillDialog'
import { SkillConsolidationDialog } from '@/features/settings'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'

export type FilterTab = 'all' | 'learned' | 'builtin' | 'user' | 'promoted' | 'draft' | 'deprecated'

export type UnifiedSkill =
  | { kind: 'learned'; id: string; name: string; enabled: boolean; raw: LearnedSkill }
  | { kind: 'builtin'; id: string; name: string; enabled: boolean; raw: SkillInfo }

// P2: Consolidation progress from backend events
export interface ConsolidationProgress {
  stage: 'loading' | 'preclustering' | 'analyzing' | 'retrying' | 'validating' | 'done' | 'cancelled'
  current: number
  total: number
  detail: string
}

export function SkillsModule(): React.ReactElement {
  const [learnedRaw, setLearnedRaw] = React.useState<LearnedSkill[]>([])
  const [builtinRaw, setBuiltinRaw] = React.useState<SkillInfo[]>([])
  const [loading, setLoading] = React.useState(true)
  const [selectedId, setSelectedId] = React.useState<string | null>(null)
  const [query, setQuery] = React.useState('')
  const [pendingDelete, setPendingDelete] = React.useState<UnifiedSkill | null>(null)
  const [forkingName, setForkingName] = React.useState<string | null>(null)
  const [proposing, setProposing] = React.useState(false)
  const [backfilling, setBackfilling] = React.useState(false)
  const [activeFilter, setActiveFilter] = React.useState<FilterTab>('all')
  const [proposal, setProposal] = React.useState<SkillConsolidationProposal | null>(null)
  const [consolidationOpen, setConsolidationOpen] = React.useState(false)
  const [createDialogOpen, setCreateDialogOpen] = React.useState(false)

  // P2-1: Consolidation progress state
  const [consolidationProgress, setConsolidationProgress] = React.useState<ConsolidationProgress | null>(null)

  // ── 布局状态 ──
  const [isMobile, setIsMobile] = React.useState(false)
  const [compactMode, setCompactMode] = React.useState(false)
  const [mobileShowDetail, setMobileShowDetail] = React.useState(false)
  const containerRef = React.useRef<HTMLDivElement>(null)
  const [containerWidth, setContainerWidth] = React.useState(0)

  const refetch = React.useCallback(async () => {
    setLoading(true)
    const [l, b] = await Promise.allSettled([listLearnedSkills(), listSkills()])
    if (l.status === 'fulfilled') setLearnedRaw(l.value)
    else toast.error('加载学得技能失败', { description: String(l.reason) })
    if (b.status === 'fulfilled') setBuiltinRaw(b.value)
    else toast.error('加载内置技能失败', { description: String(b.reason) })
    setLoading(false)
  }, [])

  React.useEffect(() => {
    void refetch()
  }, [refetch])

  // P2-1: Listen for consolidation progress events from backend
  React.useEffect(() => {
    let unlisten: UnlistenFn | undefined
    const setup = async () => {
      unlisten = await listen<ConsolidationProgress>('skill-consolidation:progress', (event) => {
        setConsolidationProgress(event.payload)
      })
    }
    void setup()
    return () => {
      if (unlisten) unlisten()
    }
  }, [])

  // ── 响应式断点检测 ──
  React.useEffect(() => {
    const mq = window.matchMedia('(max-width: 767px)')
    const update = () => setIsMobile(mq.matches)
    update()
    // jsdom 可能没有 addEventListener，使用可选链
    mq.addEventListener?.('change', update)
    return () => {
      mq.removeEventListener?.('change', update)
    }
  }, [])

  // ── 容器宽度检测（自动紧凑模式 + 中屏适配）──
  React.useEffect(() => {
    const el = containerRef.current
    if (!el) return
    let rafId = 0
    const ro = new ResizeObserver((entries) => {
      cancelAnimationFrame(rafId)
      rafId = requestAnimationFrame(() => {
        const width = entries[0]?.contentRect.width ?? 0
        setContainerWidth(width)
      })
    })
    ro.observe(el)
    return () => {
      cancelAnimationFrame(rafId)
      ro.disconnect()
    }
  }, [])

  // 容器宽度 < 600px 时自动启用紧凑模式；中屏 (600-900px) 也默认紧凑
  const effectiveCompact = React.useMemo(() => {
    if (containerWidth > 0 && containerWidth < 900) return true
    return compactMode
  }, [containerWidth, compactMode])


  const learned: UnifiedSkill[] = React.useMemo(
    () => learnedRaw.map((s) => ({ kind: 'learned', id: s.id, name: s.name, enabled: s.enabled, raw: s })),
    [learnedRaw],
  )
  const builtin: UnifiedSkill[] = React.useMemo(
    () => builtinRaw.map((s) => ({ kind: 'builtin', id: s.name, name: s.name, enabled: s.enabled, raw: s })),
    [builtinRaw],
  )

  // Debounced search query for filtering (300ms)
  const [debouncedQuery, setDebouncedQuery] = React.useState('')
  React.useEffect(() => {
    const timer = setTimeout(() => setDebouncedQuery(query), 300)
    return () => clearTimeout(timer)
  }, [query])

  const filterFn = React.useCallback(
    (s: UnifiedSkill) => {
      const q = debouncedQuery.trim().toLowerCase()
      if (!q) return true
      if (s.name.toLowerCase().includes(q)) return true
      if (s.kind === 'learned') {
        if (s.raw.context?.toLowerCase().includes(q)) return true
        if (s.raw.category?.toLowerCase().includes(q)) return true
      } else {
        if (s.raw.category?.toLowerCase().includes(q)) return true
        if (s.raw.description?.toLowerCase().includes(q)) return true
      }
      return false
    },
    [debouncedQuery],
  )
  const learnedFiltered = learned.filter(filterFn)
  const builtinFiltered = builtin.filter(filterFn)

  // Split builtin skills by provenance
  const userSkills = builtinFiltered.filter(
    (s) => s.kind === 'builtin' && s.raw.provenance === 'user',
  )
  const bundledSkills = builtinFiltered.filter(
    (s) => s.kind === 'builtin' && s.raw.provenance !== 'user',
  )

  // Lifecycle sub-filters for learned skills
  const learnedPromoted = learnedFiltered.filter(
    (s) => s.kind === 'learned' && (s.raw.lifecycle || 'promoted') === 'promoted',
  )
  const learnedDraft = learnedFiltered.filter(
    (s) => s.kind === 'learned' && s.raw.lifecycle === 'draft',
  )
  const learnedDeprecated = learnedFiltered.filter(
    (s) => s.kind === 'learned' && s.raw.lifecycle === 'deprecated',
  )

  // Filter tabs with dynamic counts
  const filterTabs: { value: FilterTab; label: string; count: number }[] = [
    { value: 'all', label: '全部', count: learnedFiltered.length + builtinFiltered.length },
    { value: 'learned', label: '自学技能', count: learnedFiltered.length },
    { value: 'builtin', label: '内建技能', count: bundledSkills.length },
    { value: 'user', label: '自定义', count: userSkills.length },
    { value: 'promoted', label: '已晋升', count: learnedPromoted.length },
    { value: 'draft', label: '草稿', count: learnedDraft.length },
    { value: 'deprecated', label: '已弃用', count: learnedDeprecated.length },
  ]

  // Apply active filter
  const displayLearned = React.useMemo(() => {
    switch (activeFilter) {
      case 'all': case 'learned': return learnedFiltered
      case 'promoted': return learnedPromoted
      case 'draft': return learnedDraft
      case 'deprecated': return learnedDeprecated
      default: return []
    }
  }, [activeFilter, learnedFiltered, learnedPromoted, learnedDraft, learnedDeprecated])

  const displayUserSkills = React.useMemo(() => {
    return activeFilter === 'all' || activeFilter === 'user' ? userSkills : []
  }, [activeFilter, userSkills])

  const displayBundledSkills = React.useMemo(() => {
    return activeFilter === 'all' || activeFilter === 'builtin' ? bundledSkills : []
  }, [activeFilter, bundledSkills])

  const selected =
    [...learned, ...builtin].find((s) => s.id === selectedId) ?? null

  const onToggleEnabled = async (skill: UnifiedSkill, next: boolean) => {
    if (skill.kind === 'learned') {
      setLearnedRaw((prev) => prev.map((s) => (s.id === skill.id ? { ...s, enabled: next } : s)))
      try {
        await toggleLearnedSkill(skill.id, next)
      } catch (err) {
        toast.error('切换状态失败', { description: String(err) })
        setLearnedRaw((prev) => prev.map((s) => (s.id === skill.id ? { ...s, enabled: !next } : s)))
      }
    } else {
      setBuiltinRaw((prev) => prev.map((s) => (s.name === skill.id ? { ...s, enabled: next } : s)))
      try {
        await toggleSkill({ name: skill.name, enabled: next })
      } catch (err) {
        toast.error('切换状态失败', { description: String(err) })
        setBuiltinRaw((prev) => prev.map((s) => (s.name === skill.id ? { ...s, enabled: !next } : s)))
      }
    }
  }

  const onConfirmDelete = async () => {
    if (!pendingDelete || pendingDelete.kind !== 'learned') return
    const target = pendingDelete
    setPendingDelete(null)
    setLearnedRaw((prev) => prev.filter((s) => s.id !== target.id))
    if (selectedId === target.id) setSelectedId(null)
    try {
      await deleteLearnedSkill(target.id)
      toast.success(`已删除「${target.name}」`)
    } catch (err) {
      toast.error('删除失败', { description: String(err) })
      void refetch()
    }
  }

  const onFork = async (name: string) => {
    setForkingName(name)
    try {
      const destPath = await forkSkillToUser(name)
      toast.success(`已 Fork 到 ${destPath}`, {
        description: '现在可以在 ~/.uclaw/skills/ 下编辑这份 skill。',
      })
      const fresh = await reloadSkills()
      setBuiltinRaw(fresh)
    } catch (err) {
      toast.error('Fork 失败', { description: String(err) })
    } finally {
      setForkingName(null)
    }
  }

  const onReload = async () => {
    setLoading(true)
    try {
      const fresh = await reloadSkills()
      setBuiltinRaw(fresh)
    } catch (err) {
      toast.error('重新加载失败', { description: String(err) })
    } finally {
      setLoading(false)
    }
  }

  const onPropose = async () => {
    setProposing(true)
    setConsolidationProgress(null)
    try {
      const result = await proposeSkillConsolidation()
      setConsolidationProgress(null)
      if (!result.clusters || result.clusters.length === 0) {
        toast.info('暂无可合并的重复技能')
        return
      }
      setProposal(result)
      setConsolidationOpen(true)
    } catch (err) {
      // Don't show toast for user-initiated cancellation
      if (String(err).includes('取消')) {
        toast.info('已取消整合')
      } else {
        toast.error('无法分析技能整合方案', { description: String(err) })
      }
    } finally {
      setProposing(false)
      setConsolidationProgress(null)
    }
  }

  // P2-2: Cancel in-flight consolidation
  const onCancelConsolidation = async () => {
    try {
      await cancelSkillConsolidation()
    } catch {
      // Best-effort cancel
    }
  }

  const onBackfill = async () => {
    setBackfilling(true)
    try {
      const result = await backfillSkillKeywords()
      if (result.backfilledSkills === 0) {
        toast.info('关键词索引已完整', {
          description: `${result.totalLearnedSkills} 条技能全部已索引`,
        })
      } else {
        toast.success('关键词回填完成', {
          description: `${result.backfilledSkills}/${result.totalLearnedSkills} 条新增 · 共 ${result.keywordsInserted} 个关键词`,
        })
      }
    } catch (err) {
      toast.error('回填关键词失败', { description: String(err) })
    } finally {
      setBackfilling(false)
    }
  }

  const isEmpty = !loading && learned.length === 0 && builtin.length === 0
  const enabledLearnedCount = learned.filter((s) => s.enabled).length

  // ── 移动端：选中技能后切换到详情 ──
  const handleMobileSelect = React.useCallback((id: string) => {
    setSelectedId(id)
    setMobileShowDetail(true)
  }, [])
  const handleMobileBack = React.useCallback(() => {
    setMobileShowDetail(false)
    // 不取消选中，但返回列表视图
  }, [])

  // ── 紧凑模式切换 ──
  const toggleCompact = React.useCallback(() => setCompactMode((v) => !v), [])

  return (
    <div ref={containerRef} className="flex flex-col h-full min-h-0">
      {/* ─── 头部区域：标题 + 搜索 + 筛选 + 操作 ─── */}
      <div className="shrink-0 border-b border-border">
        <ModuleHeader
          group="capability"
          title="技能"
        />
        {/* 搜索框 + lifecycle filter + 创建按钮 + 布局控件 */}
        <div className="titlebar-no-drag flex items-center gap-3 px-5 md:px-8 pb-4">
          <div className="relative flex-1 max-w-[160px] md:max-w-xs">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground/60 pointer-events-none" />
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="搜索技能…"
              className="h-8 pl-8 text-[12px] rounded-lg border-border/50 focus-visible:ring-2 focus-visible:ring-ring/20 focus-visible:border-ring/40 transition-all duration-200"
            />
          </div>
          {/* Filter tabs — 小屏隐藏 */}
          <div className="hidden md:flex gap-1 flex-wrap">
            {filterTabs.map((tab) => (
              <button
                key={tab.value}
                type="button"
                onClick={() => setActiveFilter(tab.value)}
                className={cn(
                  'rounded-full px-3 py-1.5 text-[11px] font-medium transition-all duration-200 whitespace-nowrap active:scale-95',
                  activeFilter === tab.value
                    ? 'bg-primary text-primary-foreground shadow-[0_1px_4px_rgba(0,0,0,0.1)]'
                    : 'text-muted-foreground hover:text-foreground hover:bg-muted/60',
                )}
              >
                {tab.label}
                <span className="ml-1 opacity-70 tabular-nums">({tab.count})</span>
              </button>
            ))}
          </div>
          {/* Spacer */}
          <div className="flex-1" />
          {/* 布局切换按钮（仅桌面端） */}
          {!isMobile && (
            <Button
              size="sm"
              variant="ghost"
              onClick={toggleCompact}
              title={effectiveCompact ? '切换为卡片模式' : '切换为紧凑模式'}
              className="h-8 px-2.5 text-[11px] gap-1.5 text-muted-foreground hover:text-foreground rounded-lg"
            >
              {effectiveCompact ? <Columns className="size-3.5" /> : <List className="size-3.5" />}
              <span className="hidden lg:inline">{effectiveCompact ? '卡片' : '紧凑'}</span>
            </Button>
          )}
          {/* 创建按钮 */}
          <Button
            size="sm"
            onClick={() => setCreateDialogOpen(true)}
            className="h-8 text-[12px] gap-1.5 rounded-lg shadow-[0_1px_3px_rgba(0,0,0,0.06)] hover:shadow-[0_2px_6px_rgba(0,0,0,0.1)] transition-all duration-200"
          >
            <Plus className="size-3.5" />
            <span className="hidden sm:inline">自定义技能</span>
          </Button>
        </div>
        {/* 移动端筛选标签（横向滚动） */}
        {isMobile && (
          <div className="titlebar-no-drag flex gap-1 px-5 pb-3 overflow-x-auto scrollbar-none">
            {filterTabs.map((tab) => (
              <button
                key={tab.value}
                type="button"
                onClick={() => setActiveFilter(tab.value)}
                className={cn(
                  'rounded-full px-2.5 py-1 text-[10.5px] font-medium transition-all duration-200 whitespace-nowrap shrink-0 active:scale-95',
                  activeFilter === tab.value
                    ? 'bg-primary text-primary-foreground shadow-[0_1px_4px_rgba(0,0,0,0.1)]'
                    : 'text-muted-foreground hover:text-foreground hover:bg-muted/60',
                )}
              >
                {tab.label}
                <span className="ml-0.5 opacity-70 tabular-nums">({tab.count})</span>
              </button>
            ))}
          </div>
        )}
      </div>

      {/* P2: Consolidation progress bar */}
      {proposing && consolidationProgress && (
        <div className="shrink-0 border-b border-border/40 bg-muted/20 px-5 md:px-8 py-2.5">
          <div className="flex items-center gap-3">
            <Loader2 className="size-3.5 text-primary animate-spin shrink-0" />
            <div className="flex-1 min-w-0">
              <div className="flex items-center justify-between gap-2 mb-1">
                <span className="text-[11px] text-muted-foreground truncate">
                  {consolidationProgress.detail}
                </span>
                {consolidationProgress.total > 1 && (
                  <span className="text-[10px] text-muted-foreground/60 tabular-nums shrink-0">
                    {consolidationProgress.current}/{consolidationProgress.total}
                  </span>
                )}
              </div>
              <div className="h-1 bg-muted rounded-full overflow-hidden">
                <div
                  className={cn(
                    'h-full rounded-full transition-all duration-500 ease-out',
                    consolidationProgress.stage === 'retrying' ? 'bg-amber-500' : 'bg-primary',
                  )}
                  style={{
                    width: `${consolidationProgress.total > 0
                      ? Math.round((consolidationProgress.current / consolidationProgress.total) * 100)
                      : 0}%`,
                  }}
                />
              </div>
            </div>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => void onCancelConsolidation()}
              className="h-7 px-2 text-[11px] gap-1 text-muted-foreground hover:text-destructive rounded-md shrink-0"
            >
              <X className="size-3" />
              <span className="hidden sm:inline">取消</span>
            </Button>
          </div>
        </div>
      )}

      {/* ─── 主体区域 ─── */}
      {isEmpty ? (
        <div className="titlebar-no-drag flex-1 min-h-0 flex items-center justify-center">
          <div className="flex flex-col items-center gap-4 text-center px-8">
            <div className="relative">
              <div className="absolute inset-0 rounded-2xl bg-primary/5 blur-xl" />
              <div className="relative size-16 rounded-2xl bg-muted/40 border border-border/30 flex items-center justify-center">
                <Search className="size-7 text-muted-foreground/30" />
              </div>
            </div>
            <div className="space-y-1.5">
              <div className="text-[14px] font-medium text-foreground/60">你的 Agent 还没学到技能</div>
              <div className="text-[11.5px] text-muted-foreground/60 max-w-[240px]">
                让它处理几次任务就会积累。也可以点击右上角「自定义技能」手动创建
              </div>
            </div>
          </div>
        </div>
      ) : isMobile ? (
        /* ── 移动端布局：全宽列表或全宽详情 ── */
        <div className="titlebar-no-drag flex-1 min-h-0 relative overflow-hidden">
          {/* 列表视图 */}
          <div className={cn(
            'absolute inset-0 transition-transform duration-300 ease-out',
            mobileShowDetail ? '-translate-x-full opacity-0' : 'translate-x-0 opacity-100',
          )}>
            <SkillsList
              learned={displayLearned}
              userSkills={displayUserSkills}
              bundledSkills={displayBundledSkills}
              selectedId={selectedId}
              loading={loading}
              canPropose={enabledLearnedCount >= 2 && !proposing}
              proposing={proposing}
              backfilling={backfilling}
              onSelect={handleMobileSelect}
              onReload={() => void onReload()}
              onPropose={() => void onPropose()}
              onBackfill={() => void onBackfill()}
              compact={false}
            />
          </div>
          {/* 详情视图 */}
          <div className={cn(
            'absolute inset-0 flex flex-col transition-transform duration-300 ease-out',
            mobileShowDetail ? 'translate-x-0 opacity-100' : 'translate-x-full opacity-0',
          )}>
            <div className="flex items-center gap-2 px-4 py-2.5 border-b border-border/40 bg-background/90 backdrop-blur-xl shrink-0">
              <Button
                size="sm"
                variant="ghost"
                onClick={handleMobileBack}
                className="h-8 px-2 text-[12px] gap-1.5 rounded-lg"
              >
                <ArrowLeft className="size-4" />
                返回列表
              </Button>
            </div>
            <div className="flex-1 min-h-0">
              <SkillDetail
                skill={selected}
                forking={selected?.kind === 'builtin' && forkingName === selected.name}
                onToggleEnabled={(s, next) => void onToggleEnabled(s, next)}
                onRequestDelete={(s) => setPendingDelete(s)}
                onFork={(name) => void onFork(name)}
                onLifecycleChanged={() => void refetch()}
              />
            </div>
          </div>
        </div>
      ) : (
        /* ── 桌面端布局：固定 2:3 比例双栏 ── */
        <div className="titlebar-no-drag flex-1 min-h-0 flex">
          <div className="h-full min-w-0" style={{ flex: 2 }}>
            <SkillsList
              learned={displayLearned}
              userSkills={displayUserSkills}
              bundledSkills={displayBundledSkills}
              selectedId={selectedId}
              loading={loading}
              canPropose={enabledLearnedCount >= 2 && !proposing}
              proposing={proposing}
              backfilling={backfilling}
              onSelect={setSelectedId}
              onReload={() => void onReload()}
              onPropose={() => void onPropose()}
              onBackfill={() => void onBackfill()}
              compact={effectiveCompact}
            />
          </div>
          {/* 分割线 */}
          <div className="w-px bg-border/60 shrink-0" />
          <div className="h-full min-w-0" style={{ flex: 3 }}>
            <SkillDetail
              skill={selected}
              forking={selected?.kind === 'builtin' && forkingName === selected.name}
              onToggleEnabled={(s, next) => void onToggleEnabled(s, next)}
              onRequestDelete={(s) => setPendingDelete(s)}
              onFork={(name) => void onFork(name)}
              onLifecycleChanged={() => void refetch()}
            />
          </div>
        </div>
      )}

      {/* ─── Dialogs ─── */}
      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => {
          if (!open) setPendingDelete(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>删除技能？</AlertDialogTitle>
            <AlertDialogDescription asChild>
              <div className="space-y-2 text-sm text-muted-foreground">
                <p>即将删除「{pendingDelete?.name ?? ''}」，连同它的版本、关键词和关联边都会被清除。</p>
                <p>这个操作无法撤销。</p>
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>取消</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => void onConfirmDelete()}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              删除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <SkillConsolidationDialog
        open={consolidationOpen}
        proposal={proposal}
        onOpenChange={(next) => {
          setConsolidationOpen(next)
          if (!next) setProposal(null)
        }}
        onApplied={() => {
          void refetch()
        }}
      />

      <CreateSkillDialog
        open={createDialogOpen}
        onOpenChange={setCreateDialogOpen}
        onCreated={() => void refetch()}
      />
    </div>
  )
}
