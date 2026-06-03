/**
 * MemoryModule — 万花筒「记忆」模块。
 *
 * 使用 ModuleHeader + pill-style tabs 切换四个视图:
 * 星云图 / Boot / 时间线 / 搜索。数据一次性加载后传递给子组件。
 */
import * as React from 'react'
import { Network, Star, Clock, Search, Sparkles, BookOpen, FileText, ShieldCheck, Orbit, Brain } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { cn } from '@/lib/utils'
import { memoryGraphGetFullGraph } from '@/lib/tauri-bridge'
import type { MemoryGraphData } from '@/lib/types'
import { ModuleHeader } from '../../shared/ModuleHeader'
import { MemoryNebulaView } from '@/components/memory/MemoryNebulaView'
import { MemoryBootList } from '@/components/memory/MemoryBootList'
import { MemoryTimeline } from '@/components/memory/MemoryTimeline'
import { MemorySearchPanel } from '@/components/memory/MemorySearchPanel'
import { FragmentCardView } from '@/components/memory/FragmentCardView'
import { DailySummaryView } from '@/components/memory/DailySummaryView'
import { MemoryNodeCard } from '@/components/memory/MemoryNodeCard'
import { WikiView } from '@/components/memory/WikiView'
import { MemoryHealthPanel } from '@/components/memory/MemoryHealthPanel'
import { DualNebulaView } from '@/components/memory/DualNebulaView'
import { gbrainFullGraph, type KnowledgeGraph } from '@/lib/gbrain-browse'
import { AgentGrowthTab } from './AgentGrowthTab'

// ─── Tab 定义 ───────────────────────────────────────────────────────────

type MemoryTab = 'nebula' | 'boot' | 'timeline' | 'search' | 'fragments' | 'daily' | 'wiki' | 'health' | 'dual' | 'self'

const TABS: { value: MemoryTab; label: string; icon: React.ElementType }[] = [
  { value: 'nebula', label: '星云图', icon: Network },
  { value: 'boot', label: 'Boot', icon: Star },
  { value: 'timeline', label: '时间线', icon: Clock },
  { value: 'search', label: '搜索', icon: Search },
  { value: 'fragments', label: '碎片', icon: Sparkles },
  { value: 'daily', label: '日记', icon: BookOpen },
  // Memory OS Foundation Phase 3 — AI Wiki view.
  { value: 'wiki', label: 'Wiki', icon: FileText },
  // Dual nebula — fused memory + knowledge graph.
  { value: 'dual', label: '双星云', icon: Orbit },
  // Memory OS Foundation Phase 4 — Health findings panel.
  { value: 'health', label: 'Health', icon: ShieldCheck },
  // Memory inspector — agent's learned self (reflections / user_model / daydreams + live).
  { value: 'self', label: '成长', icon: Brain },
]

export function MemoryModule(): React.ReactElement {
  const [activeTab, setActiveTab] = React.useState<MemoryTab>('nebula')
  const [graphData, setGraphData] = React.useState<MemoryGraphData | null>(null)
  const [selectedNodeId, setSelectedNodeId] = React.useState<string | null>(null)
  const [knowledgeGraph, setKnowledgeGraph] = React.useState<KnowledgeGraph | null>(null)
  const [knowledgeError, setKnowledgeError] = React.useState(false)
  const [pendingWikiSlug, setPendingWikiSlug] = React.useState<string | undefined>(undefined)

  // 一次性加载图数据
  React.useEffect(() => {
    let cancelled = false
    const load = async () => {
      try {
        const res = await memoryGraphGetFullGraph()
        if (!cancelled) {
          setGraphData(res as MemoryGraphData)
        }
      } catch (err) {
        console.error('[MemoryModule] 加载记忆图失败:', err)
      }
    }
    void load()
    return () => { cancelled = true }
  }, [])

  // 懒加载 knowledge graph — 仅在 dual tab 首次激活时请求。
  // 失败后设置 knowledgeError=true，避免重试紧循环。
  React.useEffect(() => {
    if (activeTab !== 'dual' || knowledgeGraph !== null || knowledgeError) return
    let cancelled = false
    gbrainFullGraph(150)
      .then((g) => { if (!cancelled) setKnowledgeGraph(g) })
      .catch(() => { if (!cancelled) setKnowledgeError(true) })
    return () => { cancelled = true }
  }, [activeTab, knowledgeGraph, knowledgeError])

  // 离开 wiki tab 时清除 pendingWikiSlug，防止重新切回时重打开旧页面。
  // 注意：dual→wiki 同批次切换时，activeTab 已是 'wiki'，此 effect 不会清除。
  React.useEffect(() => {
    if (activeTab !== 'wiki' && pendingWikiSlug !== undefined) setPendingWikiSlug(undefined)
  }, [activeTab, pendingWikiSlug])

  // 统计信息
  const subtitle = React.useMemo(() => {
    if (!graphData) return '加载中…'
    const total = graphData.nodes.length
    const bootCount = graphData.nodes.filter((n) => n.kind === 'boot').length
    return `${total} 条记忆 · ${bootCount} 个 Boot`
  }, [graphData])

  const handleSelectNode = (nodeId: string): void => {
    setSelectedNodeId(nodeId)
  }

  const handleNodeDeleted = (): void => {
    setSelectedNodeId(null)
  }

  return (
    <div className="flex flex-col h-full min-h-0">
      {/* Header */}
      <ModuleHeader
        group="capability"
        title="记忆"
        subtitle={subtitle}
      />

      {/* Pill-style tab bar */}
      <div className="titlebar-no-drag flex items-center gap-1 px-8 pb-4">
        {TABS.map((tab) => {
          const Icon = tab.icon
          return (
            <button
              key={tab.value}
              type="button"
              onClick={() => setActiveTab(tab.value)}
              className={cn(
                'rounded-full px-3 py-1.5 text-[11px] font-medium transition-all duration-200 whitespace-nowrap active:scale-95 flex items-center gap-1',
                activeTab === tab.value
                  ? 'bg-primary text-primary-foreground shadow-[0_1px_4px_rgba(0,0,0,0.1)]'
                  : 'text-muted-foreground hover:text-foreground hover:bg-muted/60',
              )}
            >
              <Icon className="size-3" />
              {tab.label}
            </button>
          )
        })}
      </div>

      {/* Tab content — fills remaining space */}
      <div className="titlebar-no-drag flex-1 min-h-0 px-8 pb-4">
        {activeTab === 'nebula' && (
          <MemoryNebulaView
            graphData={graphData}
            onSelectNode={handleSelectNode}
            className="h-full w-full rounded-xl overflow-hidden border border-border/40"
          />
        )}
        {activeTab === 'boot' && (
          <MemoryBootList
            onSelectNode={handleSelectNode}
            className="h-full"
          />
        )}
        {activeTab === 'timeline' && (
          <MemoryTimeline
            onSelectNode={handleSelectNode}
            className="h-full"
          />
        )}
        {activeTab === 'search' && (
          <MemorySearchPanel
            onSelectNode={handleSelectNode}
            className="h-full"
          />
        )}
        {activeTab === 'fragments' && (
          <FragmentCardView onSelectNode={handleSelectNode} />
        )}
        {activeTab === 'daily' && (
          <DailySummaryView />
        )}
        {activeTab === 'wiki' && (
          <WikiView initialSlug={pendingWikiSlug} className="h-full w-full rounded-xl overflow-hidden border border-border/40" />
        )}
        {activeTab === 'dual' && (
          <div className="relative h-full w-full">
            <DualNebulaView
              memory={graphData}
              knowledge={knowledgeGraph}
              onSelect={(id, layer) => {
                if (layer === 'memory') setSelectedNodeId(id)
                else { setPendingWikiSlug(id); setActiveTab('wiki') }
              }}
              className="h-full w-full rounded-xl overflow-hidden border border-border/40"
            />
            {knowledgeError && (
              <div className="absolute top-2 right-2 text-[10px] px-2 py-1 rounded bg-destructive/10 text-destructive border border-destructive/30">
                知识层未连接
              </div>
            )}
          </div>
        )}
        {activeTab === 'health' && (
          <MemoryHealthPanel
            onSelectSubject={handleSelectNode}
            className="h-full w-full rounded-xl overflow-hidden border border-border/40"
          />
        )}
        {activeTab === 'self' && <AgentGrowthTab />}
      </div>

      {/* 节点详情弹窗 */}
      <Dialog
        open={!!selectedNodeId}
        onOpenChange={(open) => { if (!open) setSelectedNodeId(null) }}
      >
        <DialogContent className="sm:max-w-lg max-h-[80vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>记忆节点详情</DialogTitle>
          </DialogHeader>
          {selectedNodeId && (
            <MemoryNodeCard
              nodeId={selectedNodeId}
              onDeleted={handleNodeDeleted}
            />
          )}
        </DialogContent>
      </Dialog>
    </div>
  )
}
