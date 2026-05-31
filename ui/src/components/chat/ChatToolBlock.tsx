/**
 * ChatToolBlock — Chat 模式工具调用块（紧凑列表行样式）
 *
 *   🔧 bash ls -a
 *   🔧 bash rm -rf …
 *   ⟳  bash long-running …
 *   ⚠  bash failing-cmd
 *
 * 状态以工具图标 + 行色变化体现，不再使用大圆点。
 */

import * as React from 'react'
import { ChevronRight, AlertTriangle, Loader2, Check } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  getToolIcon,
  getToolPhrase,
  ToolResultRenderer,
  BashStreamView,
} from '@/shared/tool-rendering'
import type { LiveOutput } from '@/atoms/agent-atoms'

export interface ChatToolBlockProps {
  toolName: string
  input: Record<string, unknown>
  result?: string
  isError?: boolean
  isCompleted: boolean
  animate?: boolean
  index?: number
  /** 实时流式输出 — 仅 bash 工具运行期间携带，完成后不传 */
  liveOutput?: LiveOutput
}

export function ChatToolBlock({
  toolName,
  input,
  result,
  isError = false,
  isCompleted,
  animate = false,
  index = 0,
  liveOutput,
}: ChatToolBlockProps): React.ReactElement {
  const [expanded, setExpanded] = React.useState(false)

  const phrase = getToolPhrase(toolName, input)
  const ToolIcon = getToolIcon(toolName)
  const displayLabel = isCompleted ? phrase.label : phrase.loadingLabel

  const delay = animate && index < 10 ? `${index * 30}ms` : '0ms'
  const canExpand = !!result

  return (
    <div
      className={cn(
        'relative',
        animate && 'animate-in fade-in slide-in-from-left-1 duration-200 fill-mode-both',
      )}
      style={animate ? { animationDelay: delay } : undefined}
    >
      <button
        type="button"
        disabled={!canExpand}
        onClick={() => canExpand && setExpanded((v) => !v)}
        className={cn(
          'group relative flex w-full items-center gap-1.5 rounded-md py-0.5 pl-1.5 pr-1.5 text-left',
          'transition-colors duration-150',
          // 出错的整行轻微着染，让失败一眼可见
          isCompleted && isError && 'bg-destructive/[0.04] hover:bg-destructive/[0.07]',
          canExpand
            ? !(isCompleted && isError) && 'hover:bg-muted/40'
            : 'cursor-default',
          canExpand ? 'cursor-pointer' : '',
        )}
      >
        {/* 状态指示：成功 check / 运行中 spinner / 出错 warning
            尺寸 size-3 与 ThinkingBlock 头部 Brain 图标对齐 */}
        {!isCompleted ? (
          <Loader2 className="size-3 shrink-0 animate-spin text-primary/70" />
        ) : isError ? (
          <AlertTriangle className="size-3 shrink-0 text-destructive" />
        ) : (
          <Check className="size-3 shrink-0 text-emerald-500/80 dark:text-emerald-400/80" strokeWidth={2.5} />
        )}

        {/* 工具图标 — 与 thinking 头部保持一致的尺寸和 muted 调性 */}
        <ToolIcon
          className={cn(
            'size-3 shrink-0',
            isError
              ? 'text-destructive/70'
              : 'text-muted-foreground/60 group-hover:text-muted-foreground transition-colors',
          )}
        />

        {/* 命令/工具描述 — 字号、行高、配色与 ThinkingBlock 展开正文保持一致
            (text-[13px] leading-relaxed text-foreground/75 + 细微的弱化对比) */}
        <span
          className={cn(
            'truncate text-[13px] leading-relaxed',
            isError
              ? 'text-destructive font-medium'
              : 'text-foreground/75 group-hover:text-foreground/90 transition-colors',
            !isCompleted && !isError && 'text-foreground/85',
          )}
        >
          {displayLabel}
        </span>

        {canExpand && (
          <ChevronRight
            className={cn(
              // 与 thinking 头部 chevron 一致：size-3，muted-foreground/40，rotate on expanded
              'ml-auto shrink-0 size-3 text-muted-foreground/40',
              'transition-all duration-200',
              'opacity-0 group-hover:opacity-100',
              expanded && 'rotate-90 opacity-100',
            )}
          />
        )}
      </button>

      {/* 展开的结果面板 — 保持轻量的左边框风格（用户偏好），
          字号 / 行高 / 颜色仍与 ThinkingBlock 展开正文一致 */}
      {expanded && result && (
        <div
          className={cn(
            'ml-[22px] mr-2 mt-1 mb-2 pl-3 pr-1 py-1.5',
            'border-l',
            isError ? 'border-destructive/40' : 'border-border/50 dark:border-border/60',
            'text-[13px] leading-relaxed text-foreground/75',
            'animate-in fade-in slide-in-from-top-1 duration-150',
          )}
        >
          <ToolResultRenderer
            toolName={toolName}
            input={input}
            result={result}
            isError={isError}
          />
        </div>
      )}

      {/* 实时流式输出面板 — 仅 bash 工具运行期间且有内容时显示 */}
      {!isCompleted && toolName === 'bash' && liveOutput && liveOutput.segments.length > 0 && (
        <div className="ml-[22px] mr-2 mt-1 mb-2 animate-in fade-in duration-150">
          <BashStreamView
            command={String(input.command ?? '')}
            live={liveOutput}
            logPath={undefined}
          />
        </div>
      )}
    </div>
  )
}
