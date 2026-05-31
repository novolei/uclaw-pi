/**
 * ThinkingBlock — collapsible "Thinking" reasoning block.
 *
 * Default-collapsed; a "Thinking" pill toggles a left-bordered 13px markdown
 * body (matching the ChatToolBlock expand panel). Markdown supports inline
 * file-path chips via `markdownFileChipPlugin` + `useFileChipResolver`.
 *
 * Lives in `shared/tool-rendering` (not `features/agent`) because it is consumed
 * by BOTH the agent core (ContentBlock / AgentMessages) and the chat domain's
 * NativeBlockRenderer — per the code-organization discipline, cross-domain
 * rendering sinks down to `shared/`. Resolving this placement removes the prior
 * back-edge where the chat NativeBlockRenderer reached up into the agent core
 * (it used to import ThinkingBlock from the agent ContentBlock module).
 *
 * It depends only on neutral preview-chip helpers (`@/components/preview/chips/*`)
 * — no agent atom/bridge — so it sits cleanly in shared.
 */

import * as React from 'react'
import type { ComponentProps } from 'react'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { Brain, ChevronRight } from 'lucide-react'
import { markdownFileChipPlugin } from '@/components/preview/chips/markdownFileChipPlugin'
import { FilePathChip } from '@/components/preview/chips/FilePathChip'
import { useFileChipResolver } from '@/components/preview/chips/useFileChipResolver'
import { cn } from '@/lib/utils'

/** Minimal structural shape of a thinking content block (SDK types removed). */
interface ThinkingContentBlock {
  type: 'thinking'
  thinking: string
}

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

export interface ThinkingBlockProps {
  block: ThinkingContentBlock
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
