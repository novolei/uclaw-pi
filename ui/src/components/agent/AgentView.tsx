/**
 * AgentView — Agent 模式主视图容器
 *
 * 职责：
 * - 加载当前 Agent 会话消息
 * - 发送/停止/压缩 Agent 消息
 * - 附件上传处理
 * - AgentHeader 支持标题编辑 + 文件浏览器切换
 *
 * 注意：IPC 流式事件监听已提升到全局 useGlobalAgentListeners，
 * 本组件为纯展示 + 交互组件。
 *
 * 布局：AgentHeader | AgentMessages | AgentInput + 可选 FileBrowser 侧面板
 */

import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom, useStore } from 'jotai'
import { toast } from 'sonner'
import { Bot, CornerDownLeft, Square, Settings, Paperclip, X, Copy, Check, Brain, Map as MapIcon, Sparkles, AlertTriangle } from 'lucide-react'
import { AgentMessages } from '@/features/agent'
import { AgentHeader } from '@/features/agent'
import { BrowserPreviewOverlay } from '@/features/agent'
import { ContextUsageBadge } from '@/features/agent'
import { AutoPreviewPopover } from '@/features/agent'
import { StrategyPresetSelector } from '@/features/agent'
import { PermissionBanner } from '@/features/agent'
// Bundle 27-A — heartbeat indicator + stall banner + interrupted-reply recovery.
import { AgentHeartbeatBanner } from '@/features/agent'
import { PermissionModeSelector } from '@/features/agent'
import { AgentStatusBar } from '@/features/agent'
import { AskUserBanner } from '@/features/agent'
import { ExitPlanModeBanner } from '@/features/agent'
import { PlanModeSuggestBanner } from '@/features/agent'
import { AutomationRunBanner } from '@/features/agent'
import { PlanModeDashedBorder } from '@/features/agent'
import { PetWidget } from '@/features/agent'
import { ProviderModelSelector } from '@/components/chat/ProviderModelSelector'
import { AttachmentPreviewItem } from '@/components/chat/AttachmentPreviewItem'
import { RichTextInput } from '@/components/ai-elements/rich-text-input'
import { SpeechButton } from '@/components/ai-elements/speech-button'
import { SttModal } from '@/components/stt/SttModal'
import { FirstRunDialog } from '@/components/stt/FirstRunDialog'
import { modelStatusAtom } from '@/atoms/stt-atoms'
import { smartJoin } from '@/lib/stt/punctuation'
import { invoke } from '@tauri-apps/api/core'
import {
  ComposerMentionController,
  type ComposerMentionControllerHandle,
} from '@/components/composer/ComposerMentionController'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Switch } from '@/components/ui/switch'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { cn } from '@/lib/utils'
import { getActiveAccelerator, getAcceleratorDisplay } from '@/lib/shortcut-registry'
import { FeishuNotifyToggle } from '@/components/chat/FeishuNotifyToggle'
import {
  agentStreamingStatesAtom,
  agentChannelIdAtom,
  agentModelIdAtom,
  agentSessionChannelMapAtom,
  agentSessionModelMapAtom,
  agentSessionStrategyMapAtom,
  currentAgentWorkspaceIdAtom,
  agentPendingPromptAtom,
  agentPendingFilesAtom,
  agentWorkspacesAtom,
  agentStreamErrorsAtom,
  type AgentStreamErrorPayload,
  agentSessionDraftsAtom,
  agentSessionDraftHtmlAtom,
  agentPromptSuggestionsAtom,
  agentMessageRefreshAtom,
  agentSessionsAtom,
  liveMessagesMapAtom,
  agentThinkingAtom,
  stoppedByUserSessionsAtom,
  agentPlanModeSessionsAtom,
  agentSessionPathMapAtom,
  allPendingAskUserRequestsAtom,
  allPendingExitPlanRequestsAtom,
  finalizeStreamingActivities,
  workspaceAttachedDirsMapAtom,
  agentSessionAttachedDirsMapAtom,
  workspaceFilesVersionAtom,
  composerFocusedAtom,
  composerHasTextAtom,
  proactiveLearningEventsAtom,
  memoryRecallEventAtom,
  skillRecallsMapAtom,
} from '@/atoms/agent-atoms'
import type { AgentContextStatus } from '@/atoms/agent-atoms'
import {
  agentQueuedMessagesMapAtom,
  removeQueuedMessage,
  type QueuedAgentMessage,
} from '@/atoms/agent-queue-messages'
import { QueuedMessagesBanner } from '@/features/agent'
import { activeProviderModelAtom } from '@/atoms/active-model'
import { channelsAtom, thinkingExpandedAtom } from '@/atoms/chat-atoms'
import { workspacesAtom } from '@/atoms/workspace'
import { useOpenSession } from '@/hooks/useOpenSession'
import { AgentSessionProvider } from '@/contexts/session-context'
import { draftSessionIdsAtom } from '@/atoms/draft-session-atoms'
import { sendWithCmdEnterAtom } from '@/atoms/shortcut-atoms'
import { agentStatusBarEnabledAtom } from '@/atoms/ui-preferences'
import type { AgentSendInput, AgentMessage, AgentPendingFile } from '@/lib/agent-types'
import { fileToBase64 } from '@/lib/file-utils'
import { createClipboardTextFile } from '@/lib/clipboard-attachment'
import { GitChipsRow } from '@/components/chat/git/GitChipsRow'
import {
  updateSettings,
  getAgentSessionPath,
  getAgentSessionMessages,
  sendAgentMessage,
  stopAgent,
  openFileDialog,
  getPathForFile,
  checkPathsType,
  createAgentSession,
  forkAgentSession,
  rewindSession,
  saveFilesToAgentSession,
  agentSteer,
  agentFollowUp,
  onStreamComplete,
  onQueuedConsumed,
  attachSessionDirectory,
  estimateSessionContext,
} from '@/lib/tauri-bridge'

// ===== 思考模式 Hover Popover =====

interface AgentThinkingPopoverProps {
  agentThinking: import('@/lib/proma-types').ThinkingConfig | undefined
  onToggle: () => void
}

function AgentThinkingPopover({ agentThinking, onToggle }: AgentThinkingPopoverProps): React.ReactElement {
  const [thinkingExpanded, setThinkingExpanded] = useAtom(thinkingExpandedAtom)
  const [open, setOpen] = React.useState(false)
  const hoverTimeout = React.useRef<ReturnType<typeof setTimeout> | null>(null)

  const isEnabled = agentThinking?.type === 'adaptive'

  const handleMouseEnter = React.useCallback(() => {
    if (hoverTimeout.current) clearTimeout(hoverTimeout.current)
    setOpen(true)
  }, [])

  const handleMouseLeave = React.useCallback(() => {
    hoverTimeout.current = setTimeout(() => setOpen(false), 150)
  }, [])

  React.useEffect(() => {
    return () => {
      if (hoverTimeout.current) clearTimeout(hoverTimeout.current)
    }
  }, [])

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className={cn(
            'size-[36px] rounded-full',
            isEnabled ? 'text-green-500' : 'text-foreground/60 hover:text-foreground'
          )}
          onClick={onToggle}
          onMouseEnter={handleMouseEnter}
          onMouseLeave={handleMouseLeave}
        >
          <Brain className="size-5" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        side="top"
        align="center"
        sideOffset={8}
        className="w-auto min-w-[160px] p-2 px-2.5"
        onMouseEnter={handleMouseEnter}
        onMouseLeave={handleMouseLeave}
        onOpenAutoFocus={(e) => e.preventDefault()}
      >
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center justify-between gap-4">
            <span className="text-xs text-foreground/70">思考模式</span>
            <Switch
              checked={isEnabled}
              onCheckedChange={onToggle}
              className="h-4 w-7 [&>span]:size-3 [&>span]:data-[state=checked]:translate-x-3"
            />
          </div>
          <div className="h-px bg-border" />
          <div className="flex items-center justify-between gap-4">
            <span className="text-xs text-foreground/70">展开思考</span>
            <Switch
              checked={thinkingExpanded}
              onCheckedChange={setThinkingExpanded}
              className="h-4 w-7 [&>span]:size-3 [&>span]:data-[state=checked]:translate-x-3"
            />
          </div>
        </div>
      </PopoverContent>
    </Popover>
  )
}

export function AgentView({ sessionId }: { sessionId: string }): React.ReactElement {
  // [FLASH-DEBUG] 渲染计数器
  const renderCountRef = React.useRef(0)
  renderCountRef.current++
  if (renderCountRef.current % 50 === 0) {
    console.log(`[FLASH-DEBUG] AgentView(${sessionId.slice(0, 8)}) render #${renderCountRef.current}`)
  }

  const [messages, setMessages] = React.useState<AgentMessage[]>([])
  const setStreamingStates = useSetAtom(agentStreamingStatesAtom)
  const streamingStates = useAtomValue(agentStreamingStatesAtom)
  const streamState = streamingStates.get(sessionId)
  const streaming = streamState?.running ?? false
  const stoppedByUserSessions = useAtomValue(stoppedByUserSessionsAtom)
  const sendWithCmdEnter = useAtomValue(sendWithCmdEnterAtom)
  const stoppedByUser = stoppedByUserSessions.has(sessionId)
  const liveMessagesMap = useAtomValue(liveMessagesMapAtom)
  const setLiveMessagesMap = useSetAtom(liveMessagesMapAtom)
  // 稳定化空数组引用，避免 ?? [] 每次创建新引用导致下游 useMemo 链不必要重算
  const liveMessages = liveMessagesMap.get(sessionId) ?? []
  // Per-session 渠道/模型配置（优先读 session map，回退到全局默认值）
  const sessionChannelMap = useAtomValue(agentSessionChannelMapAtom)
  const sessionModelMap = useAtomValue(agentSessionModelMapAtom)
  const setSessionChannelMap = useSetAtom(agentSessionChannelMapAtom)
  const setSessionModelMap = useSetAtom(agentSessionModelMapAtom)
  const sessionStrategyMap = useAtomValue(agentSessionStrategyMapAtom)
  const currentStrategy = sessionStrategyMap.get(sessionId) ?? 'balanced'
  const defaultChannelId = useAtomValue(agentChannelIdAtom)
  const [defaultModelId, setDefaultModelId] = useAtom(agentModelIdAtom)
  const agentChannelId = sessionChannelMap.get(sessionId) ?? defaultChannelId
  const agentModelId = sessionModelMap.get(sessionId) ?? defaultModelId
  const [activeProviderModel] = useAtom(activeProviderModelAtom)
  const [agentThinking, setAgentThinking] = useAtom(agentThinkingAtom)
  const setDraftSessionIds = useSetAtom(draftSessionIdsAtom)
  const globalWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const sessions = useAtomValue(agentSessionsAtom)
  // 从会话元数据派生 workspaceId：会话数据已加载时以自身为准，未加载时回退全局 atom
  const currentWorkspaceId = React.useMemo(() => {
    const meta = sessions.find((s) => s.id === sessionId)
    if (!meta) return globalWorkspaceId // 数据未加载，回退全局
    return meta.workspaceId ?? null     // 数据已加载，以会话自身为准
  }, [sessions, sessionId, globalWorkspaceId])
  const [pendingPrompt, setPendingPrompt] = useAtom(agentPendingPromptAtom)
  const [pendingFiles, setPendingFiles] = useAtom(agentPendingFilesAtom)
  const workspaces = useAtomValue(agentWorkspacesAtom)
  // 保持 channelId 稳定：初始化前使用上次有效值，避免工具栏抖动
  const stableChannelIdRef = React.useRef(agentChannelId)
  if (agentChannelId) stableChannelIdRef.current = agentChannelId
  const stableChannelId = agentChannelId ?? stableChannelIdRef.current

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

  // Pull every available field from streamState — the badge popover shows
  // input/output/cache breakdown + dollar cost. Earlier this dropped most
  // fields, so the popover rendered blanks even when the data was there.
  const contextStatus: AgentContextStatus = {
    isCompacting: streamState?.isCompacting ?? false,
    inputTokens: streamState?.inputTokens,
    outputTokens: streamState?.outputTokens,
    cacheReadTokens: streamState?.cacheReadTokens,
    cacheCreationTokens: streamState?.cacheCreationTokens,
    costUsd: streamState?.costUsd,
    contextWindow: streamState?.contextWindow,
    skillsTokens: streamState?.skillsTokens,
  }
  const setAgentStreamErrors = useSetAtom(agentStreamErrorsAtom)
  const streamErrors = useAtomValue(agentStreamErrorsAtom)
  const agentError = streamErrors.get(sessionId) ?? null
  const planModeSessions = useAtomValue(agentPlanModeSessionsAtom)
  const isPlanMode = planModeSessions.has(sessionId)
  const store = useStore()
  const suggestionsMap = useAtomValue(agentPromptSuggestionsAtom)
  const suggestion = suggestionsMap.get(sessionId) ?? null
  const setPromptSuggestions = useSetAtom(agentPromptSuggestionsAtom)
  const setAgentSessions = useSetAtom(agentSessionsAtom)
  const openSession = useOpenSession()
  // Phase 2: real atom subscriptions for attached dirs (workspace + session levels).
  const wsAttachedMap = useAtomValue(workspaceAttachedDirsMapAtom)
  const sessionAttachedMap = useAtomValue(agentSessionAttachedDirsMapAtom)
  const setSessionAttachedMap = useSetAtom(agentSessionAttachedDirsMapAtom)
  const attachedDirs = sessionAttachedMap.get(sessionId) ?? []
  const wsAttachedDirs = currentWorkspaceId ? (wsAttachedMap.get(currentWorkspaceId) ?? []) : []
  // Phase 3 (Task 7): file version bump for SidePanel refresh after native drops.
  const setFilesVersion = useSetAtom(workspaceFilesVersionAtom)

  const draftsMap = useAtomValue(agentSessionDraftsAtom)
  const setDraftsMap = useSetAtom(agentSessionDraftsAtom)
  const inputContent = draftsMap.get(sessionId) ?? ''
  const setInputContent = React.useCallback((value: string) => {
    setDraftsMap((prev) => {
      const map = new Map(prev)
      if (value.trim() === '') {
        map.delete(sessionId)
      } else {
        map.set(sessionId, value)
      }
      return map
    })
  }, [sessionId, setDraftsMap])
  // ── composer state atoms (PetWidget) ──
  const setComposerFocused = useSetAtom(composerFocusedAtom)
  const setComposerHasText = useSetAtom(composerHasTextAtom)

  // Reset composer has-text atom when the active session changes.
  // composerFocusedAtom self-heals via TipTap's onBlur on unmount; no reset needed.
  React.useEffect(() => {
    setComposerHasText(false)
  }, [sessionId, setComposerHasText])

  const handleComposerChange = React.useCallback((v: string) => {
    setInputContent(v)
    setComposerHasText(v.trim().length > 0)
  }, [setInputContent, setComposerHasText])

  const handleComposerFocus = React.useCallback(() => setComposerFocused(true), [setComposerFocused])
  const handleComposerBlur  = React.useCallback(() => setComposerFocused(false), [setComposerFocused])

  const draftHtmlMap = useAtomValue(agentSessionDraftHtmlAtom)
  const setDraftHtmlMap = useSetAtom(agentSessionDraftHtmlAtom)
  const inputHtmlContent = draftHtmlMap.get(sessionId) ?? ''
  const setInputHtmlContent = React.useCallback((html: string) => {
    setDraftHtmlMap((prev) => {
      const map = new Map(prev)
      if (!html || html === '<p></p>') {
        map.delete(sessionId)
      } else {
        map.set(sessionId, html)
      }
      return map
    })
  }, [sessionId, setDraftHtmlMap])
  const sessionPathMap = useAtomValue(agentSessionPathMapAtom)
  const setSessionPathMap = useSetAtom(agentSessionPathMapAtom)
  const sessionPath = sessionPathMap.get(sessionId) ?? null
  const [isDragOver, setIsDragOver] = React.useState(false)
  const [errorCopied, setErrorCopied] = React.useState(false)

  // STT state
  const [firstRunOpen, setFirstRunOpen] = React.useState(false)
  const setModelStatus = useSetAtom(modelStatusAtom)

  // Query model status on mount so SpeechButton can show indicator dot.
  React.useEffect(() => {
    void invoke('stt_model_status')
      .then((s: unknown) => {
        const status = s as { openflow_ready: boolean; openflow_model_dir: string }
        setModelStatus(
          status.openflow_ready
            ? { kind: 'ready', modelDir: status.openflow_model_dir }
            : { kind: 'not-downloaded', expectedDir: status.openflow_model_dir },
        )
      })
      .catch(() => {
        /* leave modelStatus = unknown */
      })
  }, [setModelStatus])

  // Composer `/` and `@` autocomplete plumbing — the controller renders
  // the popup; the editorRef lets it watch the TipTap selection state;
  // the controllerRef gives RichTextInput a way to intercept ↑↓ Enter
  // Esc when the popup is open.
  const composerEditorRef = React.useRef<import('@tiptap/core').Editor | null>(null)
  const mentionControllerRef = React.useRef<ComposerMentionControllerHandle | null>(null)

  // pendingFiles ref（供 addFilesAsAttachments 读取最新列表，避免闭包旧值）
  const pendingFilesRef = React.useRef(pendingFiles)
  React.useEffect(() => {
    pendingFilesRef.current = pendingFiles
  }, [pendingFiles])

  // 渠道已选但模型未选时，自动选择第一个可用模型
  const globalChannels = useAtomValue(channelsAtom)

  // 是否有可用模型：使用新的 Provider 体系（activeProviderModelAtom）
  const hasAvailableModel = activeProviderModel !== null
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

  // Phase 2: derived from workspacesAtom (Task 4 auto-mkdir fills .path).
  const wsList = useAtomValue(workspacesAtom)
  const workspaceFilesPath = React.useMemo(() => {
    const ws = wsList.find((w) => w.id === currentWorkspaceId)
    return ws?.path ?? null
  }, [wsList, currentWorkspaceId])
  // workspaceSlug is no longer used (slug removed from AgentWorkspace in Phase 1).
  const workspaceSlug: string | null = null

  const allAttachedDirs = React.useMemo(() => {
    const dirs = [...attachedDirs]
    for (const d of wsAttachedDirs) {
      if (!dirs.includes(d)) dirs.push(d)
    }
    if (workspaceFilesPath && !dirs.includes(workspaceFilesPath)) {
      dirs.unshift(workspaceFilesPath)
    }
    return dirs
  }, [attachedDirs, wsAttachedDirs, workspaceFilesPath])

  // NOTE: native OS drag-drop listener was here (Phase 3 Task 7) but has been
  // moved to AppShell (singleton) to avoid N-tab duplication. See AppShell.tsx.

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
        setLiveMessagesMap((prev) => {
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
  }, [sessionId, refreshVersion, setStreamingStates, setLiveMessagesMap, store])

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
  }, [messagesLoaded, pendingPrompt, sessionId, agentChannelId, agentModelId, currentWorkspaceId, streaming, setPendingPrompt, setStreamingStates])

  // ===== 附件处理 =====

  /** 为文件生成唯一文件名（避免粘贴多张图片时文件名重复导致覆盖） */
  const makeUniqueFilename = React.useCallback((originalName: string, existingNames: string[]): string => {
    if (!existingNames.includes(originalName)) return originalName
    const dotIdx = originalName.lastIndexOf('.')
    const baseName = dotIdx > 0 ? originalName.slice(0, dotIdx) : originalName
    const ext = dotIdx > 0 ? originalName.slice(dotIdx) : ''
    let counter = 1
    while (existingNames.includes(`${baseName}-${counter}${ext}`)) {
      counter++
    }
    return `${baseName}-${counter}${ext}`
  }, [])

  /** 将 File 对象列表添加为待发送附件 */
  const addFilesAsAttachments = React.useCallback(async (files: File[]): Promise<void> => {
    // 收集已有的 pending 文件名，用于去重
    const usedNames: string[] = pendingFilesRef.current.map((f) => f.filename)

    for (const file of files) {
      try {
        const base64 = await fileToBase64(file)
        const previewUrl = file.type.startsWith('image/') ? URL.createObjectURL(file) : undefined
        const uniqueFilename = makeUniqueFilename(file.name, usedNames)
        usedNames.push(uniqueFilename)

        const pending: AgentPendingFile = {
          id: `pending-${Date.now()}-${Math.random().toString(36).slice(2)}`,
          filename: uniqueFilename,
          mediaType: file.type || 'application/octet-stream',
          size: file.size,
          previewUrl,
        }

        if (!window.__pendingAgentFileData) {
          window.__pendingAgentFileData = new Map<string, string>()
        }
        window.__pendingAgentFileData.set(pending.id, base64)

        setPendingFiles((prev) => [...prev, pending])
      } catch (error) {
        console.error('[AgentView] 添加附件失败:', error)
      }
    }
  }, [makeUniqueFilename, setPendingFiles])

  /** 打开文件选择对话框 */
  const handleOpenFileDialog = React.useCallback(async (): Promise<void> => {
    try {
      const result = await openFileDialog()
      if (result.files.length === 0) return

      for (const fileInfo of result.files) {
        const previewUrl = fileInfo.mediaType.startsWith('image/')
          ? `data:${fileInfo.mediaType};base64,${fileInfo.data}`
          : undefined

        const pending: AgentPendingFile = {
          id: `pending-${Date.now()}-${Math.random().toString(36).slice(2)}`,
          filename: fileInfo.filename,
          mediaType: fileInfo.mediaType,
          size: fileInfo.size,
          previewUrl,
        }

        if (!window.__pendingAgentFileData) {
          window.__pendingAgentFileData = new Map<string, string>()
        }
        window.__pendingAgentFileData.set(pending.id, fileInfo.data)

        setPendingFiles((prev) => [...prev, pending])
      }
    } catch (error) {
      console.error('[AgentView] 文件选择对话框失败:', error)
    }
  }, [setPendingFiles])

  /** 移除待发送文件 */
  const handleRemoveFile = React.useCallback((id: string): void => {
    setPendingFiles((prev) => {
      const file = prev.find((f) => f.id === id)
      if (file?.previewUrl?.startsWith('blob:')) {
        URL.revokeObjectURL(file.previewUrl)
      }
      window.__pendingAgentFileData?.delete(id)
      return prev.filter((f) => f.id !== id)
    })
  }, [setPendingFiles])

  /** 粘贴文件处理 */
  const handlePasteFiles = React.useCallback((files: File[]): void => {
    addFilesAsAttachments(files)
  }, [addFilesAsAttachments])

  const handleSegmentFinalized = React.useCallback((text: string): void => {
    const editor = composerEditorRef.current
    if (editor && editor.isFocused) {
      editor.commands.insertContent(text)
    } else {
      setInputContent(smartJoin(inputContent, text))
    }
    // 转写文本落地后聚焦输入框（光标置末），让用户直接回车发送。
    editor?.commands.focus('end')
  }, [composerEditorRef, inputContent, setInputContent])

  /** 粘贴超长文本 → 转为附件 */
  const handlePasteLongText = React.useCallback((text: string): void => {
    const file = createClipboardTextFile(text)
    addFilesAsAttachments([file])
    toast.success('已将超长文本转为附件', { description: file.name })
  }, [addFilesAsAttachments])

  /** 拖放处理 */
  const handleDragOver = React.useCallback((e: React.DragEvent): void => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragOver(true)
  }, [])

  const handleDragLeave = React.useCallback((e: React.DragEvent): void => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragOver(false)
  }, [])

  const handleDrop = React.useCallback(async (e: React.DragEvent): Promise<void> => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragOver(false)

    const droppedFiles = Array.from(e.dataTransfer.files)
    if (droppedFiles.length === 0) return

    // 通过 preload 的 webUtils.getPathForFile 获取真实路径
    const pathMap = new Map<string, File>()
    const paths: string[] = []
    for (const f of droppedFiles) {
      try {
        const p = getPathForFile(f)
        if (p) {
          paths.push(p)
          pathMap.set(p, f)
        }
      } catch { /* 无法获取路径时忽略 */ }
    }

    if (paths.length > 0) {
      try {
        // 通过主进程检测目录 vs 文件
        const { directories, files: filePaths } = await checkPathsType(paths)

        // Phase 2: real attach_session_directory.
        for (const dirPath of directories) {
          try {
            const updated = await attachSessionDirectory(sessionId, dirPath)
            setSessionAttachedMap((prev) => {
              const map = new Map(prev)
              map.set(sessionId, updated)
              return map
            })
            const dirName = dirPath.split('/').pop() || dirPath
            toast.success(`已附加目录: ${dirName}`)
          } catch (err) {
            console.error('[AgentView] attach directory failed', err)
          }
        }

        // 普通文件作为附件
        const regularFiles = filePaths.map((p: string) => pathMap.get(p)!).filter(Boolean)
        if (regularFiles.length > 0) {
          addFilesAsAttachments(regularFiles)
        }
      } catch (error) {
        console.error('[AgentView] 路径检测失败，回退处理:', error)
        addFilesAsAttachments(droppedFiles)
      }
    } else {
      // 无路径信息：回退，所有项按普通文件处理
      addFilesAsAttachments(droppedFiles)
    }
  }, [sessionId, addFilesAsAttachments])

  /** ModelSelector 选择回调 */

  /** 发送消息 */
  const handleSend = React.useCallback(async (): Promise<void> => {
    const text = inputContent.trim()
    // 如果输入为空但有建议，使用建议内容
    const effectiveText = text || suggestion || ''
    if ((!effectiveText && pendingFiles.length === 0) || !activeProviderModel) return

    // /compact 输入框拦截：与徽章按钮共用一条路径，确保 UI 上的合成
    // 消息气泡 + isCompacting 旋转动画一致出现。否则后端会跑通但前端
    // 没有任何视觉反馈（见 PR #99 dogfood 反馈）。
    if (effectiveText === '/compact' && pendingFiles.length === 0) {
      setInputContent('')
      setComposerHasText(false)
      handleCompactRef.current?.()
      return
    }

    // Agent 正在 streaming —— 走 Codex 风格的队列机制，而不是立刻打断。
    // 用户的消息进入 composer 上方的 QueuedMessagesBanner，他们可以：
    //   - 点"引导"立即注入（对应原来的 interrupt:true 路径）
    //   - 点"编辑"把消息回填到 composer 继续编辑
    //   - 点"删除"丢弃
    // 默认行为：等当前 turn 自然结束后 FIFO 自动 dispatch。
    if (streaming) {
      // 队列阶段不接受附件（保持与旧 streaming-append 一致）
      if (pendingFiles.length > 0) {
        toast.info('Agent 运行中暂不支持排队带附件的消息', {
          description: '请等待完成后再发送附件，或先撤除附件仅发送文本',
        })
        return
      }

      // Generate uuid up-front so we can pass it to agentFollowUp for
      // optimistic banner dequeue (backend persists with a server-side uuid
      // that does not round-trip, so we use the fallback approach).
      const followUpUuid = crypto.randomUUID()
      store.set(agentQueuedMessagesMapAtom, (prev) => {
        if (!effectiveText.trim()) return prev
        const existing = prev[sessionId] ?? []
        return {
          ...prev,
          [sessionId]: [
            ...existing,
            { id: followUpUuid, text: effectiveText, queuedAt: Date.now() },
          ],
        }
      })

      // Tell the backend about the follow-up immediately. The backend's
      // FollowUpQueue owns serialization; the frontend must NOT auto-flush
      // on completion. Card stays visible until backend emits agent:queued-consumed.
      agentFollowUp({ sessionId, userMessage: effectiveText, uuid: followUpUuid })
        .catch((error: unknown) => {
          console.error('[AgentView] agentFollowUp failed:', error)
          toast.error('队列消息发送失败', { description: String(error) })
        })

      // 清空输入框 — banner 已经接管显示
      setInputContent('')
      setInputHtmlContent('')
      setComposerHasText(false)
      setPromptSuggestions((prev) => {
        if (!prev.has(sessionId)) return prev
        const map = new Map(prev)
        map.delete(sessionId)
        return map
      })
      return
    }

    // 清除当前会话的错误消息
    setAgentStreamErrors((prev) => {
      if (!prev.has(sessionId)) return prev
      const map = new Map(prev)
      map.delete(sessionId)
      return map
    })

    // 清除当前会话的提示建议
    setPromptSuggestions((prev) => {
      if (!prev.has(sessionId)) return prev
      const map = new Map(prev)
      map.delete(sessionId)
      return map
    })

    // 清除当前会话的轮次徽章（记忆召回、主动学习、技能召回）
    store.set(memoryRecallEventAtom, (prev) => {
      const next = new Map(prev)
      next.delete(sessionId)
      next.delete('__global__')
      return next
    })
    store.set(proactiveLearningEventsAtom, (prev) =>
      prev.filter((ev) => ev.sessionId !== sessionId)
    )
    store.set(skillRecallsMapAtom, (prev) => {
      const next = new Map(prev)
      next.delete(sessionId)
      return next
    })

    // 1. 如果有 pending 文件，先保存到 session 目录
    let fileReferences = ''
    if (pendingFiles.length > 0) {
      const workspace = workspaces.find((w) => w.id === currentWorkspaceId)
      if (workspace) {
        // 区分：已有 sourcePath 的文件（从侧面板添加）直接引用，其余需要保存
        const existingFiles = pendingFiles.filter((f) => f.sourcePath)
        const newFiles = pendingFiles.filter((f) => !f.sourcePath)

        const allRefs: Array<{ filename: string; targetPath: string }> = []

        // 已有路径的文件直接引用
        for (const f of existingFiles) {
          allRefs.push({ filename: f.filename, targetPath: f.sourcePath! })
        }

        // 新上传的文件保存到 session 目录
        if (newFiles.length > 0) {
          const filesToSave = newFiles.map((f) => ({
            filename: f.filename,
            data: window.__pendingAgentFileData?.get(f.id) || '',
          }))
          try {
            const saved = await saveFilesToAgentSession({
              workspaceSlug: workspace.id,
              sessionId,
              files: filesToSave,
            })
            allRefs.push(...saved)
          } catch (error) {
            console.error('[AgentView] 保存附件到 session 失败:', error)
          }
        }

        if (allRefs.length > 0) {
          const refs = allRefs.map((f) => `- ${f.filename}: ${f.targetPath}`).join('\n')
          fileReferences += `<attached_files>\n${refs}\n</attached_files>\n\n`
        }
      }

      // 清理
      for (const f of pendingFiles) {
        if (f.previewUrl?.startsWith('blob:')) URL.revokeObjectURL(f.previewUrl)
        window.__pendingAgentFileData?.delete(f.id)
      }
      setPendingFiles([])
    }

    // 2. 构建最终消息
    const finalMessage = fileReferences + effectiveText

    // 防御性快照：将当前流式 assistant 内容保存到消息列表
    // 避免重置流式状态时丢失前一轮回复（竞态场景：complete 事件到达但 STREAM_COMPLETE 尚未到达）
    const prevStream = store.get(agentStreamingStatesAtom).get(sessionId)
    if (prevStream && prevStream.content && !prevStream.running) {
      setMessages((prev) => {
        // 仅在最后一条不是 assistant 消息时追加（避免重复）
        const lastMsg = prev[prev.length - 1]
        if (lastMsg?.role === 'assistant') return prev
        return [...prev, {
          id: `snapshot-${Date.now()}`,
          role: 'assistant' as const,
          content: prevStream.content,
          createdAt: Date.now(),
          model: prevStream.model,
        }]
      })
    }

    // 清除打断状态（上一轮的打断标记不再显示）
    store.set(stoppedByUserSessionsAtom, (prev: Set<string>) => {
      if (!prev.has(sessionId)) return prev
      const next = new Set(prev)
      next.delete(sessionId)
      return next
    })

    // 取消 draft 标记，让会话出现在侧边栏
    setDraftSessionIds((prev: Set<string>) => {
      if (!prev.has(sessionId)) return prev
      const next = new Set(prev)
      next.delete(sessionId)
      return next
    })

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
        model: agentModelId || undefined,
        startedAt: streamStartedAt,
        inputTokens: existing?.inputTokens,
        contextWindow: existing?.contextWindow,
      })
      return map
    })

    // 乐观更新：立即显示用户消息
    const tempUserMsg: AgentMessage = {
      id: `temp-${Date.now()}`,
      role: 'user',
      content: finalMessage,
      createdAt: Date.now(),
    }
    setMessages((prev) => [...prev, tempUserMsg])

    const input: AgentSendInput = {
      sessionId,
      userMessage: finalMessage,
      channelId: agentChannelId ?? activeProviderModel?.providerId ?? '',
      modelId: activeProviderModel?.modelId || agentModelId || undefined,
      workspaceId: currentWorkspaceId || undefined,
      startedAt: streamStartedAt,
      strategy: currentStrategy !== 'balanced' ? currentStrategy : undefined,
      ...(attachedDirs.length > 0 && { additionalDirectories: attachedDirs }),
      // 解析用户消息中的 Skill/MCP 引用，传递结构化元数据给后端
      ...(() => {
        const skills = [...effectiveText.matchAll(/\/skill:(\S+)/g)].map(m => m[1]).filter(Boolean) as string[]
        const mcps = [...effectiveText.matchAll(/#mcp:(\S+)/g)].map(m => m[1]).filter(Boolean) as string[]
        return {
          ...(skills.length > 0 && { mentionedSkills: skills }),
          ...(mcps.length > 0 && { mentionedMcpServers: mcps }),
        }
      })(),
    }

    setInputContent('')
    setInputHtmlContent('')
    setComposerHasText(false)

    sendAgentMessage(input)
      .then(() => {
        // Reload messages from DB so the persisted assistant reply appears.
        // Note: streaming state is managed entirely by chat:stream-complete / chat:stream-error events.
        // Setting running=false here would race with those events and kill the streaming display.
        getAgentSessionMessages(sessionId)
          .then((msgs: any[]) => {
            if (msgs.length === 0) return
            // IMPORTANT: pass through the full message shape from the backend
            // (id, role, content, createdAt, reasoning, toolActivities, model, …).
            // A previous version of this code stripped everything except id/role/
            // content/createdAt, which made historical thinking blocks and tool
            // call cards vanish from earlier turns the moment a new message was
            // sent — they only re-appeared after a tab switch (which hits the
            // initial-load path that does setMessages(msgs) cleanly).
            setMessages(msgs as AgentMessage[])
          })
          .catch(console.error)
      })
      .catch((error: unknown) => {
        console.error('[AgentView] 发送消息失败:', error)
        setStreamingStates((prev) => {
          const current = prev.get(sessionId)
          if (!current) return prev
          const map = new Map(prev)
          map.set(sessionId, { ...current, running: false })
          return map
        })
      })
  }, [inputContent, pendingFiles, sessionId, activeProviderModel, agentChannelId, agentModelId, currentWorkspaceId, workspaces, streaming, suggestion, currentStrategy, store, setStreamingStates, setPendingFiles, setAgentStreamErrors, setPromptSuggestions, setInputContent, setLiveMessagesMap, setMessages])

  // ── Codex-style message queue (Bundle 2-A) ──────────────────────
  //
  // When the agent is streaming, new user messages go into a per-session
  // queue surfaced as a banner above the composer. The queued message
  // doesn't auto-fire — user decides:
  //   - 引导 (steer)  → handleSteerQueued: send now + interrupt
  //   - 编辑 (edit)   → handleEditQueued: pop back to composer
  //   - 删除 (trash)  → handleDeleteQueued: discard
  //
  // After streaming finishes naturally, an effect below pops the oldest
  // queued message and auto-dispatches it (no interrupt — current turn
  // already done).
  const queuedMessages = useAtomValue(agentQueuedMessagesMapAtom)
  const currentQueue = React.useMemo(
    () => queuedMessages[sessionId] ?? [],
    [queuedMessages, sessionId],
  )

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
  }, [sessionId, agentChannelId, agentModelId, currentWorkspaceId, streaming, setStreamingStates, store])

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

  const allAskUserRequests = useAtomValue(allPendingAskUserRequestsAtom)
  const allExitPlanRequests = useAtomValue(allPendingExitPlanRequestsAtom)
  const agentStatusBarEnabled = useAtomValue(agentStatusBarEnabledAtom)
  const hasBannerOverlay =
    (allAskUserRequests.get(sessionId)?.length ?? 0) > 0 ||
    (allExitPlanRequests.get(sessionId)?.length ?? 0) > 0

  const hasTextInput = inputContent.trim().length > 0
  const canSend = (hasTextInput || pendingFiles.length > 0 || !!suggestion) && activeProviderModel !== null && (!streaming || hasTextInput)

  return (
    <>
    <AgentSessionProvider sessionId={sessionId}>
      {/* 主内容区域 */}
      <div className="flex flex-col h-full flex-1 min-w-0 relative">
        {/* Agent Header */}
        <AgentHeader sessionId={sessionId} />

        {/* Automation run banner — shows when the session was started by an
            automation trigger (origin starts with "automation:"). */}
        <AutomationRunBanner
          metadataJson={sessions.find((s) => s.id === sessionId)?.metadataJson}
        />

        {/* 消息区域 */}
        <AgentMessages
          sessionId={sessionId}
          /* sessionModelId 回退链：per-session map → 全局默认 → localStorage 活跃模型。
             历史消息 (M2-J 之前) 没在 DB 存 model，必须靠这里兜底显示头像 + caption。 */
          sessionModelId={agentModelId || activeProviderModel?.modelId || undefined}
          messages={messages}
          messagesLoaded={messagesLoaded}
          streaming={streaming}
          streamState={streamState}
          liveMessages={liveMessages}
          sessionPath={sessionPath}
          attachedDirs={attachedDirs}
          stoppedByUser={stoppedByUser}
          onRetry={handleRetry}
          onRetryInNewSession={handleRetryInNewSession}
          onFork={handleFork}
          onRewind={handleRewindRequest}
          onCompact={handleCompact}
        />

        {/* Browser preview overlay — positioned absolute within the relative outer container
            so it floats over the scroll area without scrolling with content */}
        <BrowserPreviewOverlay sessionId={sessionId} />

        {/* outer_timeout 内联错误块：显示超时提示 + 重试按钮 */}
        {agentError && agentError.kind === 'outer_timeout' && (
          <div className="mx-4 mb-2 rounded-md border border-destructive/40 bg-destructive/[0.04] p-3 animate-in fade-in slide-in-from-bottom-1 duration-200">
            <div className="flex items-start gap-2">
              <AlertTriangle className="size-4 text-destructive shrink-0 mt-0.5" />
              <div className="flex-1 text-sm text-foreground/85">
                <div>{agentError.message}</div>
                {agentError.timeoutSecs != null && (
                  <div className="mt-1 text-xs text-muted-foreground">
                    提示：可在 设置 → 高级 中调整 Agent 循环超时（当前 {agentError.timeoutSecs}s）。
                  </div>
                )}
              </div>
              <Button variant="outline" size="sm" onClick={handleRetry}>
                重试
              </Button>
            </div>
          </div>
        )}

        {/* 权限请求横幅 */}
        <PermissionBanner sessionId={sessionId} />

        {/* Bundle 27-A — Heartbeat / stall / interrupted-reply recovery.
            Self-contained: renders nothing when there's no live run,
            no stall, and no recovery payload pending. Owns its own
            event listeners (agent:heartbeat / agent:stalled /
            agent:stall-recovered / agent:interrupted-recovered). */}
        <AgentHeartbeatBanner sessionId={sessionId} />

        {/* Plan 模式自动建议横幅 — advisory (not blocking) */}
        <PlanModeSuggestBanner sessionId={sessionId} />

        {/* AskUserQuestion 交互式问答横幅 */}
        <AskUserBanner sessionId={sessionId} />

        {/* Plan 模式指示条 */}
        {isPlanMode && (
          <div className="mx-4 mb-2 flex items-center gap-2 px-3 py-2 rounded-lg bg-primary/5 text-primary text-sm animate-in fade-in slide-in-from-bottom-1 duration-200">
            <MapIcon className="size-4 animate-pulse" />
            <span className="font-medium">Agent 正在规划中...</span>
            <span className="text-xs text-muted-foreground">完成后将请求你的审批</span>
          </div>
        )}

        {/* ExitPlanMode 计划审批横幅 */}
        <ExitPlanModeBanner sessionId={sessionId} />

        {/* 任务执行状态条 — sticky 在输入栏正上方，agent 跑任务时常驻可见。
            默认关闭，可在 设置 → 外观 中开启。 */}
        {agentStatusBarEnabled && <AgentStatusBar sessionId={sessionId} />}

        {/* 输入区域 — 交互横幅显示时隐藏，由横幅替代 */}
        {!hasBannerOverlay && (
        <div className="px-2.5 pb-2.5 md:px-[18px] md:pb-[18px]" data-input-mode="agent">
          {/* Bundle 2-A — Codex / Claude App style queued-messages banner.
              Sits as a SIBLING above the composer card (not inside it),
              so the queue visually stacks with the input like in the
              Claude app and codex CLI. The component itself returns null
              when no messages are queued so this branch has zero cost on
              the regular hot path. */}
          <QueuedMessagesBanner
            messages={currentQueue}
            onSteer={handleSteerQueued}
            onEdit={handleEditQueued}
            onDelete={handleDeleteQueued}
          />

          <div
            className={cn(
              'relative rounded-[17px] border-[0.5px] border-border bg-background/70 backdrop-blur-sm transition-all duration-200',
              isPlanMode && !isDragOver && 'plan-mode-border',
              isDragOver && 'border-[2px] border-dashed border-[#2ecc71] bg-[#2ecc71]/[0.03]'
            )}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDrop}
          >
            {/* Pet anchored to the entire composer card's top — sits above all
                inner banners (model warning, attachment preview, agent suggestion,
                sticky user message, etc.). bottom:100% references this card's top. */}
            <PetWidget />
            {isPlanMode && !isDragOver && <PlanModeDashedBorder />}
            {/* 未配置模型提示 */}
            {!activeProviderModel && (
              <div className="flex items-center gap-2 px-4 py-2 text-sm text-amber-600 dark:text-amber-400">
                <Settings size={14} />
                <span>请在下方工具栏选择模型</span>
              </div>
            )}

            {/* 附件预览区域 */}
            {pendingFiles.length > 0 && (
              <div className="flex flex-wrap gap-2 px-3 pt-2.5 pb-1.5">
                {pendingFiles.map((file) => (
                  <AttachmentPreviewItem
                    key={file.id}
                    filename={file.filename}
                    mediaType={file.mediaType}
                    previewUrl={file.previewUrl}
                    onRemove={() => handleRemoveFile(file.id)}
                  />
                ))}
              </div>
            )}

            {/* Agent 建议提示 */}
            {suggestion && !streaming && (
              <div className="px-3 pt-2.5 pb-1.5">
                <button
                  type="button"
                  className="group flex items-start gap-2 w-full rounded-lg border border-dashed border-primary/30 bg-primary/[0.03] px-3 py-2.5 text-left text-sm transition-colors hover:border-primary/50 hover:bg-primary/[0.06]"
                  onClick={handleSend}
                >
                  <Sparkles className="size-4 shrink-0 mt-0.5 text-primary/60 group-hover:text-primary/80" />
                  <span className="flex-1 min-w-0 text-foreground/80 group-hover:text-foreground line-clamp-3">{suggestion}</span>
                  <X
                    className="size-3.5 shrink-0 mt-0.5 text-muted-foreground/40 hover:text-foreground transition-colors"
                    onClick={(e) => {
                      e.stopPropagation()
                      setPromptSuggestions((prev) => {
                        if (!prev.has(sessionId)) return prev
                        const map = new Map(prev)
                        map.delete(sessionId)
                        return map
                      })
                    }}
                  />
                </button>
              </div>
            )}

            <div className="relative">
              <RichTextInput
                value={inputContent}
                onChange={handleComposerChange}
                onFocus={handleComposerFocus}
                onBlur={handleComposerBlur}
                onSubmit={handleSend}
                onPasteFiles={handlePasteFiles}
                onPasteLongText={handlePasteLongText}
                placeholder={
                  activeProviderModel
                    ? sendWithCmdEnter
                      ? '输入消息... (⌘/Ctrl+Enter 发送，Enter 换行，@ 引用文件，/ 调用 Skill，# 调用 MCP)'
                      : '输入消息... (Enter 发送，Shift+Enter 换行，@ 引用文件，/ 调用 Skill，# 调用 MCP)'
                    : '请先在下方工具栏选择模型...'
                }
                disabled={!activeProviderModel}
                autoFocusTrigger={sessionId}
                collapsible
                workspacePath={sessionPath}
                workspaceSlug={workspaceSlug}
                attachedDirs={allAttachedDirs}
                htmlValue={inputHtmlContent}
                onHtmlChange={setInputHtmlContent}
                sendWithCmdEnter={sendWithCmdEnter}
                editorRef={composerEditorRef}
                onKeyDownIntercept={(e) =>
                  mentionControllerRef.current?.handleKeyDown(e) ?? false}
              />
              <ComposerMentionController
                ref={mentionControllerRef}
                editorRef={composerEditorRef}
                value={inputContent}
                setValue={setInputContent}
                sessionId={sessionId}
                disabled={!activeProviderModel}
              />
            </div>

            {/* Footer 工具栏 */}
            <div className="flex items-center justify-between px-2 py-1 h-[48px] gap-4">
              <div className="flex items-center gap-1.5 flex-1 min-w-0">
                <ProviderModelSelector />
                <PermissionModeSelector sessionId={sessionId} />
                {/* 思考模式切换 + 展开偏好 */}
                <AgentThinkingPopover
                  agentThinking={agentThinking}
                  onToggle={() => {
                    const next = agentThinking?.type === 'adaptive'
                      ? { type: 'disabled' as const }
                      : { type: 'adaptive' as const }
                    setAgentThinking(next)
                    updateSettings({ agentThinking: next })
                  }}
                />
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      className="size-[36px] rounded-full text-foreground/60 hover:text-foreground"
                      onClick={handleOpenFileDialog}
                    >
                      <Paperclip className="size-5" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent side="top">
                    <p>添加附件</p>
                  </TooltipContent>
                </Tooltip>
                <ContextUsageBadge
                  inputTokens={contextStatus.inputTokens}
                  outputTokens={contextStatus.outputTokens}
                  cacheReadTokens={contextStatus.cacheReadTokens}
                  cacheCreationTokens={contextStatus.cacheCreationTokens}
                  costUsd={contextStatus.costUsd}
                  contextWindow={contextStatus.contextWindow}
                  skillsTokens={contextStatus.skillsTokens}
                  isCompacting={contextStatus.isCompacting}
                  isProcessing={streaming}
                  onCompact={handleCompact}
                />
                <StrategyPresetSelector sessionId={sessionId} />
                <AutoPreviewPopover />
                {/* <FeishuNotifyToggle sessionId={sessionId} /> */}

                <GitChipsRow />
                <SpeechButton
                  composer="agent"
                  onShowDownloadDialog={() => setFirstRunOpen(true)}
                />
              </div>

              <div className="flex items-center gap-1.5">
                {streaming && !hasTextInput ? (
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="size-[36px] rounded-full text-destructive hover:!text-[hsl(0,75%,55%)] hover:!bg-[var(--stop-hover-bg)]"
                        onClick={handleStop}
                      >
                        <Square className="size-[16px]" fill="currentColor" strokeWidth={0} />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent side="top">
                      <p>停止 Agent ({getAcceleratorDisplay(getActiveAccelerator('stop-generation'))})</p>
                    </TooltipContent>
                  </Tooltip>
                ) : (
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className={cn(
                      'size-[36px] rounded-full',
                      canSend
                        ? 'text-primary hover:bg-primary/10'
                        : 'text-foreground/30 cursor-not-allowed'
                    )}
                    onClick={handleSend}
                    disabled={!canSend}
                  >
                    <CornerDownLeft className="size-[22px]" />
                  </Button>
                )}
              </div>
            </div>
          </div>
        </div>
        )}
      </div>
    </AgentSessionProvider>

    {/* 回退确认弹窗 */}
    <AlertDialog
      open={rewindTargetUuid !== null}
      onOpenChange={(v) => { if (!v) setRewindTargetUuid(null) }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>确认回退</AlertDialogTitle>
          <AlertDialogDescription>
            回退将截断该消息之后的所有对话，并恢复文件到该时刻的状态。此操作不可撤销，确定要回退吗？
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>取消</AlertDialogCancel>
          <AlertDialogAction
            onClick={handleRewindConfirm}
            className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
          >
            回退
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
    <SttModal composer="agent" onSegmentFinalized={handleSegmentFinalized} />
    <FirstRunDialog
      open={firstRunOpen}
      onOpenChange={setFirstRunOpen}
      onReady={() => { window.dispatchEvent(new CustomEvent('uclaw:stt-start-after-ready')) }}
    />
    </>
  )
}
