/**
 * AgentMessages — Agent 消息列表
 *
 * 复用 Chat 的 Conversation/Message 原语组件渲染持久化消息与流式气泡。
 *
 * features/agent migration split (was 1276 lines): the per-message renderer
 * (AgentMessageItem), the token/cost/duration badge (MessageMetaBar +
 * DurationBadge), the streaming bubble (StreamingMessageBubble), the streaming
 * indicators, the compaction-fold card, and the attachment-image pieces moved to
 * sibling files under `message-list/`; the pure helpers to `lib/`; and the
 * list-shell side-effects/derivations to `hooks/useMessageListState`. This file
 * is now the Conversation shell + the render orchestration only. Behavior is
 * unchanged — same JSX, same conditions, same scroll/streaming wiring.
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { WelcomeEmptyState } from '@/components/welcome/WelcomeEmptyState'
import {
  Conversation,
  ConversationContent,
  ConversationScrollButton,
} from '@/components/ai-elements/conversation'
import { ScrollMinimap } from '@/components/ai-elements/scroll-minimap'
import { StickyUserMessage } from '@/components/ai-elements/sticky-user-message'
import { proactiveLearningEventsAtom, memoryRecallEventAtom, skillRecallsMapAtom } from '@/atoms/agent-atoms'
import { agentDisplayNameForAtom } from '@/atoms/agent-display-name'
import { stickyUserMessageEnabledAtom } from '@/atoms/ui-preferences'
import { ProactiveLearningChip } from '@/components/chat/ProactiveLearningChip'
import { MemoryRecallChip } from '@/components/chat/MemoryRecallChip'
import { SkillRecallChips } from './SkillRecallChips'
import { ScrollPositionManager } from '@/hooks/useScrollPositionMemory'
import { CompactingIndicator, CompactBoundaryDivider } from './CompactionIndicators'
import type { AgentMessage } from '@/lib/agent-types'
import type { AgentStreamState } from '@/atoms/agent-atoms'
import { AgentMessageItem } from './message-list/AgentMessageItem'
import { StreamingMessageBubble } from './message-list/StreamingMessageBubble'
import { useMessageListState } from '../hooks/useMessageListState'

/** AgentMessages 属性接口 */
interface AgentMessagesProps {
  sessionId: string
  /** 用户在前端选择的模型 ID（用于显示渠道配置的 Model Name） */
  sessionModelId?: string
  messages: AgentMessage[]
  /** 消息是否已完成首次加载 */
  messagesLoaded?: boolean
  streaming: boolean
  streamState?: AgentStreamState
  /** 实时消息列表（流式期间累积，用于过渡状态判断） */
  liveMessages?: any[]
  /** 当前会话工作目录，用于解析相对文件路径 */
  sessionPath?: string | null
  /** 附加目录列表（与 sessionPath 一并用作相对路径解析候选） */
  attachedDirs?: string[]
  /** 最后一轮是否被用户中断 */
  stoppedByUser?: boolean
  onRetry?: () => void
  onRetryInNewSession?: () => void
  onFork?: (upToMessageUuid: string) => void
  onRewind?: (assistantMessageUuid: string) => void
  onCompact?: () => void
}

/** 空状态引导 — 使用 WelcomeEmptyState */
function EmptyState(): React.ReactElement {
  return <WelcomeEmptyState />
}

export function AgentMessages({ sessionId, sessionModelId, messages, messagesLoaded, streaming, streamState, liveMessages, sessionPath, attachedDirs, stoppedByUser, onRetry, onRetryInNewSession, onFork, onRewind, onCompact }: AgentMessagesProps): React.ReactElement {
  const proactiveLearningEvents = useAtomValue(proactiveLearningEventsAtom)
  const memoryRecallMap = useAtomValue(memoryRecallEventAtom)
  const skillRecallsMap = useAtomValue(skillRecallsMapAtom)
  const stickyUserMessageEnabled = useAtomValue(stickyUserMessageEnabledAtom)
  const agentNameLookupOuter = useAtomValue(agentDisplayNameForAtom)

  const {
    ready,
    smoothContent,
    transitioning,
    hasContent,
    suppressAgentRunning,
    minimapItems,
    displayItems,
    allUserMessagesData,
    streamingModelId,
    agentStreamingModel,
    retrying,
    startedAt,
  } = useMessageListState({ sessionId, sessionModelId, messages, messagesLoaded, streaming, streamState, liveMessages })

  return (
    <Conversation resize={ready && !transitioning ? 'smooth' : 'instant'} className={ready ? 'opacity-100 transition-opacity duration-200' : 'opacity-0'}>
      <ScrollPositionManager id={sessionId} ready={ready} />
      <ConversationContent>
        {!hasContent && !streaming ? (
          <EmptyState />
        ) : (
          <>
            {/* 消息渲染 — AgentMessageItem + compact_boundary 分隔线 */}
            {displayItems.map((item) => {
              if (item.kind === 'boundary') {
                return (
                  <div key={item.boundary.uuid} data-live-marker="compact-boundary">
                    <CompactBoundaryDivider removed={item.boundary.removed} remaining={item.boundary.remaining} />
                  </div>
                )
              }
              const msg = item.message
              return (
                <div key={msg.id} data-message-id={msg.id} data-message-role={msg.role === 'user' ? 'user' : undefined}>
                  <AgentMessageItem
                    message={msg}
                    sessionPath={sessionPath}
                    attachedDirs={attachedDirs}
                    sessionId={sessionId}
                    sessionModelId={sessionModelId}
                  />
                </div>
              )
            })}

            {/* 流式气泡（含头像/名称/时间） */}
            {!suppressAgentRunning && (streaming || smoothContent || retrying || (streamState?.toolActivities?.length ?? 0) > 0 || streamState?.reasoning) && (
              <StreamingMessageBubble
                sessionId={sessionId}
                agentName={agentNameLookupOuter(sessionId)}
                agentStreamingModel={agentStreamingModel}
                streamingModelId={streamingModelId}
                streaming={streaming}
                streamState={streamState}
                smoothContent={smoothContent}
                retrying={retrying}
                startedAt={startedAt}
              />
            )}

            {/* 压缩中指示器：由 isCompacting flag 驱动 */}
            {streamState?.isCompacting && <CompactingIndicator />}

            {/* liveMessages 尾部渲染：handleCompact 注入的 /compact 用户气泡 +
                compacting 压缩中指示器（compact_boundary 已提升到 displayItems
                按时间戳插入消息列表正确位置，不在此处渲染）。 */}
            {liveMessages?.map((item: any) => {
              const key = item.uuid ?? `live-${item._createdAt}`
              // compact_boundary is now interleaved into displayItems above — skip here
              if (item.type === 'system' && item.subtype === 'compact_boundary') {
                return null
              }
              if (item.type === 'system' && item.subtype === 'compacting') {
                // Bundle 14-A — dedupe with the streamState-driven indicator
                // at line ~1173. Both fire concurrently when handleCompact
                // sets `isCompacting: true` AND inserts a `compacting`
                // marker into liveMessages, producing TWO pills. The
                // streamState-driven indicator is the primary (it tracks
                // backend state); the liveMessages marker is the
                // optimistic-fallback for the React batch race the
                // comment in handleCompact mentions. So: only render the
                // liveMessages version when streamState.isCompacting is
                // NOT already true. The two signals are now mutually
                // exclusive, never both visible at once.
                if (streamState?.isCompacting) return null
                return <div key={key} data-live-marker="compacting"><CompactingIndicator /></div>
              }
              if (item.type === 'user') {
                const text = item.message?.content?.[0]?.text ?? ''
                if (!text) return null
                return (
                  <div key={key} data-live-marker="user-synthetic" className="flex justify-end my-2 px-1">
                    <span className="text-[12px] text-muted-foreground px-3 py-1 rounded-full border border-border/40 bg-muted/30">
                      {text}
                    </span>
                  </div>
                )
              }
              return null
            })}

          </>
        )}
      </ConversationContent>
      {/* 统一徽章行：技能召回 + 主动学习 + 记忆召回
          — 按 sessionId 隔离；null sessionId/conversationId 事件 fallback 到当前 session（启动边缘情况 / Debug 测试用） */}
      {(() => {
        const skillRecalls = skillRecallsMap.get(sessionId) ?? []
        const sessionProactiveEvents = proactiveLearningEvents.filter(
          (ev) => ev.sessionId === sessionId || ev.sessionId == null
        )
        const memoryRecallEvent = memoryRecallMap.get(sessionId) ?? memoryRecallMap.get('__global__') ?? null

        const hasAny =
          skillRecalls.length > 0 ||
          sessionProactiveEvents.length > 0 ||
          (memoryRecallEvent != null && memoryRecallEvent.totalCandidates > 0)

        if (!hasAny) return null

        return (
          <div className="flex flex-wrap items-center gap-1.5 pl-[46px] pr-4 pb-2 mt-2">
            <SkillRecallChips sessionId={sessionId} className="mt-0 pl-0" />
            {sessionProactiveEvents.slice(0, 3).map((ev) => (
              <ProactiveLearningChip key={ev.timestamp} event={ev} />
            ))}
            {memoryRecallEvent != null && memoryRecallEvent.totalCandidates > 0 && (
              <MemoryRecallChip event={memoryRecallEvent} inline />
            )}
          </div>
        )
      })()}
      <ScrollMinimap items={minimapItems} />
      <ConversationScrollButton />
      {stickyUserMessageEnabled && allUserMessagesData.length > 0 && (
        <StickyUserMessage userMessages={allUserMessagesData} />
      )}
    </Conversation>
  )
}
