/**
 * useMessageListState — the message-list side-effect & derivation engine for
 * AgentMessages. Extracted from the AgentMessages component body during the
 * features/agent migration split so the component stays presentational.
 *
 * Owns (unchanged from the original, same order, same deps):
 *  - visibleMessages filtering
 *  - the fade-in `ready` gate (session-switch reset + RAF reveal)
 *  - the streaming smoothContent derivation (+ the anti-flicker guard)
 *  - the streaming→persisted transition (instant resize cooldown)
 *  - minimapItems + the Tab-level minimap-cache sync effect
 *  - displayItems (compact_boundary interleave by timestamp)
 *  - allUserMessagesData (for StickyUserMessage)
 *  - the resolved streaming model/name/cursor derivations
 *
 * This is a behavior-preserving move: every useState/useEffect/useMemo is the
 * same as before, just relocated. The component reads the returned values.
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import type { MinimapItem } from '@/components/ai-elements/scroll-minimap'
import { useSmoothStream } from '@/lib/proma-ui'
import { resolveModelDisplayName } from '@/lib/model-logo'
import { userProfileAtom } from '@/atoms/user-profile'
import { tabMinimapCacheAtom } from '@/atoms/tab-atoms'
import { channelsAtom } from '@/atoms/chat-atoms'
import type { AgentMessage } from '@/lib/agent-types'
import type { AgentStreamState } from '@/atoms/agent-atoms'
import { parseAttachedFiles, isImageFile } from '../lib/agent-message-helpers'

/** Discriminated render item: a persisted message or an interleaved boundary. */
export type DisplayItem =
  | { kind: 'message'; message: AgentMessage }
  | { kind: 'boundary'; boundary: any }

export interface MessageListStateArgs {
  sessionId: string
  sessionModelId?: string
  messages: AgentMessage[]
  messagesLoaded?: boolean
  streaming: boolean
  streamState?: AgentStreamState
  liveMessages?: any[]
}

export function useMessageListState({
  sessionId,
  sessionModelId,
  messages,
  messagesLoaded,
  streaming,
  streamState,
  liveMessages,
}: MessageListStateArgs) {
  const userProfile = useAtomValue(userProfileAtom)
  const setMinimapCache = useSetAtom(tabMinimapCacheAtom)
  const channels = useAtomValue(channelsAtom)

  const visibleMessages = React.useMemo(() => {
    return messages.filter((m) => {
      if (m.role === 'user') {
        const { files, text } = parseAttachedFiles(m.content ?? '')
        return files.length > 0 || !!text.trim()
      }
      return true
    })
  }, [messages])
  /** 淡入控制：切换会话时先隐藏，等布局完成后再显示。 */
  const [ready, setReady] = React.useState(false)
  const prevSessionIdRef = React.useRef<string | null>(null)

  React.useEffect(() => {
    if (sessionId !== prevSessionIdRef.current) {
      prevSessionIdRef.current = sessionId
      setReady(false)
    }
  }, [sessionId])

  React.useEffect(() => {
    if (ready) return

    // 必须等消息加载完成，否则 messages=[] 会被误判为空对话
    if (messagesLoaded === false) return

    // 流式进行中且有实时内容 → 跳过 fade 直接显示
    if (streaming && liveMessages && liveMessages.length > 0) {
      setReady(true)
      return
    }

    if (visibleMessages.length === 0 && !streaming) {
      setReady(true)
      return
    }
    let cancelled = false
    requestAnimationFrame(() => {
      if (!cancelled) setReady(true)
    })
    return () => { cancelled = true }
  }, [visibleMessages, streaming, liveMessages, messagesLoaded])

  // 从 streamState 属性中计算派生值
  const streamingContent = streamState?.content ?? ''
  // streamState.model 在 SDK 首事件前未填充；此时回退到会话当前选用模型，避免 caption 空缺 + Bot 默认图标
  const streamingModelId = streamState?.model ?? sessionModelId
  const agentStreamingModel = streamingModelId ? resolveModelDisplayName(streamingModelId, channels) : undefined
  const retrying = streamState?.retrying
  const startedAt = streamState?.startedAt

  const { displayedContent: rawSmoothContent } = useSmoothStream({
    content: streamingContent,
    isStreaming: streaming,
  })

  // 防闪屏守卫：useSmoothStream 通过 useEffect 重置 displayedContent，比 render 晚一帧。
  // 当 streamingContent 已清空但 smoothContent 仍持有旧值时，
  // 会导致 fallback 气泡与持久化消息同时渲染一帧（重复内容闪烁）。
  // 用原始 streamingContent 作为守卫：内容已清空且不在流式中，立即归零。
  const smoothContent = (streaming || streamingContent) ? rawSmoothContent : ''

  /**
   * 流式完成过渡：streaming 结束到持久化消息加载完成之间，
   * 强制 resize="instant" 避免中间高度变化触发平滑滚动动画。
   *
   * 使用 render-phase 计算避免 useEffect 延迟一帧的问题：
   * - streaming 变 false 的第一帧就能立即切到 instant，防止闪动
   * - 后续通过 ref+timeout 延迟 150ms 才允许切回 smooth
   */
  const [transitioningCooldown, setTransitioningCooldown] = React.useState(false)
  const wasStreamingRef = React.useRef(streaming)

  // render-phase 判断：是否处于需要 instant resize 的过渡期
  // liveMessages 非空说明持久化消息还没加载完（加载完后会清空 liveMessages）
  const needsInstant = !streaming && (!!streamingContent || !!smoothContent || (liveMessages != null && liveMessages.length > 0))

  React.useEffect(() => {
    // 刚从 streaming → not-streaming：启动 cooldown
    if (wasStreamingRef.current && !streaming) {
      setTransitioningCooldown(true)
    }
    wasStreamingRef.current = streaming
  }, [streaming])

  React.useEffect(() => {
    if (needsInstant) return
    // 过渡完成后延迟 150ms 才关闭 cooldown，给 StickToBottom 时间稳定
    const timer = setTimeout(() => setTransitioningCooldown(false), 150)
    return () => clearTimeout(timer)
  }, [needsInstant])

  const transitioning = needsInstant || transitioningCooldown

  const hasContent = visibleMessages.length > 0

  // 压缩流程进行中（含收尾窗口：compact_boundary 已到但 result 未到）
  // → 一律抑制 AgentRunningIndicator，避免压缩分隔符切换期间闪烁。
  // compactInFlight 从点击压缩 / SDK compacting 事件开始为 true，
  // 直到整个 stream 结束（stream state 被删除）才消失。
  const suppressAgentRunning = streamState?.isCompacting || streamState?.compactInFlight

  // 迷你地图数据 — 跳过 compacted 消息以减少噪音
  // model 字段持久化前的历史 assistant 消息（DB 未填 model 列）回退到会话当前模型，
  // 否则 ItemIcon 拿不到 model 就不渲染 logo。
  const minimapItems: MinimapItem[] = React.useMemo(
    () => {
      return visibleMessages
        .filter((m) => !m.compacted)
        .map((m, i) => ({
        id: m.id || `msg-${i}`,
        role: m.role === 'status' ? 'status' as const : m.role as MinimapItem['role'],
        preview: (m.content ?? '').replace(/<attached_files>[\s\S]*?<\/attached_files>\n*/, '').slice(0, 200),
        avatar: m.role === 'user' ? userProfile.avatar : undefined,
        model: m.role === 'assistant' ? (m.model ?? sessionModelId) : m.model,
      }))
    },
    [visibleMessages, userProfile.avatar, sessionModelId]
  )

  // 将 liveMessages 中的 compact_boundary 按时间戳插入到消息列表的正确位置，
  // 使其随旧消息自然向上滚动，而不是永远粘在底部。
  const displayItems = React.useMemo<DisplayItem[]>(() => {
    const boundaries = (liveMessages ?? []).filter(
      (item: any) => item.type === 'system' && item.subtype === 'compact_boundary'
    )
    if (boundaries.length === 0) {
      return visibleMessages.map(m => ({ kind: 'message' as const, message: m }))
    }

    const items: DisplayItem[] = [
      ...visibleMessages.map(m => ({ kind: 'message' as const, message: m })),
      ...boundaries.map(b => ({ kind: 'boundary' as const, boundary: b })),
    ]

    items.sort((a, b) => {
      const aTime = a.kind === 'message' ? a.message.createdAt : a.boundary._createdAt
      const bTime = b.kind === 'message' ? b.message.createdAt : b.boundary._createdAt
      return aTime - bTime
    })

    return items
  }, [visibleMessages, liveMessages])

  // 同步 minimap 缓存到 Tab 级别（供 Tab hover 预览使用）
  React.useEffect(() => {
    if (minimapItems.length > 0) {
      setMinimapCache((prev) => {
        const next = new Map(prev)
        next.set(sessionId, minimapItems.map(item => ({ ...item, avatar: item.avatar ?? undefined })))
        return next
      })
    }
  }, [sessionId, minimapItems, setMinimapCache])

  // 所有用户消息的数据 — 供 StickyUserMessage 使用
  const allUserMessagesData = React.useMemo(() => {
    return visibleMessages
      .filter((m) => m.role === 'user')
      .map((m) => {
        const { files, text } = parseAttachedFiles(m.content ?? '')
        return {
          id: m.id ?? null,
          text,
          attachments: files.map((f) => ({ filename: f.filename, isImage: isImageFile(f.filename) })),
        }
      })
  }, [visibleMessages])

  return {
    visibleMessages,
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
  }
}
