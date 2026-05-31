/**
 * Local content-block structural types for the agent ContentBlock dispatcher and
 * its per-type sub-renderers. The Claude Code SDK types were removed; these
 * structural equivalents are sufficient for rendering.
 *
 * Shared by `ContentBlock.tsx` (dispatcher) and `content-blocks/ToolUseBlock.tsx`.
 */

export interface SDKContentBlock {
  type: string
  text?: string
  id?: string
  name?: string
  input?: Record<string, any>
  thinking?: string
  tool_use_id?: string
  content?: string | SDKContentBlock[]
  is_error?: boolean
  [key: string]: unknown
}

export type SDKMessage = {
  type: string
  uuid?: string
  message?: { content?: SDKContentBlock[] | string }
  subtype?: string
  tool_use_id?: string
  usage?: { duration_ms?: number; total_tokens?: number; tool_uses?: number }
  [key: string]: unknown
}

export type SDKTextBlock = SDKContentBlock & { type: 'text'; text: string }
export type SDKToolUseBlock = SDKContentBlock & {
  type: 'tool_use'
  id: string
  name: string
  input: Record<string, any>
}
export type SDKThinkingBlock = SDKContentBlock & { type: 'thinking'; thinking: string }
export type SDKUserMessage = SDKMessage & { type: 'user' }
export type SDKToolResultBlock = SDKContentBlock & {
  type: 'tool_result'
  tool_use_id: string
  content?: string | SDKContentBlock[]
  is_error?: boolean
}
export type SDKSystemMessage = SDKMessage & {
  type: 'system'
  subtype?: string
  tool_use_id?: string
}
