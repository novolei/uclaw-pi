/**
 * NativeBlockRenderer — renders a Vec<ContentBlock> in original order.
 *
 * Pairing rule: each `tool_use` looks ahead in the same array for its
 * matching `tool_result` (by id ↔ tool_use_id) and renders a single
 * <ChatToolBlock>. Already-paired `tool_result`s are skipped on their
 * own iteration. Orphaned tool_results (no prior tool_use) get a
 * minimal placeholder so we don't silently drop persisted content.
 */

import * as React from 'react'
import type { ContentBlock } from '@/lib/chat-types'
import { ThinkingBlock } from './ThinkingBlock'
import { ChatToolBlock } from '@/components/chat/ChatToolBlock'
import { MessageResponse } from '@/components/ai-elements/message'

export interface NativeBlockRendererProps {
  blocks: ContentBlock[]
  /** Reserved for future use (e.g. mention links / per-conversation context). */
  conversationId?: string
  /** Optional className for the outer wrapper. */
  className?: string
}

export function NativeBlockRenderer({
  blocks,
  className,
  conversationId,
}: NativeBlockRendererProps): React.ReactElement {
  // Pre-compute a tool_use_id → tool_result lookup so each tool_use can
  // grab its result in O(1). Walk the array in order so we can also build
  // the "already paired" set in one pass.
  const { resultMap, pairedResults } = React.useMemo(() => {
    const map = new Map<string, Extract<ContentBlock, { type: 'tool_result' }>>()
    const paired = new Set<string>()
    for (const b of blocks) {
      if (b.type === 'tool_result') map.set(b.tool_use_id, b)
    }
    for (const b of blocks) {
      if (b.type === 'tool_use' && map.has(b.id)) paired.add(b.id)
    }
    return { resultMap: map, pairedResults: paired }
  }, [blocks])

  return (
    <div className={className} data-native-blocks="true">
      {blocks.map((b, idx) => {
        if (b.type === 'text') {
          return (
            <MessageResponse key={`b-${idx}-text`} sessionId={conversationId ?? null}>{b.text}</MessageResponse>
          )
        }
        if (b.type === 'thinking') {
          return (
            <ThinkingBlock
              key={`b-${idx}-thinking`}
              block={{ type: 'thinking', thinking: b.thinking }}
              sessionId={conversationId ?? null}
            />
          )
        }
        if (b.type === 'tool_use') {
          const result = resultMap.get(b.id)
          const isCompleted = result !== undefined
          return (
            <ChatToolBlock
              key={`b-${idx}-tool-${b.id}`}
              toolName={b.name}
              input={b.input}
              result={result?.content}
              isError={result?.is_error}
              isCompleted={isCompleted}
            />
          )
        }
        if (b.type === 'tool_result') {
          // Skip if paired with a prior tool_use.
          if (pairedResults.has(b.tool_use_id)) return null
          // Orphan — render a minimal placeholder so the content isn't dropped.
          return (
            <div
              key={`b-${idx}-orphan-${b.tool_use_id}`}
              className="my-2 rounded border border-dashed border-border/50 bg-muted/30 px-2.5 py-1.5 text-[12px] text-muted-foreground/75"
              title={`tool_use_id: ${b.tool_use_id}`}
            >
              <span className="font-mono text-[11px]">tool result (orphaned)</span>
              <pre className="mt-1 whitespace-pre-wrap font-mono text-[11.5px] text-foreground/75">
                {b.content}
              </pre>
            </div>
          )
        }
        return null
      })}
    </div>
  )
}
