/**
 * ContentBlock — 单个 SDKAssistantMessage 内容块渲染
 *
 * 支持三种内容块类型：
 * - text: 通过 MessageResponse 渲染 Markdown
 * - tool_use: 语义化短语行（如 "读取 foo.ts 第 10-60 行"），展开显示结构化结果
 * - thinking: 默认展开，左上角 "Thinking" 标签 + 虚线边框内容区
 */

import * as React from 'react'
import type { ComponentProps } from 'react'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { markdownFileChipPlugin } from '@/components/preview/chips/markdownFileChipPlugin'
import { FilePathChip } from '@/components/preview/chips/FilePathChip'
import { useFileChipResolver } from '@/components/preview/chips/useFileChipResolver'
import {
  ChevronRight,
  ChevronDown,
  XCircle,
  Loader2,
  Brain,
  MessageSquareText,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { MessageResponse } from '@/components/ai-elements/message'
import { getToolIcon, getToolPhrase, ToolResultRenderer } from '@/shared/tool-rendering'
import { formatDuration } from './AgentMessages'
// Local content-block types (SDK types removed; these structural equivalents are sufficient)
interface SDKContentBlock { type: string; text?: string; id?: string; name?: string; input?: Record<string, any>; thinking?: string; tool_use_id?: string; content?: string | SDKContentBlock[]; is_error?: boolean; [key: string]: unknown }
type SDKMessage = { type: string; uuid?: string; message?: { content?: SDKContentBlock[] | string }; subtype?: string; tool_use_id?: string; usage?: { duration_ms?: number; total_tokens?: number; tool_uses?: number }; [key: string]: unknown }
type SDKTextBlock = SDKContentBlock & { type: 'text'; text: string }
type SDKToolUseBlock = SDKContentBlock & { type: 'tool_use'; id: string; name: string; input: Record<string, any> }
type SDKThinkingBlock = SDKContentBlock & { type: 'thinking'; thinking: string }
type SDKUserMessage = SDKMessage & { type: 'user' }
type SDKToolResultBlock = SDKContentBlock & { type: 'tool_result'; tool_use_id: string; content?: string | SDKContentBlock[]; is_error?: boolean }
type SDKSystemMessage = SDKMessage & { type: 'system'; subtype?: string; tool_use_id?: string }

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

interface SubAgentMeta {
  durationMs: number
  totalTokens: number
  toolUses: number
}

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

// ===== ContentBlock Props =====

export interface ContentBlockProps {
  /** 内容块数据 */
  block: SDKContentBlock
  /** 所有消息（用于查找工具结果） */
  allMessages: SDKMessage[]
  /** 是否启用入场动画 */
  animate?: boolean
  /** 在父级中的索引（用于动画延迟） */
  index?: number
  /** 当 turn 中已有主要内容（text）时，非主要块（tool/thinking）颜色变淡 */
  dimmed?: boolean
  /** 子代理的内容块（Agent/Task 工具调用的嵌套子块） */
  childBlocks?: SDKContentBlock[]
  /** 是否正在流式输出中（仅流式中的未完成工具调用才显示 spinner） */
  isStreaming?: boolean
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

interface ToolUseBlockProps {
  block: SDKToolUseBlock
  allMessages: SDKMessage[]
  animate?: boolean
  index?: number
  dimmed?: boolean
  childBlocks?: SDKContentBlock[]
  /** 是否正在流式输出中 */
  isStreaming?: boolean
}

function ToolUseBlock({ block, allMessages, animate = false, index = 0, dimmed = false, childBlocks, isStreaming }: ToolUseBlockProps): React.ReactElement {
  const [expanded, setExpanded] = React.useState(false)
  const toolResult = useToolResult(block.id, allMessages)
  const isAgentTool = block.name === 'Agent' || block.name === 'Task'
  const hasChildren = isAgentTool && childBlocks && childBlocks.length > 0
  const subAgentMeta = useSubAgentMeta(block.id, allMessages)

  // Agent/Task 子代理内容默认折叠
  const [childrenExpanded, setChildrenExpanded] = React.useState(false)

  const phrase = getToolPhrase(block.name, block.input)
  const ToolIcon = getToolIcon(block.name)

  const isCompleted = toolResult !== null
  const isError = toolResult?.isError === true

  // 运行中显示进行时短语，完成或非流式（已终止）显示完成态短语
  const displayLabel = (isCompleted || !isStreaming) ? phrase.label : phrase.loadingLabel

  const delay = animate && index < 10 ? `${index * 30}ms` : '0ms'

  // Agent/Task: 提取 prompt 用于气泡展示
  const agentPrompt = isAgentTool
    ? (typeof block.input.prompt === 'string' ? block.input.prompt : undefined)
    : undefined

  // 子代理工具调用统计
  const childToolCount = childBlocks?.filter((b) => b.type === 'tool_use').length ?? 0

  // ===== Agent/Task 工具：特殊渲染 =====
  if (isAgentTool) {
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
                resultText={toolResult?.result}
              />
            )}
          </div>
        )}
      </div>
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

// ===== 思考块（默认展开，Thinking 标签 + 虚线边框） =====

/**
 * 极简 markdown components — 与 ChatToolBlock 展开体一致的 13px 样式，
 * 不引入 Tailwind Typography 的 prose 相关字号覆盖。
 */
const THINKING_MD_COMPONENTS = {
  // 段落继承容器的 13px / leading-relaxed
  p: ({ children }: { children?: React.ReactNode }) => (
    <p className="my-1 [&:first-child]:mt-0 [&:last-child]:mb-0">{children}</p>
  ),
  // 行内 code 渲染为小灰底 chip（和聊天 prose 中行内 code 风格一致）
  code: ({ children, className }: { children?: React.ReactNode; className?: string }) => (
    <code
      className={cn(
        'rounded bg-foreground/10 px-[0.35em] py-[0.15em] text-[0.875em] font-medium',
        className,
      )}
    >
      {children}
    </code>
  ),
  // 代码块：保持 13px 容器字号但用 mono + 轻微缩进
  pre: ({ children }: { children?: React.ReactNode }) => (
    <pre className="my-1.5 overflow-x-auto rounded bg-foreground/[0.04] px-2 py-1.5 text-[12.5px] leading-relaxed">
      {children}
    </pre>
  ),
  ul: ({ children }: { children?: React.ReactNode }) => (
    <ul className="my-1 list-disc pl-5 space-y-0.5">{children}</ul>
  ),
  ol: ({ children }: { children?: React.ReactNode }) => (
    <ol className="my-1 list-decimal pl-5 space-y-0.5">{children}</ol>
  ),
  a: ({ children, href }: { children?: React.ReactNode; href?: string }) => (
    <a href={href} className="text-primary hover:underline">{children}</a>
  ),
} as const

const THINKING_REMARK_PLUGINS = [remarkGfm, markdownFileChipPlugin]

// ===== Thinking chip adapter =====

interface ThinkingChipProps {
  rawPath: string
  label: string
  line?: number
  col?: number
}

function ThinkingFileChip(props: ThinkingChipProps & { sessionId: string | null }): React.ReactElement {
  const resolution = useFileChipResolver(props.rawPath, props.sessionId)
  return (
    <FilePathChip
      rawPath={props.rawPath}
      label={props.label}
      state={resolution.state}
      mountId={resolution.mountId}
      relPath={resolution.relPath}
      absolutePath={resolution.absolutePath}
      sessionId={props.sessionId}
      line={props.line}
      col={props.col}
    />
  )
}

interface ThinkingBlockProps {
  block: SDKThinkingBlock
  dimmed?: boolean
  /** 当前会话 ID — 用于文件 chip 路径解析 */
  sessionId?: string | null
}

export function ThinkingBlock({ block, dimmed = false, sessionId = null }: ThinkingBlockProps): React.ReactElement {
  const [isExpanded, setIsExpanded] = React.useState(false)

  const toggleExpand = React.useCallback(() => {
    setIsExpanded((prev) => !prev)
  }, [])

  const thinkingComponents = React.useMemo(
    () => ({
      ...THINKING_MD_COMPONENTS,
      'file-path-chip': (chipProps: ThinkingChipProps) => (
        <ThinkingFileChip {...chipProps} sessionId={sessionId} />
      ),
    }),
    [sessionId],
  ) as ComponentProps<typeof Markdown>['components']

  return (
    <div className={cn('mb-2', dimmed && 'opacity-60')}>
      <button
        type="button"
        onClick={toggleExpand}
        className={cn(
          'group flex items-center gap-1.5 rounded-md px-1.5 py-0.5 -mx-1.5 transition-colors hover:bg-muted/40',
          isExpanded ? 'mb-1.5' : 'mb-0',
        )}
      >
        <Brain className="size-3 text-muted-foreground/60 group-hover:text-muted-foreground transition-colors" />
        <span className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground/60 group-hover:text-muted-foreground transition-colors">
          Thinking
        </span>
        <ChevronRight
          className={cn(
            'size-3 text-muted-foreground/40 transition-all duration-200',
            isExpanded && 'rotate-90',
          )}
        />
      </button>
      {isExpanded && (
        <div
          className={cn(
            // 与 ChatToolBlock 展开面板风格统一：左边框 + 缩进 + 13px 字号
            'ml-[18px] mr-2 mt-1 mb-2 pl-3 pr-1 py-1.5',
            'border-l border-border/50 dark:border-border/60',
            'text-[13px] leading-relaxed',
            dimmed ? 'text-muted-foreground/60' : 'text-foreground/75',
            'animate-in fade-in slide-in-from-top-1 duration-150',
          )}
        >
          <Markdown remarkPlugins={THINKING_REMARK_PLUGINS} components={thinkingComponents}>
            {block.thinking}
          </Markdown>
        </div>
      )}
    </div>
  )
}

// ===== ContentBlock 主组件 =====

export function ContentBlock({ block, allMessages, animate = false, index = 0, dimmed = false, childBlocks, isStreaming }: ContentBlockProps): React.ReactElement | null {
  // text 块 — 主要内容，不受 dimmed 影响
  if (block.type === 'text') {
    const textBlock = block as SDKTextBlock
    if (!textBlock.text) return null
    return (
      <MessageResponse>{textBlock.text}</MessageResponse>
    )
  }

  // tool_use 块
  if (block.type === 'tool_use') {
    const toolBlock = block as SDKToolUseBlock
    return (
      <ToolUseBlock
        block={toolBlock}
        allMessages={allMessages}
        animate={animate}
        index={index}
        dimmed={dimmed}
        childBlocks={childBlocks}
        isStreaming={isStreaming}
      />
    )
  }

  // thinking 块
  if (block.type === 'thinking') {
    const thinkingBlock = block as SDKThinkingBlock
    if (!thinkingBlock.thinking) return null
    return <ThinkingBlock block={thinkingBlock} dimmed={dimmed} sessionId={null} />
  }

  return null
}
