/**
 * useAgentComposer — the agent composer's attachment + paste/drop + submit
 * engine, extracted VERBATIM from the AgentView component body during the
 * features/agent migration split (the largest agent leaf).
 *
 * This is the critical user path the agent driver touches every session:
 *  - attachment building (makeUniqueFilename / addFilesAsAttachments)
 *  - the OS file picker (handleOpenFileDialog) + remove (handleRemoveFile)
 *  - clipboard paste of files + long-text → attachment (handlePasteFiles /
 *    handlePasteLongText)
 *  - drag-over / drag-leave / drop with native path resolution + directory
 *    attach (handleDragOver / handleDragLeave / handleDrop)
 *  - the STT segment-finalized insertion (handleSegmentFinalized)
 *  - the full send pipeline (handleSend) — /compact intercept, the Codex-style
 *    follow-up queue while streaming, error/suggestion/turn-badge clearing,
 *    attachment persistence, optimistic user message, and sendAgentMessage.
 *
 * Behavior-preserving move: every useCallback / useState / useRef body is
 * identical to the original, including dependency arrays, only relocated. The
 * shell owns the shared draft/atom state and passes it in; the hook returns the
 * handlers + the `isDragOver` flag + the `pendingFilesRef`.
 *
 * IPC routes through the agent bridge (no `@tauri-apps/api` here).
 */

import * as React from 'react'
import { toast } from 'sonner'
import type { Editor } from '@tiptap/core'
import { smartJoin } from '@/lib/stt/punctuation'
import { fileToBase64 } from '@/lib/file-utils'
import { createClipboardTextFile } from '@/lib/clipboard-attachment'
import type { AgentSendInput, AgentMessage, AgentPendingFile, AgentWorkspace } from '@/lib/agent-types'
import {
  agentStreamingStatesAtom,
  agentStreamErrorsAtom,
  agentPromptSuggestionsAtom,
  liveMessagesMapAtom,
  stoppedByUserSessionsAtom,
  agentSessionAttachedDirsMapAtom,
  proactiveLearningEventsAtom,
  memoryRecallEventAtom,
  skillRecallsMapAtom,
  type AgentStrategy,
} from '@/atoms/agent-atoms'
import { draftSessionIdsAtom } from '@/atoms/draft-session-atoms'
import { agentQueuedMessagesMapAtom } from '@/atoms/agent-queue-messages'
import type { ActiveProviderModel } from '@/atoms/active-model'
import {
  sendAgentMessage,
  getAgentSessionMessages,
  agentFollowUp,
  saveFilesToAgentSession,
  attachSessionDirectory,
  openFileDialog,
  getPathForFile,
  checkPathsType,
} from '@/lib/bridge/agent'

type SetMap<K, V> = React.Dispatch<React.SetStateAction<Map<K, V>>>
type SetSet<T> = React.Dispatch<React.SetStateAction<Set<T>>>
type JotaiStore = ReturnType<typeof import('jotai').useStore>

/** Inputs the composer hook needs from the AgentView shell. */
export interface UseAgentComposerArgs {
  sessionId: string
  inputContent: string
  setInputContent: (value: string) => void
  setInputHtmlContent: (html: string) => void
  setComposerHasText: (v: boolean) => void
  pendingFiles: AgentPendingFile[]
  setPendingFiles: React.Dispatch<React.SetStateAction<AgentPendingFile[]>>
  composerEditorRef: React.MutableRefObject<Editor | null>
  activeProviderModel: ActiveProviderModel | null
  agentChannelId: string | null
  agentModelId: string | null
  currentWorkspaceId: string | null
  workspaces: AgentWorkspace[]
  streaming: boolean
  suggestion: string | null
  currentStrategy: AgentStrategy
  attachedDirs: string[]
  store: JotaiStore
  setStreamingStates: SetMap<string, any>
  setMessages: React.Dispatch<React.SetStateAction<AgentMessage[]>>
  setAgentStreamErrors: SetMap<string, any>
  setPromptSuggestions: SetMap<string, any>
  setDraftSessionIds: SetSet<string>
  setSessionAttachedMap: SetMap<string, string[]>
  /** /compact path shares the badge-button handler via a ref to avoid TDZ. */
  handleCompactRef: React.MutableRefObject<(() => void) | null>
}

export interface UseAgentComposerResult {
  isDragOver: boolean
  pendingFilesRef: React.MutableRefObject<AgentPendingFile[]>
  addFilesAsAttachments: (files: File[]) => Promise<void>
  handleOpenFileDialog: () => Promise<void>
  handleRemoveFile: (id: string) => void
  handlePasteFiles: (files: File[]) => void
  handlePasteLongText: (text: string) => void
  handleSegmentFinalized: (text: string) => void
  handleDragOver: (e: React.DragEvent) => void
  handleDragLeave: (e: React.DragEvent) => void
  handleDrop: (e: React.DragEvent) => Promise<void>
  handleSend: () => Promise<void>
}

export function useAgentComposer(args: UseAgentComposerArgs): UseAgentComposerResult {
  const {
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
  } = args

  const [isDragOver, setIsDragOver] = React.useState(false)

  // pendingFiles ref（供 addFilesAsAttachments 读取最新列表，避免闭包旧值）
  const pendingFilesRef = React.useRef(pendingFiles)
  React.useEffect(() => {
    pendingFilesRef.current = pendingFiles
  }, [pendingFiles])

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
  }, [sessionId, addFilesAsAttachments, setSessionAttachedMap])

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
  }, [inputContent, pendingFiles, sessionId, activeProviderModel, agentChannelId, agentModelId, currentWorkspaceId, workspaces, streaming, suggestion, currentStrategy, attachedDirs, store, setStreamingStates, setPendingFiles, setAgentStreamErrors, setPromptSuggestions, setInputContent, setInputHtmlContent, setComposerHasText, setMessages, setDraftSessionIds, handleCompactRef])

  return {
    isDragOver,
    pendingFilesRef,
    addFilesAsAttachments,
    handleOpenFileDialog,
    handleRemoveFile,
    handlePasteFiles,
    handlePasteLongText,
    handleSegmentFinalized,
    handleDragOver,
    handleDragLeave,
    handleDrop,
    handleSend,
  }
}
