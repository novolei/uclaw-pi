/**
 * useGlobalChatListeners — 全局 Chat IPC 监听器
 *
 * 在应用顶层挂载，永不销毁。将所有 Chat 流式事件
 * 写入对应 Jotai atoms，确保页面切换时不丢失事件。
 */

import { useEffect } from 'react'
import { useStore } from 'jotai'

/** Jotai 没有公开导出 Store 类型，这里通过 useStore 的返回类型推导 */
type Store = ReturnType<typeof useStore>
import {
  streamingStatesAtom,
  chatStreamErrorsAtom,
  conversationsAtom,
  chatMessageRefreshAtom,
  pendingAgentRecommendationAtom,
} from '@/atoms/chat-atoms'
import { agentSessionsAtom } from '@/atoms/agent-atoms'
import type { ConversationStreamState } from '@/atoms/chat-atoms'
import {
  onStreamChunk,
  onStreamReasoning,
  onStreamComplete,
  onStreamError,
  onStreamToolActivity,
  listConversations as listConversationsIPC,
} from '@/lib/tauri-bridge'

interface StreamChunkEvent { conversationId: string; delta: string; seq?: number }
interface StreamReasoningEvent { conversationId: string; delta: string; seq?: number }
interface StreamCompleteEvent { conversationId: string }
interface StreamErrorEvent { conversationId: string; error: string }
interface StreamToolActivityEvent { conversationId: string; activity: any }

// ─── Module-level singleton ───────────────────────────────────────────────────

const lastChatChunkSeq = new Map<string, number>()
const lastChatReasoningSeq = new Map<string, number>()

let chatCleanupFns: Array<() => void> = []
let chatInitialized = false

function startChatListeners(store: Store): void {
  if (chatInitialized) return
  chatInitialized = true

  const updateState = (
    convId: string,
    updater: (prev: ConversationStreamState) => ConversationStreamState,
  ): void => {
    store.set(streamingStatesAtom, (prev) => {
      const current = prev.get(convId) ?? {
        streaming: false,
        content: '',
        reasoning: '',
        model: undefined,
        toolActivities: [],
        startedAt: Date.now(),
      }
      const map = new Map(prev)
      map.set(convId, updater(current))
      return map
    })
  }

  // ===== 1. 流式内容块 =====
  chatCleanupFns.push(
    onStreamChunk((event: StreamChunkEvent) => {
      const sid = event.conversationId
      if (event.seq !== undefined) {
        if (event.seq === 0) {
          lastChatChunkSeq.delete(sid)
        }
        const last = lastChatChunkSeq.get(sid)
        if (last !== undefined && event.seq <= last) return
        lastChatChunkSeq.set(sid, event.seq)
      }
      updateState(sid, (s) => {
        const isNewStream = event.seq === 0
        return { ...s, content: isNewStream ? event.delta : (s.content + event.delta) }
      })
    })
  )

  // ===== 2. 流式推理内容 =====
  chatCleanupFns.push(
    onStreamReasoning((event: StreamReasoningEvent) => {
      const sid = event.conversationId
      if (event.seq !== undefined) {
        if (event.seq === 0) {
          lastChatReasoningSeq.delete(sid)
        }
        const last = lastChatReasoningSeq.get(sid)
        if (last !== undefined && event.seq <= last) return
        lastChatReasoningSeq.set(sid, event.seq)
      }
      updateState(sid, (s) => {
        const isNewStream = event.seq === 0
        return {
          ...s,
          reasoning: isNewStream ? event.delta : ((s.reasoning ?? '') + event.delta),
          content: isNewStream ? '' : s.content,
        }
      })
    })
  )

  // ===== 3. 流式完成 =====
  chatCleanupFns.push(
    onStreamComplete((event: StreamCompleteEvent) => {
      updateState(event.conversationId, (s) => ({ ...s, streaming: false }))

      store.set(chatMessageRefreshAtom, (prev) => {
        const map = new Map(prev)
        map.set(event.conversationId, (prev.get(event.conversationId) ?? 0) + 1)
        return map
      })

      listConversationsIPC()
        .then((convs: any) => store.set(conversationsAtom, convs))
        .catch(console.error)

      // Title generation is backend-driven: send_message generates the title
      // (utility role → local MiniCPM) and emits `session:title-updated`, which
      // the global agent listener applies to conversationsAtom. (The old
      // frontend `generate_title` command never existed — removed.)
    })
  )

  // ===== 4. 流式错误 =====
  chatCleanupFns.push(
    onStreamError((event: StreamErrorEvent) => {
      updateState(event.conversationId, (s) => ({ ...s, streaming: false }))
      store.set(chatStreamErrorsAtom, (prev) => {
        const map = new Map(prev)
        map.set(event.conversationId, event.error)
        return map
      })
      store.set(chatMessageRefreshAtom, (prev) => {
        const map = new Map(prev)
        map.set(event.conversationId, (prev.get(event.conversationId) ?? 0) + 1)
        return map
      })
    })
  )

  // ===== 5. 工具活动 =====
  chatCleanupFns.push(
    onStreamToolActivity((event: StreamToolActivityEvent) => {
      // Agent tool activities are handled exclusively by useGlobalAgentListeners.
      const agentSessions = store.get(agentSessionsAtom)
      if (agentSessions.some((s) => s.id === event.conversationId)) return

      updateState(event.conversationId, (s) => ({
        ...s,
        toolActivities: [...s.toolActivities, event.activity],
      }))

      if (
        event.activity.type === 'result'
        && event.activity.toolName === 'suggest_agent_mode'
        && event.activity.result
        && !event.activity.isError
      ) {
        try {
          const parsed = JSON.parse(event.activity.result)
          if (parsed.type === 'agent_recommendation' && parsed.reason && parsed.suggestedPrompt) {
            store.set(pendingAgentRecommendationAtom, {
              reason: parsed.reason,
              suggestedPrompt: parsed.suggestedPrompt,
              conversationId: event.conversationId,
            })
          }
        } catch {
          // ignore
        }
      }
    })
  )

  // Vite HMR: tear down before this module is hot-replaced
  if (import.meta.hot) {
    import.meta.hot.dispose(() => {
      chatInitialized = false
      for (const fn of chatCleanupFns) fn()
      chatCleanupFns = []
      lastChatChunkSeq.clear()
      lastChatReasoningSeq.clear()
    })
  }
}

// ─── React hook ──────────────────────────────────────────────────────────────

export function useGlobalChatListeners(): void {
  const store = useStore()

  useEffect(() => {
    startChatListeners(store)
    // No cleanup — listeners are intentionally global for the app lifetime.
  }, [store])
}
