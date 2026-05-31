/**
 * useAgentActions — the AgentView shell's stop / compact / retry / fork / rewind
 * action handlers + the agent-error toast/copy wiring, extracted VERBATIM from
 * the AgentView component body during the features/agent migration split.
 *
 * Owns (unchanged from the original — same callbacks, same deps, same order):
 *  - handleStop          (stop the current streaming turn)
 *  - handleCompact       (manual /compact: synthetic bubble + compacting marker)
 *  - the handleCompactRef bridge (handleSend reads it to avoid a TDZ closure)
 *  - the agent-error toast effect + handleCopyError
 *  - handleRetry / handleRetryInNewSession
 *  - handleFork
 *  - the rewind confirm-dialog state + handleRewindRequest / handleRewindConfirm
 *  - the proma:stop-generation + proma:focus-input keyboard-shortcut listeners
 *
 * Behavior-preserving move: every body + dependency array is identical to the
 * original, only relocated. IPC routes through the agent bridge (no
 * `@tauri-apps/api` here).
 */

import * as React from 'react'
import { toast } from 'sonner'
import type { AgentMessage } from '@/lib/agent-types'
import type { ActiveProviderModel } from '@/atoms/active-model'
import {
  agentStreamingStatesAtom,
  agentStreamErrorsAtom,
  agentMessageRefreshAtom,
  liveMessagesMapAtom,
  agentSessionsAtom,
  finalizeStreamingActivities,
  type AgentStreamErrorPayload,
} from '@/atoms/agent-atoms'
import { useOpenSession } from '@/hooks/useOpenSession'
import {
  sendAgentMessage,
  stopAgent,
  createAgentSession,
  forkAgentSession,
  rewindSession,
} from '@/lib/bridge/agent'

type SetMap<K, V> = React.Dispatch<React.SetStateAction<Map<K, V>>>
type JotaiStore = ReturnType<typeof import('jotai').useStore>

export interface UseAgentActionsArgs {
  sessionId: string
  messages: AgentMessage[]
  setMessages: React.Dispatch<React.SetStateAction<AgentMessage[]>>
  agentChannelId: string | null
  agentModelId: string | null
  currentWorkspaceId: string | null
  activeProviderModel: ActiveProviderModel | null
  streaming: boolean
  agentError: AgentStreamErrorPayload | null
  store: JotaiStore
  setStreamingStates: SetMap<string, any>
  setAgentStreamErrors: SetMap<string, any>
  setAgentSessions: React.Dispatch<React.SetStateAction<any[]>>
}

export interface UseAgentActionsResult {
  handleStop: () => void
  handleCompact: () => void
  handleCompactRef: React.MutableRefObject<(() => void) | null>
  handleCopyError: () => Promise<void>
  errorCopied: boolean
  handleRetry: () => void
  handleRetryInNewSession: () => Promise<void>
  handleFork: (upToMessageUuid: string) => Promise<void>
  rewindTargetUuid: string | null
  setRewindTargetUuid: React.Dispatch<React.SetStateAction<string | null>>
  handleRewindRequest: (assistantMessageUuid: string) => void
  handleRewindConfirm: () => Promise<void>
}

export function useAgentActions(args: UseAgentActionsArgs): UseAgentActionsResult {
  const {
    sessionId,
    messages,
    agentChannelId,
    agentModelId,
    currentWorkspaceId,
    activeProviderModel,
    streaming,
    agentError,
    store,
    setStreamingStates,
    setAgentStreamErrors,
    setAgentSessions,
  } = args

  const openSession = useOpenSession()
  const [errorCopied, setErrorCopied] = React.useState(false)

  /** 停止生成 */
  const handleStop = React.useCallback((): void => {
    setStreamingStates((prev) => {
      const current = prev.get(sessionId)
      if (!current || !current.running) return prev
      const map = new Map(prev)
      map.set(sessionId, {
        ...current,
        running: false,
        ...finalizeStreamingActivities(current.toolActivities, current.teammates),
      })
      return map
    })

    stopAgent(sessionId).catch(console.error)
  }, [sessionId, setStreamingStates])

  /** 手动发送 /compact 命令 */
  const handleCompact = React.useCallback((): void => {
    // /compact 的后端拦截在任何 channel/model 逻辑之前发生，不需要 channelId。
    // 旧 `if (!agentChannelId) return` 守卫会在用户没显式选 channel 时
    // 静默吞掉整个调用（普通消息有 activeProviderModel.providerId 兜底，
    // 但 handleCompact 没兜底）→ 前端清输入框、不发 IPC、零反应。

    // 如果当前正在 streaming（agent 还在多轮工具调用中），先停掉当前 turn，
    // 再走 /compact。
    if (streaming) {
      stopAgent(sessionId).catch(console.error)
    }

    const streamStartedAt = Date.now()
    const localUuid = crypto.randomUUID()

    // 1. 立即注入合成用户消息（/compact 气泡立刻可见，与普通发送路径一致）
    //    同时注入 compacting 系统消息 → CompactingIndicator，避免 isCompacting
    //    flag 在 React 单批次内翻转（false→true→false）导致指示器从未渲染。
    const syntheticMsg = {
      type: 'user',
      uuid: localUuid,
      message: {
        content: [{ type: 'text', text: '/compact' }],
      },
      parent_tool_use_id: null,
      _createdAt: streamStartedAt,
    }
    const compactingMsg = {
      type: 'system',
      subtype: 'compacting',
      uuid: `compacting-${localUuid}`,
      _createdAt: streamStartedAt,
    }

    store.set(liveMessagesMapAtom, (prev) => {
      const map = new Map(prev)
      const current = map.get(sessionId) ?? []
      map.set(sessionId, [...current, syntheticMsg, compactingMsg])
      return map
    })

    // 2. 初始化流式状态 + 乐观设 isCompacting=true（SDK compacting 事件之前就显示"正在压缩..."分隔符）
    setStreamingStates((prev) => {
      const map = new Map(prev)
      const current = prev.get(sessionId) ?? {
        running: true,
        content: '',
        toolActivities: [],
        teammates: [],
        model: agentModelId || undefined,
        startedAt: streamStartedAt,
      }
      map.set(sessionId, { ...current, running: true, startedAt: streamStartedAt, isCompacting: true, compactInFlight: true })
      return map
    })

    sendAgentMessage({
      sessionId,
      userMessage: '/compact',
      // 兜底用 activeProviderModel.providerId（与普通发送路径一致），
      // 否则用户未显式选 channel 时 agentChannelId 为 null，IPC 会带空 channelId。
      // 后端 /compact 拦截不读 channelId，这里只是保持 schema 完整。
      channelId: agentChannelId ?? activeProviderModel?.providerId ?? null,
      modelId: agentModelId || undefined,
      workspaceId: currentWorkspaceId || undefined,
      startedAt: streamStartedAt,
    }).catch((error: unknown) => {
      console.error('[AgentView] /compact 发送失败:', error)
      // 回滚：移除合成用户消息 + compacting 消息 + 清除 isCompacting flag
      store.set(liveMessagesMapAtom, (prev) => {
        const map = new Map(prev)
        const current = (map.get(sessionId) ?? []).filter(
          (m) => (m as unknown as { uuid?: string }).uuid !== localUuid
            && (m as unknown as { uuid?: string }).uuid !== `compacting-${localUuid}`,
        )
        map.set(sessionId, current)
        return map
      })
      setStreamingStates((prev) => {
        const map = new Map(prev)
        const current = prev.get(sessionId)
        if (!current) return prev
        map.set(sessionId, { ...current, isCompacting: false, compactInFlight: false })
        return map
      })
    })
  }, [sessionId, agentChannelId, agentModelId, currentWorkspaceId, streaming, setStreamingStates, store, activeProviderModel])

  // 给 handleSend 用的 handleCompact 引用：handleCompact 在文件下方定义，
  // handleSend 不能直接闭包它（会触发 use-before-declaration），用 ref 解耦。
  const handleCompactRef = React.useRef<typeof handleCompact | null>(null)
  React.useEffect(() => {
    handleCompactRef.current = handleCompact
  }, [handleCompact])

  // 当 agent 报错时用 toast 通知用户（outer_timeout 改为内联展示，不弹 toast）
  const prevAgentError = React.useRef<AgentStreamErrorPayload | null>(null)
  React.useEffect(() => {
    if (agentError && agentError !== prevAgentError.current) {
      // outer_timeout 有专属的内联错误块，无需 toast
      if (agentError.kind !== 'outer_timeout') {
        toast.error('Agent 出错了', { description: agentError.message, duration: 6000 })
      }
    }
    prevAgentError.current = agentError
  }, [agentError])

  /** 复制错误信息到剪贴板 */
  const handleCopyError = React.useCallback(async (): Promise<void> => {
    if (!agentError) return

    try {
      await navigator.clipboard.writeText(agentError.message)
      setErrorCopied(true)
      setTimeout(() => setErrorCopied(false), 2000)
    } catch (error) {
      console.error('[AgentView] 复制错误信息失败:', error)
    }
  }, [agentError])

  /** 重试：在当前会话中重新发送最后一条用户消息 */
  const handleRetry = React.useCallback((): void => {
    if (!agentChannelId || streaming) return

    // 找到最后一条用户消息
    const lastUserMsg = [...messages].reverse().find((m) => m.role === 'user')
    if (!lastUserMsg) return

    // 清除错误状态
    setAgentStreamErrors((prev) => {
      if (!prev.has(sessionId)) return prev
      const map = new Map(prev)
      map.delete(sessionId)
      return map
    })

    // 初始化流式状态（startedAt 由渲染进程生成，传递给主进程原样回传）
    const streamStartedAt = Date.now()
    setStreamingStates((prev) => {
      const map = new Map(prev)
      const existing = prev.get(sessionId)
      map.set(sessionId, {
        running: true,
        content: '',
        toolActivities: [],
        teammates: [],
        model: agentModelId || undefined,
        startedAt: streamStartedAt,
        inputTokens: existing?.inputTokens,
        contextWindow: existing?.contextWindow,
      })
      return map
    })

    sendAgentMessage({
      sessionId,
      userMessage: lastUserMsg.content,
      channelId: agentChannelId,
      modelId: agentModelId || undefined,
      workspaceId: currentWorkspaceId || undefined,
      startedAt: streamStartedAt,
    }).catch(console.error)
  }, [messages, sessionId, agentChannelId, agentModelId, currentWorkspaceId, streaming, setAgentStreamErrors, setStreamingStates])

  /** 在新会话中重试：创建新会话 + 切换 tab + 发送引用旧会话的提示词 */
  const handleRetryInNewSession = React.useCallback(async (): Promise<void> => {
    if (!agentChannelId) return

    try {
      const meta = await createAgentSession(
        undefined, agentChannelId, currentWorkspaceId || undefined,
      )
      setAgentSessions((prev) => [meta, ...prev])

      // 切换到新会话 tab
      openSession('agent', meta.id, meta.title)

      // 发送引用旧会话的默认提示词
      const prompt = `上个会话的 id 是 ${sessionId}，可以参考同工作区下的会话继续完成工作`

      // 初始化新会话流式状态
      setStreamingStates((prev) => {
        const map = new Map(prev)
        map.set(meta.id, {
          running: true,
          content: '',
          toolActivities: [],
          teammates: [],
          model: agentModelId || undefined,
          startedAt: Date.now(),
        })
        return map
      })

      sendAgentMessage({
        sessionId: meta.id,
        userMessage: prompt,
        channelId: agentChannelId,
        modelId: agentModelId || undefined,
        workspaceId: currentWorkspaceId || undefined,
      }).catch(console.error)
    } catch (error) {
      console.error('[AgentView] 在新会话中重试失败:', error)
    }
  }, [sessionId, agentChannelId, agentModelId, currentWorkspaceId, openSession, setAgentSessions, setStreamingStates])

  /** 分叉会话：从指定消息处创建新会话并自动切换 */
  const handleFork = React.useCallback(async (upToMessageUuid: string): Promise<void> => {
    try {
      const meta = await forkAgentSession({
        sessionId,
        upToMessageUuid,
      })
      setAgentSessions((prev) => [meta, ...prev])

      // 切换到新会话 tab
      openSession('agent', meta.id, meta.title)

      toast.success('已创建分叉会话', {
        description: meta.title,
      })
    } catch (error) {
      console.error('[AgentView] 分叉会话失败:', error)
      toast.error('分叉会话失败', {
        description: error instanceof Error ? error.message : '未知错误',
      })
    }
  }, [sessionId, openSession, setAgentSessions])

  /** 快照回退：同一会话内回退到指定消息点，恢复文件 + 截断对话 */
  const [rewindTargetUuid, setRewindTargetUuid] = React.useState<string | null>(null)

  const handleRewindRequest = React.useCallback((assistantMessageUuid: string): void => {
    setRewindTargetUuid(assistantMessageUuid)
  }, [])

  const handleRewindConfirm = React.useCallback(async (): Promise<void> => {
    if (!rewindTargetUuid) return
    const targetUuid = rewindTargetUuid
    setRewindTargetUuid(null)

    try {
      const result = await rewindSession({
        sessionId,
        assistantMessageUuid: targetUuid,
      })

      // 刷新消息列表
      store.set(agentMessageRefreshAtom, (prev) => {
        const map = new Map(prev)
        map.set(sessionId, (prev.get(sessionId) ?? 0) + 1)
        return map
      })

      if (result.fileRewind?.canRewind) {
        const fileCount = result.fileRewind.filesChanged?.length ?? 0
        toast.success('已回退到此处', {
          description: fileCount > 0 ? `${fileCount} 个文件已恢复` : '文件无变化',
        })
      } else if (result.fileRewind?.error) {
        toast.warning('已回退对话', {
          description: `文件恢复不可用：${result.fileRewind.error}`,
        })
      } else {
        toast.success('已回退到此处')
      }
    } catch (error) {
      console.error('[AgentView] 回退失败:', error)
      toast.error('回退失败', {
        description: error instanceof Error ? error.message : '未知错误',
      })
    }
  }, [rewindTargetUuid, sessionId, store])

  // 监听快捷键系统分发的 stop-generation 事件
  React.useEffect(() => {
    const handler = (): void => {
      if (streaming) handleStop()
    }
    window.addEventListener('proma:stop-generation', handler)
    return () => window.removeEventListener('proma:stop-generation', handler)
  }, [streaming, handleStop])

  // 监听快捷键系统分发的 focus-input 事件（Cmd+L）
  React.useEffect(() => {
    const handler = (): void => {
      const proseMirror = document.querySelector('[data-input-mode="agent"] .ProseMirror') as HTMLElement | null
      proseMirror?.focus()
    }
    window.addEventListener('proma:focus-input', handler)
    return () => window.removeEventListener('proma:focus-input', handler)
  }, [])

  return {
    handleStop,
    handleCompact,
    handleCompactRef,
    handleCopyError,
    errorCopied,
    handleRetry,
    handleRetryInNewSession,
    handleFork,
    rewindTargetUuid,
    setRewindTargetUuid,
    handleRewindRequest,
    handleRewindConfirm,
  }
}
