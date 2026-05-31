/**
 * ToolUseBlock — renders a single `tool_use` content block.
 *
 * Computes the tool result (useToolResult), sub-agent usage (useSubAgentMeta),
 * phrase + icon + status once, then branches:
 * - Agent/Task tools → <SubAgentToolBlock> (collapsible sub-agent group).
 * - Plain tools → semantic phrase row that expands a <ToolResultRenderer>.
 *
 * Extracted from ContentBlock.tsx during the features/agent migration so the
 * dispatcher stays thin; the sub-agent group was further split into
 * `SubAgentToolBlock.tsx` to keep each file ≤ ~300 lines.
 */

import * as React from 'react'
import { ChevronRight, XCircle, Loader2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import { getToolIcon, getToolPhrase, ToolResultRenderer } from '@/shared/tool-rendering'
import { SubAgentToolBlock, type SubAgentMeta } from './SubAgentToolBlock'
import type {
  SDKContentBlock,
  SDKMessage,
  SDKToolUseBlock,
  SDKUserMessage,
  SDKToolResultBlock,
  SDKSystemMessage,
} from './types'

// ===== useToolResult Hook =====

interface ToolResultData {
  result?: string
  isError?: boolean
}

/** 在 allMessages 中查找匹配 toolUseId 的工具结果 */
function useToolResult(toolUseId: string, allMessages: SDKMessage[]): ToolResultData | null {
  return React.useMemo(() => {
    for (const msg of allMessages) {
      if (msg.type !== 'user') continue
      const userMsg = msg as SDKUserMessage
      const contentBlocks = userMsg.message?.content
      if (!Array.isArray(contentBlocks)) continue

      for (const block of contentBlocks) {
        if (block.type === 'tool_result') {
          const resultBlock = block as SDKToolResultBlock
          if (resultBlock.tool_use_id === toolUseId) {
            let result: string | undefined
            if (typeof resultBlock.content === 'string') {
              result = resultBlock.content
            } else if (Array.isArray(resultBlock.content)) {
              result = (resultBlock.content as Array<{ type: string; text?: string }>)
                .filter((c) => c.type === 'text' && typeof c.text === 'string')
                .map((c) => c.text)
                .join('\n')
            }
            return { result, isError: resultBlock.is_error }
          }
        }
      }
    }
    return null
  }, [toolUseId, allMessages])
}

// ===== useSubAgentMeta Hook =====

/** 从 allMessages 中查找匹配 toolUseId 的 task_notification 系统消息，提取用量数据 */
function useSubAgentMeta(toolUseId: string, allMessages: SDKMessage[]): SubAgentMeta | null {
  return React.useMemo(() => {
    for (const msg of allMessages) {
      if (msg.type !== 'system') continue
      const sysMsg = msg as SDKSystemMessage
      if (sysMsg.subtype !== 'task_notification') continue
      if (sysMsg.tool_use_id !== toolUseId) continue
      const usage = sysMsg.usage
      if (!usage) return null
      return {
        durationMs: usage.duration_ms ?? 0,
        totalTokens: usage.total_tokens ?? 0,
        toolUses: usage.tool_uses ?? 0,
      }
    }
    return null
  }, [toolUseId, allMessages])
}

// ===== 工具短语 diff 着色 =====

/** 将 displayLabel 中的 +N 染绿、-N 染红（仅对 Edit/Write 工具生效，避免 `head -5` 等命令参数被误染） */
function renderLabelWithDiffColors(label: string, toolName: string): React.ReactNode {
  if (toolName !== 'Edit' && toolName !== 'Write') return label
  const parts = label.split(/((?:^|(?<=\s))[+-]\d+)/g)
  if (parts.length === 1) return label
  return parts.map((part, i) => {
    if (/^\+\d+$/.test(part)) {
      return <span key={i} className="text-green-500">{part}</span>
    }
    if (/^-\d+$/.test(part)) {
      return <span key={i} className="text-red-500">{part}</span>
    }
    return part
  })
}

// ===== 工具调用块 =====

export interface ToolUseBlockProps {
  block: SDKToolUseBlock
  allMessages: SDKMessage[]
  animate?: boolean
  index?: number
  dimmed?: boolean
  childBlocks?: SDKContentBlock[]
  /** 是否正在流式输出中 */
  isStreaming?: boolean
}

export function ToolUseBlock({ block, allMessages, animate = false, index = 0, dimmed = false, childBlocks, isStreaming }: ToolUseBlockProps): React.ReactElement {
  const [expanded, setExpanded] = React.useState(false)
  const toolResult = useToolResult(block.id, allMessages)
  const isAgentTool = block.name === 'Agent' || block.name === 'Task'
  const subAgentMeta = useSubAgentMeta(block.id, allMessages)

  const phrase = getToolPhrase(block.name, block.input)
  const ToolIcon = getToolIcon(block.name)

  const isCompleted = toolResult !== null
  const isError = toolResult?.isError === true

  // 运行中显示进行时短语，完成或非流式（已终止）显示完成态短语
  const displayLabel = (isCompleted || !isStreaming) ? phrase.label : phrase.loadingLabel

  const delay = animate && index < 10 ? `${index * 30}ms` : '0ms'

  // ===== Agent/Task 工具：特殊渲染 =====
  if (isAgentTool) {
    // 提取 prompt 用于气泡展示
    const agentPrompt =
      typeof block.input.prompt === 'string' ? block.input.prompt : undefined
    return (
      <SubAgentToolBlock
        allMessages={allMessages}
        childBlocks={childBlocks}
        dimmed={dimmed}
        animate={animate}
        delay={delay}
        displayLabel={displayLabel}
        ToolIcon={ToolIcon}
        isCompleted={isCompleted}
        isError={isError}
        isStreaming={isStreaming}
        agentPrompt={agentPrompt}
        resultText={toolResult?.result}
        subAgentMeta={subAgentMeta}
      />
    )
  }

  // ===== 普通工具：语义化短语 + 结构化结果 =====
  return (
    <div
      className={cn(
        animate && 'animate-in fade-in duration-150 fill-mode-both',
      )}
      style={animate ? { animationDelay: delay } : undefined}
    >
      <button
        type="button"
        className="flex items-center gap-2 py-0.5 text-left hover:opacity-70 transition-opacity group"
        onClick={() => setExpanded(!expanded)}
      >
        {!isCompleted && isStreaming ? (
          <Loader2 className="size-3.5 animate-spin text-primary/50 shrink-0" />
        ) : isError ? (
          <XCircle className="size-3.5 text-destructive/70 shrink-0" />
        ) : null}

        <ToolIcon className={cn('size-3.5 shrink-0', dimmed ? 'text-muted-foreground/70' : 'text-muted-foreground')} />

        <span className={cn(
          'truncate text-[14px]',
          dimmed ? 'text-muted-foreground/70' : 'text-muted-foreground',
        )}>{renderLabelWithDiffColors(displayLabel, block.name)}</span>

        <ChevronRight
          className={cn(
            'shrink-0 size-3 text-muted-foreground/40 opacity-0 group-hover:opacity-100 transition-all duration-150',
            expanded && 'rotate-90 opacity-100',
          )}
        />
      </button>

      {expanded && toolResult?.result && (
        <div className="ml-5.5 mt-1 mb-2 pl-3 border-l-2 border-border/30 animate-in fade-in slide-in-from-top-1 duration-150">
          <ToolResultRenderer
            toolName={block.name}
            input={block.input}
            result={toolResult.result}
            isError={isError}
          />
        </div>
      )}
    </div>
  )
}
