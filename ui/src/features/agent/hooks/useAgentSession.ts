/**
 * useAgentSession — the AgentView shell's session/event/streaming subscription
 * engine, extracted VERBATIM from the AgentView component body during the
 * features/agent migration split.
 *
 * Owns (unchanged from the original — same effects, same deps, same order):
 *  - the `messages` + `messagesLoaded` state
 *  - per-session channel/model map init from the global defaults
 *  - the auto-select-first-model effect (channel chosen but no model)
 *  - the session workspace-path lookup (file browser)
 *  - the `chat:stream-complete` refresh-bump listener
 *  - the `agent:queued-consumed` banner-dequeue listener
 *  - the initial message-load + context-estimate + streaming-state cleanup
 *  - the pending-prompt auto-send (quick-task / settings trigger)
 *
 * Behavior-preserving move: every useEffect / useState body is identical to the
 * original, only relocated. The shell passes the derived inputs in and reads the
 * returned `messages` / `setMessages` / `messagesLoaded`.
 *
 * IPC routes through the agent bridge (no `@tauri-apps/api` here).
 */

import * as React from 'react'
import { useAtomValue, useSetAtom, useStore } from 'jotai'
import type { Channel } from '@/lib/proma-types'
import type { AgentMessage, AgentSendInput } from '@/lib/agent-types'
import type { ActiveProviderModel } from '@/atoms/active-model'
import {
  agentStreamingStatesAtom,
  agentSessionChannelMapAtom,
  agentSessionModelMapAtom,
  agentSessionPathMapAtom,
  agentMessageRefreshAtom,
  agentPendingPromptAtom,
  liveMessagesMapAtom,
} from '@/atoms/agent-atoms'
import { agentQueuedMessagesMapAtom, removeQueuedMessage } from '@/atoms/agent-queue-messages'
import {
  getAgentSessionPath,
  getAgentSessionMessages,
  sendAgentMessage,
  estimateSessionContext,
  updateSettings,
  onStreamComplete,
  onStreamError,
  onQueuedConsumed,
} from '@/lib/bridge/agent'
import { allPendingAskUserRequestsAtom } from '@/atoms/agent-atoms'

export interface UseAgentSessionArgs {
  sessionId: string
  currentWorkspaceId: string | null
  defaultChannelId: string | null
  defaultModelId: string | null
  setDefaultModelId: (id: string) => void
  agentChannelId: string | null
  agentModelId: string | null
  globalChannels: Channel[]
  activeProviderModel: ActiveProviderModel | null
  streaming: boolean
}

export interface UseAgentSessionResult {
  messages: AgentMessage[]
  setMessages: React.Dispatch<React.SetStateAction<AgentMessage[]>>
  messagesLoaded: boolean
}

export function useAgentSession(args: UseAgentSessionArgs): UseAgentSessionResult {
  const {
    sessionId,
    currentWorkspaceId,
    defaultChannelId,
    defaultModelId,
    setDefaultModelId,
    agentChannelId,
    agentModelId,
    globalChannels,
    activeProviderModel,
    streaming,
  } = args

  const store = useStore()
  const [messages, setMessages] = React.useState<AgentMessage[]>([])
  const setStreamingStates = useSetAtom(agentStreamingStatesAtom)
  const sessionChannelMap = useAtomValue(agentSessionChannelMapAtom)
  const sessionModelMap = useAtomValue(agentSessionModelMapAtom)
  const setSessionChannelMap = useSetAtom(agentSessionChannelMapAtom)
  const setSessionModelMap = useSetAtom(agentSessionModelMapAtom)
  const setSessionPathMap = useSetAtom(agentSessionPathMapAtom)
  const [pendingPrompt, setPendingPrompt] = [
    useAtomValue(agentPendingPromptAtom),
    useSetAtom(agentPendingPromptAtom),
  ] as const

  // 已有会话首次打开时，从全局默认值初始化 per-session map
  React.useEffect(() => {
    if (!sessionId) return
    if (!sessionChannelMap.has(sessionId) && defaultChannelId) {
      setSessionChannelMap((prev) => {
        if (prev.has(sessionId)) return prev
        const map = new Map(prev)
        map.set(sessionId, defaultChannelId)
        return map
      })
    }
    if (!sessionModelMap.has(sessionId) && defaultModelId) {
      setSessionModelMap((prev) => {
        if (prev.has(sessionId)) return prev
        const map = new Map(prev)
        map.set(sessionId, defaultModelId)
        return map
      })
    }
  }, [sessionId, sessionChannelMap, sessionModelMap, defaultChannelId, defaultModelId, setSessionChannelMap, setSessionModelMap])

  // 渠道已选但模型未选时，自动选择第一个可用模型
  React.useEffect(() => {
    if (!agentChannelId || agentModelId) return

    const channel = globalChannels.find((c) => c.id === agentChannelId && c.enabled)
    if (!channel) return

    const firstModel = channel.models.find((m) => m.enabled)
    if (!firstModel) return

    // 更新 per-session map
    setSessionModelMap((prev) => {
      const map = new Map(prev)
      map.set(sessionId, firstModel.id)
      return map
    })
    // 同步全局默认值
    setDefaultModelId(firstModel.id)
    updateSettings({
      agentChannelId,
      agentModelId: firstModel.id,
    }).catch(console.error)
  }, [agentChannelId, agentModelId, globalChannels, sessionId, setSessionModelMap, setDefaultModelId])

  // 获取当前 session 的工作路径（文件浏览器需要）
  React.useEffect(() => {
    if (!currentWorkspaceId) {
      setSessionPathMap((prev) => {
        const map = new Map(prev)
        map.delete(sessionId)
        return map
      })
      return
    }

    getAgentSessionPath(currentWorkspaceId, sessionId)
      .then((path: string) => {
        if (path) {
          setSessionPathMap((prev) => {
            const map = new Map(prev)
            map.set(sessionId, path)
            return map
          })
        } else {
          setSessionPathMap((prev) => {
            const map = new Map(prev)
            map.delete(sessionId)
            return map
          })
        }
      })
      .catch(() => {
        setSessionPathMap((prev) => {
          const map = new Map(prev)
          map.delete(sessionId)
          return map
        })
      })
  }, [sessionId, currentWorkspaceId, setSessionPathMap])

  // 监听消息刷新版本号
  const refreshMap = useAtomValue(agentMessageRefreshAtom)
  const refreshVersion = refreshMap.get(sessionId) ?? 0

  // 当本会话的 chat:stream-complete 触发时刷新消息列表，
  // 确保 duration_ms / input_tokens 等在 agent_messages 写入后立即拉取。
  // sendAgentMessage().then() 的 reload 发生在 agent loop 开始时，此时 DB 尚未写入。
  // 注意：不能用 workingDoneSessionIdsAtom — sessionId 在集合中会永久留存，
  // 导致第二轮及之后的 prev.has(sid)=true，条件永远不成立。
  React.useEffect(() => {
    const unlisten = onStreamComplete((payload: { conversationId: string }) => {
      if (payload.conversationId !== sessionId) return
      store.set(agentMessageRefreshAtom, (m) => {
        const next = new Map(m)
        next.set(sessionId, (m.get(sessionId) ?? 0) + 1)
        return next
      })
    })
    return unlisten
  }, [sessionId, store])

  // 运行结束(完成 或 错误/中断)后清理本会话遗留的 ask_user banner。正常作答时
  // 请求已被 useAskUserBanner 提交流程移除,所以这里只在「提问过程中被 Stop/出错」
  // 的孤儿场景生效 —— 否则 AskUserBanner 会在中断后永远挂着。complete + error 两路
  // 都接,覆盖 abort(走 stream-error)。只动 ask_user map,不碰 streaming 态,无竞态。
  React.useEffect(() => {
    const clearOrphanAskUser = (payload: { conversationId: string }): void => {
      if (payload.conversationId !== sessionId) return
      store.set(allPendingAskUserRequestsAtom, (prev) => {
        if (!prev.has(sessionId)) return prev
        const next = new Map(prev)
        next.delete(sessionId)
        return next
      })
    }
    const unComplete = onStreamComplete(clearOrphanAskUser)
    const unError = onStreamError(clearOrphanAskUser)
    return () => {
      unComplete()
      unError()
    }
  }, [sessionId, store])

  // Pi Sprint 2 item ③ — remove a queued banner card when the backend confirms
  // it was actually consumed by the agent loop (agent:queued-consumed event).
  // Cards stay visible until this fires — no optimistic removal on enqueue.
  React.useEffect(() => {
    const unlisten = onQueuedConsumed(({ sessionId: sid, uuid }) => {
      if (sid !== sessionId) return
      store.set(agentQueuedMessagesMapAtom, (prev) => removeQueuedMessage(prev, sid, uuid))
    })
    return unlisten
  }, [sessionId, store])

  // 消息是否已完成首次加载（用于 auto-send 等待）
  const [messagesLoaded, setMessagesLoaded] = React.useState(false)

  // 加载当前会话消息
  React.useEffect(() => {
    // 流式运行中不重置 messagesLoaded，避免 streaming UI 消失后出现空窗闪烁
    const isCurrentlyStreaming = store.get(agentStreamingStatesAtom).get(sessionId)?.running ?? false
    if (!isCurrentlyStreaming) {
      setMessagesLoaded(false)
    }
    getAgentSessionMessages(sessionId)
      .then((msgs) => {
        setMessages(msgs)
        setMessagesLoaded(true)

        // ── Context initialization ──────────────────────────────────
        // After app restart or session switch, inputTokens/contextWindow
        // are undefined in the streaming state because Jotai atoms are
        // in-memory only. Request the backend to estimate context usage
        // from persisted messages so ContextUsageBadge renders immediately
        // instead of waiting for the next LLM round-trip.
        // Mirrors openhanako's context_usage WS request pattern.
        // (P0 fix: 2026-05-16)
        estimateSessionContext(sessionId).then((ctx) => {
          if (!ctx) return
          setStreamingStates((prev) => {
            const existing = prev.get(sessionId)
            // Only populate if not already set by a live turn_cost event.
            // If the user sent a message while we were fetching, the
            // turn_cost handler already set inputTokens — don't overwrite.
            if (existing?.inputTokens != null && existing.inputTokens > 0) return prev
            const map = new Map(prev)
            map.set(sessionId, {
              ...(existing ?? {
                running: false,
                content: '',
                toolActivities: [],
                teammates: [],
              }),
              inputTokens: ctx.inputTokens,
              contextWindow: ctx.contextWindow,
            })
            return map
          })
        }).catch((err) => {
          console.warn('[AgentView] estimateSessionContext failed:', err)
        })

        // 消息加载完成后，同步清除流式展示状态和实时消息，
        // 确保 React 在一次渲染中同时显示持久化消息并移除流式气泡/实时消息，
        // 避免「实时消息已清 → 持久化消息未到」的空档闪烁
        // 用 spread 保留全部 usage / context 字段（inputTokens, skillsTokens,
        // costUsd, contextWindow, …），只清除五个流式展示字段。之前用
        // 字段白名单导致 skillsTokens / costUsd 等一轮结束就丢失，
        // ContextUsageBadge 的"技能"行回轮后会消失。
        //
        // 必须包括 `reasoning` ——遗漏会导致 ThinkingBlock 一直存活，
        // 持久化消息加载后流式气泡只剩 ThinkingBlock 显示成空的
        // 「Assistant ... THINKING >」幽灵卡片（thinking 已写进 message.reasoning，
        // 由 AgentMessageItem 内联渲染，不需要在流式气泡里重复展示）。
        setStreamingStates((prev) => {
          const state = prev.get(sessionId)
          if (!state || state.running) return prev  // 仍在运行中，不清除
          const map = new Map(prev)
          if (state.inputTokens !== undefined) {
            map.set(sessionId, {
              ...state,
              running: false,
              content: '',
              reasoning: undefined,
              toolActivities: [],
              teammates: [],
            })
          } else {
            map.delete(sessionId)
          }
          return map
        })
        store.set(liveMessagesMapAtom, (prev) => {
          if (!prev.has(sessionId)) return prev
          // 仍在运行中，不清除实时消息（与 streamingStates 保护逻辑一致）
          const streamingState = store.get(agentStreamingStatesAtom).get(sessionId)
          if (streamingState?.running) return prev
          // 保留 compact_boundary 标记（"上下文已压缩"分隔符），
          // 其他瞬时合成消息正常清除。否则 chat:stream-complete 注入的
          // compact_boundary 会被 getAgentSessionMessages 回调立即清掉，
          // 用户永远看不到压缩完成的分隔线。
          const current = prev.get(sessionId) ?? []
          const boundaries = current.filter(
            (item: any) => item.type === 'system' && item.subtype === 'compact_boundary'
          )
          const map = new Map(prev)
          if (boundaries.length > 0) {
            map.set(sessionId, boundaries)
          } else {
            map.delete(sessionId)
          }
          return map
        })
      })
      .catch(console.error)
  }, [sessionId, refreshVersion, setStreamingStates, store])

  // 自动发送 pending prompt（从快速任务窗口或设置页触发）
  // 等待 messagesLoaded 确保消息加载完成后再插入乐观消息，避免被加载结果覆盖。
  // 使用 queueMicrotask 延迟发送：避免 setState → 重渲染 → cleanup 取消 timer 的竞态。
  React.useEffect(() => {
    if (!messagesLoaded) return
    if (!pendingPrompt) return
    if (pendingPrompt.sessionId !== sessionId) return
    if (!activeProviderModel || streaming) return

    // 快照当前上下文
    const snapshot = {
      message: pendingPrompt.message,
      channelId: agentChannelId ?? activeProviderModel.providerId,
      modelId: activeProviderModel.modelId || agentModelId || undefined,
      workspaceId: currentWorkspaceId || undefined,
    }
    setPendingPrompt(null)

    queueMicrotask(() => {
      // 初始化流式状态（startedAt 由渲染进程生成，传递给主进程原样回传，确保竞态保护使用同一个值）
      const streamStartedAt = Date.now()
      setStreamingStates((prev) => {
        const map = new Map(prev)
        const existing = prev.get(sessionId)
        map.set(sessionId, {
          running: true,
          content: '',
          toolActivities: [],
          teammates: [],
          model: snapshot.modelId,
          startedAt: streamStartedAt,
          inputTokens: existing?.inputTokens,
          contextWindow: existing?.contextWindow,
        })
        return map
      })

      // 乐观更新：显示用户消息
      const tempUserMsg: AgentMessage = {
        id: `temp-${Date.now()}`,
        role: 'user',
        content: snapshot.message,
        createdAt: Date.now(),
      }
      setMessages((prev) => [...prev, tempUserMsg])

      // 发送消息
      const input: AgentSendInput = {
        sessionId,
        userMessage: snapshot.message,
        channelId: snapshot.channelId,
        modelId: snapshot.modelId,
        workspaceId: snapshot.workspaceId,
        startedAt: streamStartedAt,
      }
      sendAgentMessage(input).catch((error: unknown) => {
        console.error('[AgentView] 自动发送配置消息失败:', error)
        setStreamingStates((prev) => {
          const current = prev.get(sessionId)
          if (!current) return prev
          const map = new Map(prev)
          map.set(sessionId, { ...current, running: false })
          return map
        })
      })
    })
  }, [messagesLoaded, pendingPrompt, sessionId, agentChannelId, agentModelId, currentWorkspaceId, streaming, setPendingPrompt, setStreamingStates, activeProviderModel])

  return { messages, setMessages, messagesLoaded }
}
