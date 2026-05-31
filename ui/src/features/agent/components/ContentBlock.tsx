/**
 * ContentBlock — 单个 SDKAssistantMessage 内容块渲染（dispatcher）
 *
 * 支持三种内容块类型，按 type 分发到各自的子渲染器：
 * - text: 通过 MessageResponse 渲染 Markdown（内联，无副渲染器）
 * - tool_use: → `content-blocks/ToolUseBlock`（语义化短语行 + 展开结构化结果）
 * - thinking: → `@/shared/tool-rendering` 的 ThinkingBlock（默认折叠，Thinking 标签）
 *
 * features/agent migration split (was 609 lines): the substantial tool_use
 * renderer + its result/usage hooks moved to `content-blocks/ToolUseBlock.tsx`;
 * the structural SDK types moved to `content-blocks/types.ts`; ThinkingBlock
 * sank down to `shared/tool-rendering` (it is also used by the chat domain's
 * NativeBlockRenderer). This file is now just the thin type-dispatcher.
 */

import * as React from 'react'
import { MessageResponse } from '@/components/ai-elements/message'
import { ThinkingBlock } from '@/shared/tool-rendering'
import { ToolUseBlock } from './content-blocks/ToolUseBlock'
import type {
  SDKContentBlock,
  SDKMessage,
  SDKTextBlock,
  SDKToolUseBlock,
  SDKThinkingBlock,
} from './content-blocks/types'

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

// ===== ContentBlock 主组件（dispatcher） =====

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
