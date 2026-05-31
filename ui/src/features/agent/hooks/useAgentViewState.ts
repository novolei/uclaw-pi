/**
 * useAgentViewState — the AgentView shell's complete state/derivation/wiring
 * engine, extracted VERBATIM from the AgentView component body during the
 * features/agent migration split so the shell file stays a thin layout-only
 * component (≤300 lines).
 *
 * This owns every atom read, every derived value, every small callback, the
 * STT-status mount effect, and the four sub-hooks (useAgentSession /
 * useAgentActions / useAgentComposer / useAgentQueue). It returns a single
 * view-model object the shell renders. Behavior is unchanged — same hooks in the
 * same order, same deps, same derivations; only relocated out of the component
 * body. IPC routes through the agent bridge (no `@tauri-apps/api` here).
 */

import * as React from 'react'
import { useAtom, useAtomValue, useSetAtom, useStore } from 'jotai'
import type { Editor } from '@tiptap/core'
import { modelStatusAtom } from '@/atoms/stt-atoms'
import { type ComposerMentionControllerHandle } from '@/components/composer/ComposerMentionController'
import {
  agentStreamingStatesAtom,
  agentChannelIdAtom,
  agentModelIdAtom,
  agentSessionChannelMapAtom,
  agentSessionModelMapAtom,
  agentSessionStrategyMapAtom,
  currentAgentWorkspaceIdAtom,
  agentPendingFilesAtom,
  agentWorkspacesAtom,
  agentStreamErrorsAtom,
  agentSessionDraftsAtom,
  agentSessionDraftHtmlAtom,
  agentPromptSuggestionsAtom,
  agentSessionsAtom,
  liveMessagesMapAtom,
  agentThinkingAtom,
  stoppedByUserSessionsAtom,
  agentPlanModeSessionsAtom,
  agentSessionPathMapAtom,
  allPendingAskUserRequestsAtom,
  allPendingExitPlanRequestsAtom,
  workspaceAttachedDirsMapAtom,
  agentSessionAttachedDirsMapAtom,
  composerFocusedAtom,
  composerHasTextAtom,
} from '@/atoms/agent-atoms'
import type { AgentContextStatus, AgentStreamErrorPayload } from '@/atoms/agent-atoms'
import type { AgentSessionMeta } from '@/lib/agent-types'
import { agentQueuedMessagesMapAtom } from '@/atoms/agent-queue-messages'
import type { QueuedAgentMessage } from '@/atoms/agent-queue-messages'
import { activeProviderModelAtom } from '@/atoms/active-model'
import type { ActiveProviderModel } from '@/atoms/active-model'
import { channelsAtom } from '@/atoms/chat-atoms'
import { workspacesAtom } from '@/atoms/workspace'
import { draftSessionIdsAtom } from '@/atoms/draft-session-atoms'
import { sendWithCmdEnterAtom } from '@/atoms/shortcut-atoms'
import { agentStatusBarEnabledAtom } from '@/atoms/ui-preferences'
import type { AgentMessage, AgentPendingFile } from '@/lib/agent-types'
import type { AgentStreamState } from '@/atoms/agent-atoms'
import { updateSettings, getSttModelStatus } from '@/lib/bridge/agent'

import { useAgentSession } from './useAgentSession'
import { useAgentComposer } from './useAgentComposer'
import { useAgentQueue } from './useAgentQueue'
import { useAgentActions } from './useAgentActions'

export interface AgentViewState {
  sessionId: string
  // message list
  messages: AgentMessage[]
  messagesLoaded: boolean
  streaming: boolean
  streamState: AgentStreamState | undefined
  liveMessages: any[]
  stoppedByUser: boolean
  sessionPath: string | null
  attachedDirs: string[]
  sessionModelId: string | undefined
  sessions: AgentSessionMeta[]
  // banners / error
  agentError: AgentStreamErrorPayload | null
  isPlanMode: boolean
  agentStatusBarEnabled: boolean
  hasBannerOverlay: boolean
  // composer
  inputContent: string
  inputHtmlContent: string
  handleComposerChange: (v: string) => void
  handleComposerFocus: () => void
  handleComposerBlur: () => void
  setInputHtmlContent: (html: string) => void
  setInputContent: (v: string) => void
  activeProviderModel: ActiveProviderModel | null
  sendWithCmdEnter: boolean
  canSend: boolean
  hasTextInput: boolean
  workspaceSlug: string | null
  allAttachedDirs: string[]
  pendingFiles: AgentPendingFile[]
  suggestion: string | null
  composerEditorRef: React.MutableRefObject<Editor | null>
  mentionControllerRef: React.MutableRefObject<ComposerMentionControllerHandle | null>
  agentThinking: import('@/lib/proma-types').ThinkingConfig | undefined
  contextStatus: AgentContextStatus
  currentQueue: QueuedAgentMessage[]
  // queue handlers
  handleSteerQueued: (msg: QueuedAgentMessage) => void
  handleEditQueued: (msg: QueuedAgentMessage) => void
  handleDeleteQueued: (msg: QueuedAgentMessage) => void
  // composer handlers
  handleSend: () => void
  handlePasteFiles: (files: File[]) => void
  handlePasteLongText: (text: string) => void
  handleDragOver: (e: React.DragEvent) => void
  handleDragLeave: (e: React.DragEvent) => void
  handleDrop: (e: React.DragEvent) => void
  isDragOver: boolean
  handleOpenFileDialog: () => void
  handleRemoveFile: (id: string) => void
  handleSegmentFinalized: (text: string) => void
  handleDismissSuggestion: () => void
  handleToggleThinking: () => void
  // action handlers
  handleStop: () => void
  handleCompact: () => void
  handleRetry: () => void
  handleRetryInNewSession: () => Promise<void>
  handleFork: (upToMessageUuid: string) => Promise<void>
  handleRewindRequest: (assistantMessageUuid: string) => void
  rewindTargetUuid: string | null
  setRewindTargetUuid: React.Dispatch<React.SetStateAction<string | null>>
  handleRewindConfirm: () => Promise<void>
  // STT first-run modal
  firstRunOpen: boolean
  setFirstRunOpen: React.Dispatch<React.SetStateAction<boolean>>
}

export function useAgentViewState(sessionId: string): AgentViewState {
  const setStreamingStates = useSetAtom(agentStreamingStatesAtom)
  const streamingStates = useAtomValue(agentStreamingStatesAtom)
  const streamState = streamingStates.get(sessionId)
  const streaming = streamState?.running ?? false
  const stoppedByUserSessions = useAtomValue(stoppedByUserSessionsAtom)
  const sendWithCmdEnter = useAtomValue(sendWithCmdEnterAtom)
  const stoppedByUser = stoppedByUserSessions.has(sessionId)
  const liveMessagesMap = useAtomValue(liveMessagesMapAtom)
  // 稳定化空数组引用，避免 ?? [] 每次创建新引用导致下游 useMemo 链不必要重算
  const liveMessages = liveMessagesMap.get(sessionId) ?? []
  // Per-session 渠道/模型配置（优先读 session map，回退到全局默认值）
  const sessionChannelMap = useAtomValue(agentSessionChannelMapAtom)
  const sessionModelMap = useAtomValue(agentSessionModelMapAtom)
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
  const [pendingFiles, setPendingFiles] = useAtom(agentPendingFilesAtom)
  const workspaces = useAtomValue(agentWorkspacesAtom)

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
  // Phase 2: real atom subscriptions for attached dirs (workspace + session levels).
  const wsAttachedMap = useAtomValue(workspaceAttachedDirsMapAtom)
  const sessionAttachedMap = useAtomValue(agentSessionAttachedDirsMapAtom)
  const setSessionAttachedMap = useSetAtom(agentSessionAttachedDirsMapAtom)
  const attachedDirs = sessionAttachedMap.get(sessionId) ?? []
  const wsAttachedDirs = currentWorkspaceId ? (wsAttachedMap.get(currentWorkspaceId) ?? []) : []

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
  const sessionPath = sessionPathMap.get(sessionId) ?? null

  // STT state
  const [firstRunOpen, setFirstRunOpen] = React.useState(false)
  const setModelStatus = useSetAtom(modelStatusAtom)

  // Query model status on mount so SpeechButton can show indicator dot.
  React.useEffect(() => {
    void getSttModelStatus()
      .then((status) => {
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
  const composerEditorRef = React.useRef<Editor | null>(null)
  const mentionControllerRef = React.useRef<ComposerMentionControllerHandle | null>(null)

  // 渠道已选但模型未选时，自动选择第一个可用模型
  const globalChannels = useAtomValue(channelsAtom)

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

  // ── Session / event / streaming subscriptions + message state ──
  const { messages, setMessages, messagesLoaded } = useAgentSession({
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
  })

  // ── stop / compact / retry / fork / rewind + error wiring ──
  const {
    handleStop,
    handleCompact,
    handleCompactRef,
    handleRetry,
    handleRetryInNewSession,
    handleFork,
    rewindTargetUuid,
    setRewindTargetUuid,
    handleRewindRequest,
    handleRewindConfirm,
  } = useAgentActions({
    sessionId,
    messages,
    setMessages,
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
  })

  // ── composer attachment + paste/drop + submit engine (critical user path) ──
  const composer = useAgentComposer({
    sessionId,
    inputContent,
    setInputContent,
    setInputHtmlContent,
    setComposerHasText,
    pendingFiles,
    setPendingFiles,
    composerEditorRef,
    activeProviderModel,
    agentChannelId,
    agentModelId,
    currentWorkspaceId,
    workspaces,
    streaming,
    suggestion,
    currentStrategy,
    attachedDirs,
    store,
    setStreamingStates,
    setMessages,
    setAgentStreamErrors,
    setPromptSuggestions,
    setDraftSessionIds,
    setSessionAttachedMap,
    handleCompactRef,
  })

  // ── Codex-style message queue (Bundle 2-A) ──
  const queuedMessages = useAtomValue(agentQueuedMessagesMapAtom)
  const currentQueue = React.useMemo(
    () => queuedMessages[sessionId] ?? [],
    [queuedMessages, sessionId],
  )
  const { handleSteerQueued, handleEditQueued, handleDeleteQueued } = useAgentQueue({
    sessionId,
    activeProviderModel,
    agentModelId,
    store,
    setStreamingStates,
    setInputContent,
    setComposerHasText,
  })

  const allAskUserRequests = useAtomValue(allPendingAskUserRequestsAtom)
  const allExitPlanRequests = useAtomValue(allPendingExitPlanRequestsAtom)
  const agentStatusBarEnabled = useAtomValue(agentStatusBarEnabledAtom)
  const hasBannerOverlay =
    (allAskUserRequests.get(sessionId)?.length ?? 0) > 0 ||
    (allExitPlanRequests.get(sessionId)?.length ?? 0) > 0

  const hasTextInput = inputContent.trim().length > 0
  const canSend = (hasTextInput || pendingFiles.length > 0 || !!suggestion) && activeProviderModel !== null && (!streaming || hasTextInput)

  const handleDismissSuggestion = React.useCallback(() => {
    setPromptSuggestions((prev) => {
      if (!prev.has(sessionId)) return prev
      const map = new Map(prev)
      map.delete(sessionId)
      return map
    })
  }, [sessionId, setPromptSuggestions])

  const handleToggleThinking = React.useCallback(() => {
    const next = agentThinking?.type === 'adaptive'
      ? { type: 'disabled' as const }
      : { type: 'adaptive' as const }
    setAgentThinking(next)
    updateSettings({ agentThinking: next })
  }, [agentThinking, setAgentThinking])

  return {
    sessionId,
    messages,
    messagesLoaded,
    streaming,
    streamState,
    liveMessages,
    stoppedByUser,
    sessionPath,
    attachedDirs,
    sessionModelId: agentModelId || activeProviderModel?.modelId || undefined,
    sessions,
    agentError,
    isPlanMode,
    agentStatusBarEnabled,
    hasBannerOverlay,
    inputContent,
    inputHtmlContent,
    handleComposerChange,
    handleComposerFocus,
    handleComposerBlur,
    setInputHtmlContent,
    setInputContent,
    activeProviderModel,
    sendWithCmdEnter,
    canSend,
    hasTextInput,
    workspaceSlug,
    allAttachedDirs,
    pendingFiles,
    suggestion,
    composerEditorRef,
    mentionControllerRef,
    agentThinking,
    contextStatus,
    currentQueue,
    handleSteerQueued,
    handleEditQueued,
    handleDeleteQueued,
    handleSend: composer.handleSend,
    handlePasteFiles: composer.handlePasteFiles,
    handlePasteLongText: composer.handlePasteLongText,
    handleDragOver: composer.handleDragOver,
    handleDragLeave: composer.handleDragLeave,
    handleDrop: composer.handleDrop,
    isDragOver: composer.isDragOver,
    handleOpenFileDialog: composer.handleOpenFileDialog,
    handleRemoveFile: composer.handleRemoveFile,
    handleSegmentFinalized: composer.handleSegmentFinalized,
    handleDismissSuggestion,
    handleToggleThinking,
    handleStop,
    handleCompact,
    handleRetry,
    handleRetryInNewSession,
    handleFork,
    handleRewindRequest,
    rewindTargetUuid,
    setRewindTargetUuid,
    handleRewindConfirm,
    firstRunOpen,
    setFirstRunOpen,
  }
}
