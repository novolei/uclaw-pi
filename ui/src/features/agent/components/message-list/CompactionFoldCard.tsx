/**
 * CompactionFoldCard — renders the M2-G fold synth message (the
 * Markdown placeholder /compact inserts in place of the compacted
 * messages) as a collapsible card with a single-line header instead
 * of a regular user bubble.
 *
 * Replaces the prior "horizontal divider + a Markdown user bubble"
 * dual-rendering that the user reported as "two compactions". The
 * boundary marker + the fold content are now ONE card: header line
 * shows the divider-style summary, expanding shows the structured
 * Markdown body.
 *
 * Extracted from AgentMessages.tsx during the features/agent migration split.
 * Behavior unchanged.
 */

import * as React from 'react'
import { ChevronDown, ChevronRight, Brain } from 'lucide-react'
import { MessageResponse } from '@/components/ai-elements/message'
import { cn } from '@/lib/utils'
import { formatRelativeShort } from '../../lib/agent-message-helpers'

export function CompactionFoldCard({
  markdown,
  createdAt,
}: {
  markdown: string
  createdAt: number
}): React.ReactElement {
  const [expanded, setExpanded] = React.useState(false)

  // Quick stats — count `### ` headings in the markdown for the
  // collapsed summary. Each section corresponds to one of the 8
  // StructuredFold fields. Skipping empty body bodies returns 0.
  const sectionCount = React.useMemo(() => {
    const matches = markdown.match(/^###\s+/gm)
    return matches?.length ?? 0
  }, [markdown])

  // Strip the leading `## Earlier conversation (compacted)\n` header
  // since we render our own header in the card chrome.
  const body = React.useMemo(() => {
    return markdown.replace(/^##\s+Earlier conversation \(compacted\)\s*\n+/, '')
  }, [markdown])

  return (
    <div
      className={cn(
        'flex items-center gap-3 my-4 px-1',
        expanded && 'flex-col items-stretch',
      )}
    >
      {!expanded && (
        <>
          <div className="flex-1 h-px bg-border/40" />
          <button
            type="button"
            onClick={() => setExpanded(true)}
            className={cn(
              'shrink-0 inline-flex items-center gap-1.5',
              'rounded-full border border-border/40 bg-muted/30',
              'px-2.5 py-0.5 text-[11px] text-muted-foreground/75',
              'hover:bg-muted/50 hover:text-foreground transition-colors',
            )}
          >
            <Brain className="size-3" />
            <span>上下文已压缩为结构化摘要</span>
            {sectionCount > 0 && (
              <span className="text-[10px] text-muted-foreground/45">
                · {sectionCount} 个分项
              </span>
            )}
            <ChevronRight className="size-3 opacity-60" />
          </button>
          <div className="flex-1 h-px bg-border/40" />
        </>
      )}
      {expanded && (
        <div
          className={cn(
            'rounded-xl border border-border/50 bg-muted/20',
            'overflow-hidden',
          )}
        >
          <button
            type="button"
            onClick={() => setExpanded(false)}
            className={cn(
              'flex w-full items-center gap-2 px-3 py-2',
              'border-b border-border/40 bg-muted/30',
              'text-left text-[11px] text-muted-foreground/85',
              'hover:bg-muted/50 transition-colors',
            )}
          >
            <Brain className="size-3.5 text-emerald-500/85" />
            <span className="flex-1 font-medium">上下文摘要 (M2-G StructuredFold)</span>
            <span className="text-[10px] text-muted-foreground/55">
              {sectionCount} 个分项 · {formatRelativeShort(createdAt)}
            </span>
            <ChevronDown className="size-3.5 opacity-60" />
          </button>
          <div className="px-4 py-3 text-[12.5px] text-foreground/85">
            <MessageResponse sessionId={null}>{body}</MessageResponse>
          </div>
        </div>
      )}
    </div>
  )
}
