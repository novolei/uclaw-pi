import React from 'react'
import { cn, safeParseDate } from '@/lib/utils'
import { Mic, Clipboard, Type, Clock } from 'lucide-react'
import type { FragmentItem } from '@/lib/tauri-bridge'

// 碎片子类型 → 记忆 kind 色板（theme tokens，见 docs/design/memory-ui.md）
// daily→fact, credential→entity, location→lesson, reminder→preference, inspiration→episode, bookmark→procedure
export const SUBTYPE_COLORS: Record<string, { bg: string; border: string; text: string; dot: string }> = {
  daily:       { bg: 'bg-memory-fact-bg', border: 'border-memory-fact/20', text: 'text-memory-fact', dot: 'bg-memory-fact' },
  credential:  { bg: 'bg-memory-entity-bg', border: 'border-memory-entity/20', text: 'text-memory-entity', dot: 'bg-memory-entity' },
  location:    { bg: 'bg-memory-lesson-bg', border: 'border-memory-lesson/20', text: 'text-memory-lesson', dot: 'bg-memory-lesson' },
  reminder:    { bg: 'bg-memory-preference-bg', border: 'border-memory-preference/20', text: 'text-memory-preference', dot: 'bg-memory-preference' },
  inspiration: { bg: 'bg-memory-episode-bg', border: 'border-memory-episode/20', text: 'text-memory-episode', dot: 'bg-memory-episode' },
  bookmark:    { bg: 'bg-memory-procedure-bg', border: 'border-memory-procedure/20', text: 'text-memory-procedure', dot: 'bg-memory-procedure' },
}

const DEFAULT_COLORS = SUBTYPE_COLORS.daily

// 标签中文名映射
const SUBTYPE_LABELS: Record<string, string> = {
  daily: '日常',
  credential: '凭证',
  location: '位置',
  reminder: '提醒',
  inspiration: '灵感',
  bookmark: '书签',
}

// 来源图标
function SourceIcon({ source }: { source: string }) {
  switch (source) {
    case 'voice': return <Mic className="size-3" />
    case 'clipboard': return <Clipboard className="size-3" />
    default: return <Type className="size-3" />
  }
}

interface FragmentCardProps {
  fragment: FragmentItem
  onClick?: () => void
  compact?: boolean  // CMD+K 搜索结果中使用紧凑模式
}

export function FragmentCard({ fragment, onClick, compact = false }: FragmentCardProps) {
  const tag = fragment.subtype || fragment.tags?.[0] || 'daily'
  const colors = SUBTYPE_COLORS[tag] || DEFAULT_COLORS
  
  // 格式化时间
  const timeStr = React.useMemo(() => {
    const date = safeParseDate(fragment.createdAt)
    if (!date) return '—'
    const now = new Date()
    const diff = now.getTime() - date.getTime()
    if (diff < 3600000) return `${Math.floor(diff / 60000)}分钟前`
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}小时前`
    if (diff < 604800000) return `${Math.floor(diff / 86400000)}天前`
    return date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' })
  }, [fragment.createdAt])

  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'w-full text-left rounded-lg border transition-all',
        'hover:shadow-sm hover:scale-[1.005] active:scale-[0.998]',
        colors.border,
        compact ? 'p-2.5' : 'p-3.5',
        'min-h-[44px]',  // 44px 最小点击目标
        'flex gap-3',
      )}
    >
      {/* 左侧颜色条 */}
      <div className={cn('w-1 shrink-0 rounded-full', colors.dot)} />
      
      {/* 内容区 */}
      <div className="flex-1 min-w-0 space-y-1.5">
        {/* 标题行 */}
        {fragment.title && (
          <h4 className={cn(
            'font-semibold truncate',
            compact ? 'text-[13px]' : 'text-[16px] leading-tight',
          )}>
            {fragment.title}
          </h4>
        )}
        
        {/* 内容预览 */}
        <p className={cn(
          'text-muted-foreground line-clamp-2',
          compact ? 'text-[12px]' : 'text-[14px]',
        )}>
          {fragment.content}
        </p>
        
        {/* 底部元信息 */}
        <div className="flex items-center gap-2 flex-wrap">
          {/* 标签 pill */}
          <span className={cn(
            'inline-flex items-center px-1.5 py-0.5 rounded-full text-[11px] font-medium',
            colors.bg, colors.text,
          )}>
            {SUBTYPE_LABELS[tag] || tag}
          </span>
          
          {/* 来源 */}
          <span className="inline-flex items-center gap-0.5 text-[11px] text-muted-foreground">
            <SourceIcon source={fragment.source} />
          </span>
          
          {/* 时间 */}
          <span className="text-[11px] text-muted-foreground">{timeStr}</span>
          
          {/* 复习状态 */}
          {fragment.reviewStatus && !fragment.reviewStatus.completed && (
            <span className="inline-flex items-center gap-0.5 text-[11px] text-danger">
              <Clock className="size-2.5" />
              复习中
            </span>
          )}
        </div>
      </div>
    </button>
  )
}
