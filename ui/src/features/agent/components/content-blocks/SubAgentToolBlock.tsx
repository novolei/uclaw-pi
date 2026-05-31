/**
 * SubAgentToolBlock — the Agent/Task (sub-agent) render path of a tool_use block.
 *
 * Collapsible group: header row (chevron + status + phrase + child-tool count),
 * then on expand a prompt row + the nested child ContentBlocks + a SubAgentFooter
 * (final output text + usage stats). Extracted from ToolUseBlock during the
 * features/agent ContentBlock split so each file stays ≤ ~300 lines.
 *
 * Presentational: the result lookup / phrase / icon / status are computed by the
 * parent ToolUseBlock and passed in; this file owns only the sub-agent-specific
 * bits (prompt row, footer, result-text parsing).
 */

import * as React from 'react'
import { ChevronRight, XCircle, Loader2, MessageSquareText } from 'lucide-react'
import { cn } from '@/lib/utils'
import { MessageResponse } from '@/components/ai-elements/message'
// formatDuration lives in the still-un-migrated AgentMessages core; absolute
// import is temporary until AgentMessages migrates into features/agent.
import { formatDuration } from '@/components/agent/AgentMessages'
import { ContentBlock } from '../ContentBlock'
import type { SDKContentBlock, SDKMessage } from './types'

// ===== SubAgent 用量元数据 =====

export interface SubAgentMeta {
  durationMs: number
  totalTokens: number
  toolUses: number
}

// ===== SubAgent 结果文本解析 =====

interface ParsedAgentResult {
  /** 清理后的输出文本（去除元数据） */
  text: string
  /** 从 <usage> 标签解析的用量数据（作为 task_notification 的备用） */
  usage?: SubAgentMeta
}

/** 从 Agent tool_result 文本中分离内容与元数据（agentId 行 + <usage> 标签） */
function parseAgentResultText(raw: string): ParsedAgentResult {
  let text = raw

  // 提取 <usage> 标签中的用量数据
  let usage: SubAgentMeta | undefined
  const usageMatch = text.match(/<usage>([\s\S]*?)<\/usage>/)
  if (usageMatch) {
    const body = usageMatch[1]!
    const totalTokens = Number(body.match(/total_tokens:\s*(\d+)/)?.[1]) || 0
    const toolUses = Number(body.match(/tool_uses:\s*(\d+)/)?.[1]) || 0
    const durationMs = Number(body.match(/duration_ms:\s*(\d+)/)?.[1]) || 0
    if (totalTokens > 0 || toolUses > 0 || durationMs > 0) {
      usage = { durationMs, totalTokens, toolUses }
    }
    text = text.replace(/<usage>[\s\S]*?<\/usage>/, '')
  }

  // 移除 agentId 行
  text = text.replace(/agentId:.*\n?/g, '')

  // 移除 <output> 标签包裹
  text = text.replace(/<\/?output>/g, '')

  return { text: text.trim(), usage }
}

// ===== SubAgent 完成信息尾部 =====

function SubAgentFooter({
  meta,
  resultText,
}: {
  meta: SubAgentMeta | null
  resultText?: string
}): React.ReactElement | null {
  // 解析结果文本，分离内容与元数据
  const parsed = React.useMemo(
    () => resultText ? parseAgentResultText(resultText) : null,
    [resultText],
  )

  // 优先使用 task_notification 的用量数据，备用从 result 文本中解析
  const effectiveMeta = meta ?? parsed?.usage ?? null
  const cleanText = parsed?.text || ''

  // 没有任何信息时不渲染
  if (!effectiveMeta && !cleanText) return null

  return (
    <div className="mt-2 pt-2 border-t border-border/20 space-y-1.5">
      {/* 最终输出文本（Markdown 渲染） */}
      {cleanText && (
        <div className="text-muted-foreground/70">
          <MessageResponse>{cleanText}</MessageResponse>
        </div>
      )}

      {/* 用量统计行（最底部） */}
      {effectiveMeta && (
        <div className="flex items-center gap-3 text-[12px] text-muted-foreground/60 tabular-nums">
          {effectiveMeta.durationMs > 0 && (
            <span>{formatDuration(effectiveMeta.durationMs)}</span>
          )}
          {effectiveMeta.totalTokens > 0 && (
            <span>{effectiveMeta.totalTokens.toLocaleString()} tokens</span>
          )}
          {effectiveMeta.toolUses > 0 && (
            <span>{effectiveMeta.toolUses} 次工具调用</span>
          )}
        </div>
      )}
    </div>
  )
}

// ===== 提示词折叠行 =====

function PromptRow({ prompt, dimmed = false }: { prompt: string; dimmed?: boolean }): React.ReactElement {
  const [expanded, setExpanded] = React.useState(false)
  const preview = prompt.length > 60 ? prompt.slice(0, 60) + '…' : prompt

  return (
    <div>
      <button
        type="button"
        className="flex items-center gap-2 py-0.5 text-left hover:opacity-70 transition-opacity group"
        onClick={() => setExpanded(!expanded)}
      >
        <MessageSquareText className={cn('size-3.5 shrink-0', dimmed ? 'text-muted-foreground/70' : 'text-muted-foreground')} />

        <span className={cn(
          'shrink-0 text-[14px]',
          dimmed ? 'text-muted-foreground/70' : 'text-muted-foreground',
        )}>提示词</span>

        <span className={cn(
          'truncate text-[14px]',
          dimmed ? 'text-muted-foreground/50' : 'text-muted-foreground/60',
        )}>
          {preview}
        </span>

        <ChevronRight
          className={cn(
            'shrink-0 size-3 text-muted-foreground/40 opacity-0 group-hover:opacity-100 transition-all duration-150',
            expanded && 'rotate-90 opacity-100',
          )}
        />
      </button>

      {expanded && (
        <div className="ml-5.5 mt-1 mb-2 pl-3 border-l-2 border-border/30 animate-in fade-in slide-in-from-top-1 duration-150">
          <p className="text-[13px] text-foreground/70 leading-relaxed whitespace-pre-wrap break-words">
            {prompt}
          </p>
        </div>
      )}
    </div>
  )
}

// ===== Agent/Task 子代理工具块 =====

export interface SubAgentToolBlockProps {
  allMessages: SDKMessage[]
  childBlocks?: SDKContentBlock[]
  dimmed: boolean
  animate: boolean
  /** 动画延迟（由父级计算） */
  delay: string
  /** 完成态/进行态短语标签（由父级计算） */
  displayLabel: string
  /** 工具图标组件（由父级 getToolIcon 计算） */
  ToolIcon: React.ComponentType<{ className?: string }>
  isCompleted: boolean
  isError: boolean
  isStreaming?: boolean
  /** Agent/Task prompt（由父级从 block.input 提取） */
  agentPrompt?: string
  /** 已完成时的结果文本 */
  resultText?: string
  /** task_notification 提取的用量数据 */
  subAgentMeta: SubAgentMeta | null
}

export function SubAgentToolBlock({
  allMessages,
  childBlocks,
  dimmed,
  animate,
  delay,
  displayLabel,
  ToolIcon,
  isCompleted,
  isError,
  isStreaming,
  agentPrompt,
  resultText,
  subAgentMeta,
}: SubAgentToolBlockProps): React.ReactElement {
  // Agent/Task 子代理内容默认折叠
  const [childrenExpanded, setChildrenExpanded] = React.useState(false)

  const hasChildren = childBlocks && childBlocks.length > 0
  const childToolCount = childBlocks?.filter((b) => b.type === 'tool_use').length ?? 0

  return (
    <div
      className={cn(
        animate && 'animate-in fade-in duration-150 fill-mode-both',
      )}
      style={animate ? { animationDelay: delay } : undefined}
    >
      {/* 头部行：折叠箭头 + 状态 + 语义短语 */}
      <button
        type="button"
        className="w-full flex items-center gap-2 py-0.5 text-left hover:opacity-70 transition-opacity group"
        onClick={() => setChildrenExpanded(!childrenExpanded)}
      >
        <ChevronRight
          className={cn(
            'size-3 text-muted-foreground/50 transition-transform duration-150 shrink-0',
            childrenExpanded && 'rotate-90',
          )}
        />

        {/* 状态指示：仅流式中的未完成工具才显示 spinner */}
        {!isCompleted && isStreaming ? (
          <Loader2 className="size-3.5 animate-spin text-primary/50 shrink-0" />
        ) : isError ? (
          <XCircle className="size-3.5 text-destructive/70 shrink-0" />
        ) : null}

        <ToolIcon className={cn('size-3.5 shrink-0', dimmed ? 'text-muted-foreground/70' : 'text-muted-foreground')} />

        <span className={cn(
          'truncate text-[14px]',
          dimmed ? 'text-muted-foreground/70' : 'text-muted-foreground',
        )}>{displayLabel}</span>

        {/* 子工具计数（折叠时显示） */}
        {childToolCount > 0 && !childrenExpanded && (
          <span className="shrink-0 text-[11px] text-muted-foreground/50 tabular-nums">
            {childToolCount} 项工具调用
          </span>
        )}
      </button>

      {/* 展开内容 */}
      {childrenExpanded && (
        <div className="pl-5 mt-1.5 space-y-2 border-l-2 border-primary/20 ml-[5px] animate-in fade-in slide-in-from-top-1 duration-150">
          {/* 提示词：可折叠行 */}
          {agentPrompt && <PromptRow prompt={agentPrompt} dimmed={dimmed} />}

          {/* 子代理工具调用 */}
          {hasChildren && childBlocks.map((childBlock, ci) => (
            <ContentBlock
              key={ci}
              block={childBlock}
              allMessages={allMessages}
              animate
              index={ci}
              dimmed
              isStreaming={isStreaming}
            />
          ))}

          {/* SubAgent 完成信息 */}
          {isCompleted && (
            <SubAgentFooter
              meta={subAgentMeta}
              resultText={resultText}
            />
          )}
        </div>
      )}
    </div>
  )
}
