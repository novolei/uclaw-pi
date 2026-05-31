/**
 * StreamingMessageBubble — the live assistant bubble shown during a turn
 * (header + retry notice + thinking block + tool activity + streamed markdown +
 * the AgentRunningIndicator cursor + the post-stream token/cost meta bar +
 * truncation notice).
 *
 * Extracted verbatim from the AgentMessages render body during the
 * features/agent migration split. The gating logic that decides WHETHER to
 * render this bubble stays in AgentMessages; this component renders the bubble
 * once that gate passes. Behavior is unchanged — same JSX, same conditions.
 */

import * as React from 'react'
import { AlertTriangle } from 'lucide-react'
import {
  Message,
  MessageHeader,
  MessageContent,
  MessageActions,
  MessageResponse,
} from '@/components/ai-elements/message'
import { formatMessageTime } from '@/components/chat/ChatMessageItem'
import { ThinkingBlock } from '@/shared/tool-rendering'
import { ChatToolActivityIndicator } from '@/components/chat/ChatToolActivityIndicator'
import { SkillCitationChips } from '../SkillCitationChips'
import { parseSkillCitations } from '@/lib/skill-citation'
import { normalizeAgentMarkdown } from '@/lib/normalize-agent-markdown'
import type { AgentStreamState } from '@/atoms/agent-atoms'
import { agentActivitiesToChatActivities } from '../../lib/agent-message-helpers'
import { AssistantLogo } from './AssistantLogo'
import { MessageMetaBar } from './MessageMetaBar'
import { RetryingNotice, AgentRunningIndicator } from './StreamingIndicators'

interface StreamingMessageBubbleProps {
  sessionId: string
  /** 流式渠道名（已 resolve），用于表头 model caption */
  agentStreamingModel?: string
  /** 流式模型 ID（含 sessionModelId 回退），用于表头 logo */
  streamingModelId?: string
  /** 解析后的 agent display name */
  agentName: string
  streaming: boolean
  streamState?: AgentStreamState
  /** 平滑流式文本（已过防闪屏守卫） */
  smoothContent: string
  /** streamState.retrying 的本地别名 */
  retrying?: AgentStreamState['retrying']
  /** streamState.startedAt 的本地别名 */
  startedAt?: number
}

export function StreamingMessageBubble({
  sessionId,
  agentStreamingModel,
  streamingModelId,
  agentName,
  streaming,
  streamState,
  smoothContent,
  retrying,
  startedAt,
}: StreamingMessageBubbleProps): React.ReactElement {
  return (
    <Message from="assistant">
      <MessageHeader
        name={agentName}
        model={agentStreamingModel}
        time={formatMessageTime(Date.now())}
        logo={<AssistantLogo model={streamingModelId} />}
      />
      <MessageContent>
        {retrying && <RetryingNotice retrying={retrying} />}
        {(streamState?.reasoning) && (
          <div className="mb-3">
            <ThinkingBlock block={{ type: 'thinking', thinking: streamState.reasoning } as any} dimmed={!!smoothContent} sessionId={sessionId ?? null} />
          </div>
        )}
        {(streamState?.toolActivities?.length ?? 0) > 0 && (
          <div className="mb-3">
            {/* 流式工具调用 — 转成 ChatToolActivity 后用 ChatToolActivityIndicator 渲染，
                视觉与历史消息保持一致（ChatToolBlock 的 🔧 toolName + 折叠结果卡片样式） */}
            <ChatToolActivityIndicator
              activities={agentActivitiesToChatActivities(streamState!.toolActivities)}
              isStreaming={streaming}
            />
          </div>
        )}
        {smoothContent ? (() => {
          const { cleanedContent: streamCleanedContent, citations: streamCitations } = parseSkillCitations(normalizeAgentMarkdown(smoothContent))
          return (
            <>
              <MessageResponse sessionId={sessionId ?? null}>{streamCleanedContent}</MessageResponse>
              {/* Once the citation block has fully streamed in, render
                  the chip(s) — the dedupe key uses the session id so
                  the streaming chip and the post-finalization chip
                  don't both bump cited_count. */}
              <SkillCitationChips
                citations={streamCitations}
                messageKey={`stream-${sessionId}`}
              />
              {streaming && (
                <AgentRunningIndicator
                  startedAt={startedAt}
                  toolCount={streamState?.toolActivities?.length}
                  inputTokens={streamState?.inputTokens}
                  outputTokens={streamState?.outputTokens}
                />
              )}
            </>
          )
        })() : (
          streaming && (
            <AgentRunningIndicator
              startedAt={startedAt}
              toolCount={streamState?.toolActivities?.length}
              inputTokens={streamState?.inputTokens}
              outputTokens={streamState?.outputTokens}
            />
          )
        )}
      </MessageContent>
      {/* 流式完成后显示 token 用量 */}
      {!streaming && smoothContent && streamState?.inputTokens != null && (
        <MessageActions className="pl-[46px] mt-0.5 justify-start gap-2.5">
          <MessageMetaBar usage={{
            inputTokens: streamState.inputTokens,
            outputTokens: streamState.outputTokens,
            costUsd: streamState.costUsd,
          }} />
        </MessageActions>
      )}
      {/* 截断提示：任一 LLM 调用被 token 限制截断 */}
      {!streaming && streamState?.truncated && (
        <div className="pl-[46px] mt-1 flex items-center gap-1.5 text-[12px] text-amber-500/80">
          <AlertTriangle className="size-3 shrink-0" />
          <span>部分内容因 token 限制被截断，Agent 已自动继续输出</span>
        </div>
      )}
    </Message>
  )
}
