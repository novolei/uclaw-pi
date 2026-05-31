/**
 * useAgentQueue — the Codex-style queued-message handlers (Bundle 2-A), extracted
 * VERBATIM from the AgentView component body during the features/agent migration
 * split.
 *
 * When the agent is streaming, new user messages go into a per-session queue
 * surfaced as a banner above the composer. The queued message doesn't auto-fire —
 * the user decides:
 *   - 引导 (steer)  → handleSteerQueued: send now + interrupt
 *   - 编辑 (edit)   → handleEditQueued: pop back to composer
 *   - 删除 (trash)  → handleDeleteQueued: discard
 *
 * Behavior-preserving move: every useCallback body + dependency array is
 * identical to the original, only relocated. IPC routes through the agent bridge
 * (no `@tauri-apps/api` here).
 */

import * as React from 'react'
import { toast } from 'sonner'
import {
  agentStreamingStatesAtom,
  liveMessagesMapAtom,
} from '@/atoms/agent-atoms'
import {
  agentQueuedMessagesMapAtom,
  removeQueuedMessage,
  type QueuedAgentMessage,
} from '@/atoms/agent-queue-messages'
import type { ActiveProviderModel } from '@/atoms/active-model'
import { agentSteer } from '@/lib/bridge/agent'

type SetMap<K, V> = React.Dispatch<React.SetStateAction<Map<K, V>>>
type JotaiStore = ReturnType<typeof import('jotai').useStore>

export interface UseAgentQueueArgs {
  sessionId: string
  activeProviderModel: ActiveProviderModel | null
  agentModelId: string | null
  store: JotaiStore
  setStreamingStates: SetMap<string, any>
  setInputContent: (value: string) => void
  setComposerHasText: (v: boolean) => void
}

export interface UseAgentQueueResult {
  handleSteerQueued: (msg: QueuedAgentMessage) => void
  handleEditQueued: (msg: QueuedAgentMessage) => void
  handleDeleteQueued: (msg: QueuedAgentMessage) => void
}

export function useAgentQueue(args: UseAgentQueueArgs): UseAgentQueueResult {
  const {
    sessionId,
    activeProviderModel,
    agentModelId,
    store,
    setStreamingStates,
    setInputContent,
    setComposerHasText,
  } = args

  /** Steer = send now + interrupt current turn. Mirrors the legacy
   *  streaming-append path from before the queue.
   *
   *  Fix for user-reported regression: after interrupting the previous
   *  turn, the streaming-state listener flips running=false. The new
   *  turn fires via queueAgentMessage(interrupt:true) but its
   *  streaming-start event takes a tick to arrive — UI shows "no
   *  streaming animation" in that gap. We pre-emptively set
   *  running:true on the streamingStatesAtom so the indicator stays
   *  continuous through the steer. The backend's actual streaming
   *  events take over once they land.
   */
  const handleSteerQueued = React.useCallback((msg: QueuedAgentMessage) => {
    if (!activeProviderModel) return
    // Remove from queue first so the banner closes before the network call.
    store.set(agentQueuedMessagesMapAtom, (prev) =>
      removeQueuedMessage(prev, sessionId, msg.id),
    )

    const localUuid = crypto.randomUUID()
    // Inject the synthetic user message into liveMessages so the chat
    // shows the steer immediately (no jank).
    const syntheticMsg = {
      type: 'user',
      uuid: localUuid,
      message: { content: [{ type: 'text', text: msg.text }] },
      parent_tool_use_id: null,
      _createdAt: Date.now(),
    }
    store.set(liveMessagesMapAtom, (prev) => {
      const map = new Map(prev)
      const current = map.get(sessionId) ?? []
      map.set(sessionId, [...current, syntheticMsg])
      return map
    })

    // Critical: keep the streaming-state running so the "Agent Running"
    // indicator + 3×3 spinner don't blink off between the interrupt
    // landing and the new turn's first stream chunk arriving.
    const streamStartedAt = Date.now()
    setStreamingStates((prev) => {
      const map = new Map(prev)
      const existing = prev.get(sessionId)
      map.set(sessionId, {
        running: true,
        // Reset the bubble's accumulated content so the new turn starts
        // with a clean slate visually.
        content: '',
        toolActivities: [],
        teammates: existing?.teammates ?? [],
        model: existing?.model ?? agentModelId ?? undefined,
        startedAt: streamStartedAt,
        inputTokens: existing?.inputTokens,
        contextWindow: existing?.contextWindow,
      })
      return map
    })

    agentSteer({
      sessionId,
      userMessage: msg.text,
      uuid: msg.id,
    }).catch((error: unknown) => {
      console.error('[AgentView] steer queued message failed:', error)
      toast.error('引导消息失败', { description: String(error) })
      // Rollback the synthetic message
      store.set(liveMessagesMapAtom, (prev) => {
        const map = new Map(prev)
        const current = (map.get(sessionId) ?? []).filter(
          (m) => (m as unknown as { uuid?: string }).uuid !== localUuid,
        )
        map.set(sessionId, current)
        return map
      })
    })
  }, [sessionId, activeProviderModel, store, setStreamingStates, agentModelId])

  /** Edit = pop from queue, restore to composer for further editing. */
  const handleEditQueued = React.useCallback((msg: QueuedAgentMessage) => {
    store.set(agentQueuedMessagesMapAtom, (prev) =>
      removeQueuedMessage(prev, sessionId, msg.id),
    )
    setInputContent(msg.text)
    setComposerHasText(msg.text.length > 0)
  }, [sessionId, store, setInputContent, setComposerHasText])

  /** Delete = silently discard. */
  const handleDeleteQueued = React.useCallback((msg: QueuedAgentMessage) => {
    store.set(agentQueuedMessagesMapAtom, (prev) =>
      removeQueuedMessage(prev, sessionId, msg.id),
    )
  }, [sessionId, store])

  return { handleSteerQueued, handleEditQueued, handleDeleteQueued }
}
