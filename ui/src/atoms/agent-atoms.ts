/**
 * Agent Atoms — Agent 模式的 Jotai 状态管理
 *
 * 管理 Agent 会话列表、当前会话、消息、流式状态等。
 * 从 Proma 迁移，IPC 层由 tauri-bridge.ts 提供。
 */

import { atom } from 'jotai'
import { atomFamily } from 'jotai/utils'
import type { AgentSessionMeta, AgentMessage, AgentEvent, AgentWorkspace, AgentPendingFile, RetryAttempt, PermissionRequest, AskUserRequest, ExitPlanModeRequest, ThinkingConfig, AgentEffort, TaskUsage } from '@/lib/agent-types'

/** 活动状态 */
export type ActivityStatus = 'pending' | 'running' | 'completed' | 'error' | 'backgrounded'

/** Bash 等流式工具的实时输出(临时,仅 live 会话;reload 不重建) */
export interface LiveOutput {
  segments: { stream: 'stdout' | 'stderr'; text: string }[]
  bytes: number
  droppedHead: boolean
}

/** 工具活动状态 */
export interface ToolActivity {
  toolUseId: string
  toolName: string
  input: Record<string, unknown>
  intent?: string
  displayName?: string
  result?: string
  isError?: boolean
  done: boolean
  parentToolUseId?: string
  elapsedSeconds?: number
  taskId?: string
  shellId?: string
  isBackground?: boolean
  /** MCP 工具返回的图片附件 */
  imageAttachments?: Array<{ localPath: string; filename: string; mediaType: string }>
  /** 流式工具的实时输出窗口(有界 256KB);done 后由持久化 result 接管 */
  liveOutput?: LiveOutput
}

/** 活动分组（Task 子代理） */
export interface ActivityGroup {
  parent: ToolActivity
  children: ToolActivity[]
}

/** Teammate 状态枚举 */
export type TeammateStatus = 'running' | 'completed' | 'failed' | 'stopped'

/** 单个 teammate 的实时状态（Agent Teams 功能） */
export interface TeammateState {
  taskId: string
  toolUseId?: string
  description: string
  taskType?: string
  index: number
  status: TeammateStatus
  progressDescription?: string
  currentToolName?: string
  currentToolElapsedSeconds?: number
  currentToolUseId?: string
  toolHistory: string[]
  summary?: string
  outputFile?: string
  usage?: TaskUsage
  startedAt: number
  endedAt?: number
}

/** 工具历史最大记录数 */
const MAX_TOOL_HISTORY = 20

/**
 * 将流式状态中未完成的 toolActivities 和 running teammates 标记为终态。
 */
export function finalizeStreamingActivities(
  toolActivities: ToolActivity[],
  teammates: TeammateState[]
): { toolActivities: ToolActivity[]; teammates: TeammateState[] } {
  const hasUnfinishedTools = toolActivities.some((ta) => !ta.done)
  const hasRunningTeammates = teammates.some((tm) => tm.status === 'running')

  return {
    toolActivities: hasUnfinishedTools
      ? toolActivities.map((ta) => (ta.done ? ta : { ...ta, done: true }))
      : toolActivities,
    teammates: hasRunningTeammates
      ? teammates.map((tm) =>
          tm.status === 'running'
            ? { ...tm, status: 'stopped' as const, endedAt: Date.now(), currentToolName: undefined, currentToolElapsedSeconds: undefined, currentToolUseId: undefined }
            : tm
        )
      : teammates,
  }
}

/** Agent 会话的流式状态 */
export interface AgentStreamState {
  running: boolean
  content: string
  reasoning?: string
  toolActivities: ToolActivity[]
  model?: string
  inputTokens?: number
  outputTokens?: number
  cacheReadTokens?: number
  cacheCreationTokens?: number
  costUsd?: number
  contextWindow?: number
  /** Skill manifest token cost, from agent:context_stats event.
   *  Reflects the size of the "你已学习到的技能" block appended to the
   *  system prompt. Stays 0 when the manifest is empty (no learned skills). */
  skillsTokens?: number
  isCompacting?: boolean
  compactInFlight?: boolean
  startedAt?: number
  retrying?: {
    currentAttempt: number
    maxAttempts: number
    history: RetryAttempt[]
    failed: boolean
  }
  teammates: TeammateState[]
  waitingResume?: boolean
  /** Whether any LLM call in this turn was truncated (finish_reason=length). */
  truncated?: boolean
}

/** 从 ToolActivity 派生状态 */
export function getActivityStatus(activity: ToolActivity): ActivityStatus {
  if (activity.isBackground) return 'backgrounded'
  if (!activity.done) return 'running'
  if (activity.isError) return 'error'
  return 'completed'
}

/**
 * 合并同层 TodoWrite 活动
 */
function mergeTodoWrites(activities: ToolActivity[]): ToolActivity[] {
  const todoWrites: ToolActivity[] = []
  const others: ToolActivity[] = []

  for (const a of activities) {
    if (a.toolName === 'TodoWrite') {
      todoWrites.push(a)
    } else {
      others.push(a)
    }
  }

  if (todoWrites.length === 0) return activities

  const latest = todoWrites[todoWrites.length - 1]!
  const allDone = todoWrites.every((t) => t.done)

  const merged: ToolActivity = {
    ...latest,
    done: allDone,
    isError: allDone && todoWrites.some((t) => t.isError),
  }

  return [...others, merged]
}

/**
 * 将扁平活动列表按 parentToolUseId 分组
 */
export function groupActivities(activities: ToolActivity[]): Array<ActivityGroup | ToolActivity> {
  const filtered = activities.filter((a) => {
    if (a.done && Object.keys(a.input).length === 0 && !a.result) return false
    return true
  })
  const processed = mergeTodoWrites(filtered)

  const parentIds = new Set<string>()
  for (const a of processed) {
    if (a.toolName === 'Task' || a.toolName === 'Agent') parentIds.add(a.toolUseId)
  }

  const childrenMap = new Map<string, ToolActivity[]>()
  const topLevel: Array<ActivityGroup | ToolActivity> = []

  for (const a of processed) {
    if (a.parentToolUseId && parentIds.has(a.parentToolUseId)) {
      const children = childrenMap.get(a.parentToolUseId) ?? []
      children.push(a)
      childrenMap.set(a.parentToolUseId, children)
    } else {
      topLevel.push(a)
    }
  }

  return topLevel.map((item) => {
    if ('toolUseId' in item && parentIds.has(item.toolUseId)) {
      const children = childrenMap.get(item.toolUseId) ?? []
      return { parent: item, children: mergeTodoWrites(children) } as ActivityGroup
    }
    return item
  })
}

/** 判断是否为 ActivityGroup */
export function isActivityGroup(item: ActivityGroup | ToolActivity): item is ActivityGroup {
  return 'parent' in item && 'children' in item
}

/** 待自动发送的 Agent 提示 */
export interface AgentPendingPrompt {
  sessionId: string
  message: string
}

// ===== Atoms =====

export const agentSessionsAtom = atom<AgentSessionMeta[]>([])
export const agentWorkspacesAtom = atom<AgentWorkspace[]>([])
export const currentAgentWorkspaceIdAtom = atom<string | null>(null)

/** Map workspace.id → attached dir paths. Hydrated at startup from
 *  list_spaces (each WorkspaceInfo carries attachedDirs); kept in sync
 *  by attach/detach mutations.
 */
export const workspaceAttachedDirsMapAtom = atom<Map<string, string[]>>(new Map())

/** Map agent_session.id → attached dir paths. Hydrated at startup from
 *  list_agent_sessions (each session carries attachedDirs in its JSON).
 */
export const agentSessionAttachedDirsMapAtom = atom<Map<string, string[]>>(new Map())
export const agentChannelIdAtom = atom<string | null>(null)
export const agentModelIdAtom = atom<string | null>(null)
export const agentChannelIdsAtom = atom<string[]>([])

export const agentSessionChannelMapAtom = atom<Map<string, string>>(new Map())
export const agentSessionModelMapAtom = atom<Map<string, string>>(new Map())

/** Per-session strategy preset. 'balanced' is the default (no bias). */
export type AgentStrategy = 'balanced' | 'repair' | 'optimize' | 'innovate'
export const agentSessionStrategyMapAtom = atom<Map<string, AgentStrategy>>(new Map())
export const currentAgentSessionIdAtom = atom<string | null>(null)
export const currentAgentMessagesAtom = atom<AgentMessage[]>([])

/**
 * Toggle the pin state on an agent session. Calls the backend then
 * optimistically updates `agentSessionsAtom` so the UI reflects the
 * new state without a refetch. Errors propagate to the caller for
 * toast surfacing.
 */
export const togglePinAgentSessionAtom = atom(
  null,
  async (_get, set, sessionId: string) => {
    const { togglePinAgentSession } = await import('@/lib/tauri-bridge')
    const newPinnedAt = await togglePinAgentSession(sessionId)
    set(agentSessionsAtom, (prev) =>
      prev.map((s) =>
        s.id === sessionId ? { ...s, pinnedAt: newPinnedAt } : s
      ) as typeof prev
    )
    return newPinnedAt
  }
)
export const agentStreamingStatesAtom = atom<Map<string, AgentStreamState>>(new Map())

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const liveMessagesMapAtom = atom<Map<string, any[]>>(new Map())

export interface SkillRecall {
  toolCallId: string
  kind: 'search' | 'load'
  timestamp: string
  query?: string
  results?: Array<{
    name: string
    summary: string
    score: number
    provenance: 'learned' | 'builtin'
    cited_count?: number
    category?: string
  }>
  name?: string
  reason?: string
  provenance?: 'learned' | 'builtin'
}

export const skillRecallsMapAtom = atom<Map<string, SkillRecall[]>>(new Map())

export const agentPendingPromptAtom = atom<AgentPendingPrompt | null>(null)

// Per-session pending files. Switching sessions must not leak attachments
// across — the user attaches in session A, switches to session B, B should
// have its own (possibly empty) attachment list.
export const agentPendingFilesMapAtom = atom<Map<string, AgentPendingFile[]>>(new Map())

/** Read/write the current session's pending files. Backed by
 *  `agentPendingFilesMapAtom` keyed by `currentAgentSessionIdAtom`. Reads
 *  return [] when no session is active. Writes are silently dropped when
 *  no session is active (matches old behavior of working on a global list
 *  but without the cross-session leak). */
export const agentPendingFilesAtom = atom<
  AgentPendingFile[],
  [AgentPendingFile[] | ((prev: AgentPendingFile[]) => AgentPendingFile[])],
  void
>(
  (get) => {
    const sid = get(currentAgentSessionIdAtom)
    if (!sid) return []
    return get(agentPendingFilesMapAtom).get(sid) ?? []
  },
  (get, set, next) => {
    const sid = get(currentAgentSessionIdAtom)
    if (!sid) return
    const map = get(agentPendingFilesMapAtom)
    const prev = map.get(sid) ?? []
    const value = typeof next === 'function' ? next(prev) : next
    const newMap = new Map(map)
    if (value.length === 0) {
      newMap.delete(sid)
    } else {
      newMap.set(sid, value)
    }
    set(agentPendingFilesMapAtom, newMap)
  }
)
export const workspaceCapabilitiesVersionAtom = atom(0)
export const workspaceFilesVersionAtom = atom(0)

/**
 * Phase 4b: per-workspace right-panel tab memory.
 *
 * RightSidePanel reads/writes this map keyed by the current
 * activeWorkspaceId. Switching workspace restores that workspace's
 * last viewed tab; new workspaces (no entry) default to 'files'.
 * In-memory only — app restart resets all entries.
 *
 * The ActiveTab type lives in RightSidePanel.tsx (exported) so it
 * stays co-located with the tab-list source of truth.
 */
export const workspaceActiveRightPanelTabMapAtom =
  atom<Map<string, import('@/components/app-shell/RightSidePanel').ActiveTab>>(new Map())

// ===== 侧面板 Atoms =====

export const agentSidePanelOpenMapAtom = atom<Map<string, boolean>>(new Map())

export const currentSessionSidePanelOpenAtom = atom<boolean>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return false
  return get(agentSidePanelOpenMapAtom).get(currentId) ?? true
})

export const agentSessionPathMapAtom = atom<Map<string, string>>(new Map())

export interface FileBrowserAutoReveal {
  sessionId: string
  path: string
  ts: number
}
export const fileBrowserAutoRevealAtom = atom<FileBrowserAutoReveal | null>(null)

export const recentlyModifiedPathsAtom = atom<Map<string, Map<string, number>>>(new Map())
export const RECENTLY_MODIFIED_TTL_MS = 60_000

// ===== 权限系统 Atoms =====
//
// NOTE: agentDefaultPermissionModeAtom + agentPermissionModeMapAtom were removed.
// They drove the input-bar mode selector against Tauri commands that never
// existed in the backend (`get_permission_mode` / `set_permission_mode`); the
// bridge silenced the IPC failures. The selector now lives in
// `safety-atoms.ts::safetyModeAtom` and writes to the real SafetyManager.

export const agentThinkingAtom = atom<ThinkingConfig | undefined>(undefined)
export const agentEffortAtom = atom<AgentEffort | undefined>(undefined)
export const agentMaxBudgetUsdAtom = atom<number | undefined>(undefined)
export const agentMaxTurnsAtom = atom<number | undefined>(undefined)

export const allPendingPermissionRequestsAtom = atom<Map<string, readonly PermissionRequest[]>>(new Map())

type PermissionRequestsUpdate = readonly PermissionRequest[] | ((prev: readonly PermissionRequest[]) => readonly PermissionRequest[])

export const pendingPermissionRequestsAtom = atom(
  (get): readonly PermissionRequest[] => {
    const currentId = get(currentAgentSessionIdAtom)
    if (!currentId) return []
    return get(allPendingPermissionRequestsAtom).get(currentId) ?? []
  },
  (get, set, update: PermissionRequestsUpdate) => {
    const currentId = get(currentAgentSessionIdAtom)
    if (!currentId) return
    set(allPendingPermissionRequestsAtom, (prev) => {
      const map = new Map(prev)
      const current = map.get(currentId) ?? []
      const newValue = typeof update === 'function' ? update(current) : update
      if (newValue.length === 0) map.delete(currentId)
      else map.set(currentId, newValue)
      return map
    })
  }
)

export const allPendingAskUserRequestsAtom = atom<Map<string, readonly AskUserRequest[]>>(new Map())

type AskUserRequestsUpdate = readonly AskUserRequest[] | ((prev: readonly AskUserRequest[]) => readonly AskUserRequest[])

export const pendingAskUserRequestsAtom = atom(
  (get): readonly AskUserRequest[] => {
    const currentId = get(currentAgentSessionIdAtom)
    if (!currentId) return []
    return get(allPendingAskUserRequestsAtom).get(currentId) ?? []
  },
  (get, set, update: AskUserRequestsUpdate) => {
    const currentId = get(currentAgentSessionIdAtom)
    if (!currentId) return
    set(allPendingAskUserRequestsAtom, (prev) => {
      const map = new Map(prev)
      const current = map.get(currentId) ?? []
      const newValue = typeof update === 'function' ? update(current) : update
      if (newValue.length === 0) map.delete(currentId)
      else map.set(currentId, newValue)
      return map
    })
  }
)

/**
 * Initialize the IPC listener that populates `allPendingAskUserRequestsAtom`
 * from `agent:ask_user_request` events. Call once at app start.
 */
export async function installAskUserListener(
  setMap: (update: (prev: Map<string, readonly AskUserRequest[]>) => Map<string, readonly AskUserRequest[]>) => void,
): Promise<() => void> {
  const { onAskUserRequest } = await import('@/lib/tauri-bridge')
  return await onAskUserRequest((payload) => {
    setMap((prev) => {
      const next = new Map(prev)
      const existing = next.get(payload.sessionId) ?? []
      next.set(payload.sessionId, [...existing, payload])
      return next
    })
  })
}

export const allPendingExitPlanRequestsAtom = atom<Map<string, readonly ExitPlanModeRequest[]>>(new Map())

/**
 * Initialize the IPC listener that populates `allPendingExitPlanRequestsAtom`
 * from `agent:exit_plan_request` events. Call once at app start.
 */
export async function installExitPlanListener(
  setMap: (update: (prev: Map<string, readonly ExitPlanModeRequest[]>) => Map<string, readonly ExitPlanModeRequest[]>) => void,
): Promise<() => void> {
  const { onExitPlanRequest } = await import('@/lib/tauri-bridge')
  return await onExitPlanRequest((payload) => {
    setMap((prev) => {
      const next = new Map(prev)
      const existing = next.get(payload.sessionId) ?? []
      next.set(payload.sessionId, [...existing, payload])
      return next
    })
  })
}
export const agentPlanModeSessionsAtom = atom<Set<string>>(new Set<string>())

// ───── composer state (lifted from RichTextInput) ─────
/** True iff the agent composer is currently focused. Lifted to atom for PetWidget. */
export const composerFocusedAtom = atom<boolean>(false)
/** True iff the agent composer's editor has non-empty text content. Lifted for PetWidget. */
export const composerHasTextAtom = atom<boolean>(false)

export const currentAgentSessionAtom = atom<AgentSessionMeta | null>((get) => {
  const sessions = get(agentSessionsAtom)
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return null
  return sessions.find((s) => s.id === currentId) ?? null
})

export const agentStreamingAtom = atom<boolean>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return false
  return get(agentStreamingStatesAtom).get(currentId)?.running ?? false
})

export const agentStreamingContentAtom = atom<string>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return ''
  return get(agentStreamingStatesAtom).get(currentId)?.content ?? ''
})

export const agentToolActivitiesAtom = atom<ToolActivity[]>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return []
  return get(agentStreamingStatesAtom).get(currentId)?.toolActivities ?? []
})

export const agentStreamingModelAtom = atom<string | undefined>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return undefined
  return get(agentStreamingStatesAtom).get(currentId)?.model
})

export const agentRetryingAtom = atom<AgentStreamState['retrying'] | undefined>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return undefined
  return get(agentStreamingStatesAtom).get(currentId)?.retrying
})

export const agentStartedAtAtom = atom<number | undefined>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return undefined
  return get(agentStreamingStatesAtom).get(currentId)?.startedAt
})

export const agentRunningSessionIdsAtom = atom<Set<string>>((get) => {
  const states = get(agentStreamingStatesAtom)
  const ids = new Set<string>()
  for (const [id, state] of states) {
    if (state.running) ids.add(id)
  }
  return ids
})

export type SessionIndicatorStatus = 'idle' | 'running' | 'blocked' | 'completed'

export const unviewedCompletedSessionIdsAtom = atom<Set<string>>(new Set<string>())
export const workingDoneSessionIdsAtom = atom<Set<string>>(new Set<string>())

export const agentSessionIndicatorMapAtom = atom<Map<string, SessionIndicatorStatus>>((get) => {
  const streamStates = get(agentStreamingStatesAtom)
  const pendingPerms = get(allPendingPermissionRequestsAtom)
  const pendingAskUser = get(allPendingAskUserRequestsAtom)
  const pendingExitPlan = get(allPendingExitPlanRequestsAtom)
  const unviewedCompleted = get(unviewedCompletedSessionIdsAtom)

  const map = new Map<string, SessionIndicatorStatus>()

  for (const [id, state] of streamStates) {
    if (!state.running) continue
    const hasBlock = (pendingPerms.get(id)?.length ?? 0) > 0
      || (pendingAskUser.get(id)?.length ?? 0) > 0
      || (pendingExitPlan.get(id)?.length ?? 0) > 0
    map.set(id, hasBlock ? 'blocked' : 'running')
  }

  for (const id of unviewedCompleted) {
    if (!map.has(id)) {
      map.set(id, 'completed')
    }
  }

  return map
})

function appendToolHistory(history: string[], toolName: string): string[] {
  if (history[history.length - 1] === toolName) return history
  const next = [...history, toolName]
  return next.length > MAX_TOOL_HISTORY ? next.slice(next.length - MAX_TOOL_HISTORY) : next
}

/** 实时输出窗口上限 */
const LIVE_OUTPUT_MAX_BYTES = 256 * 1024

/**
 * 向 LiveOutput 追加一块输出(纯函数)。
 * - 连续同 stream 合并进同一段
 * - 超过 256KB 从头部丢弃并置 droppedHead
 */
export function appendLiveOutput(
  prev: LiveOutput | undefined,
  stream: 'stdout' | 'stderr',
  text: string,
): LiveOutput {
  const segments = prev ? prev.segments.map((s) => ({ ...s })) : []
  const last = segments[segments.length - 1]
  if (last && last.stream === stream) {
    last.text += text
  } else {
    segments.push({ stream, text })
  }
  let bytes = (prev?.bytes ?? 0) + text.length
  let droppedHead = prev?.droppedHead ?? false

  while (bytes > LIVE_OUTPUT_MAX_BYTES && segments.length > 0) {
    const head = segments[0]!
    const over = bytes - LIVE_OUTPUT_MAX_BYTES
    if (head.text.length <= over) {
      bytes -= head.text.length
      segments.shift()
    } else {
      head.text = head.text.slice(over)
      bytes -= over
    }
    droppedHead = true
  }

  return { segments, bytes, droppedHead }
}

/**
 * 处理 AgentEvent 并更新流式状态（纯函数）
 */
export function applyAgentEvent(
  prev: AgentStreamState,
  event: AgentEvent,
): AgentStreamState {
  switch (event.type) {
    case 'text_delta':
      return { ...prev, content: prev.content + event.text, retrying: undefined }

    case 'text_complete':
      return { ...prev, content: event.text! }

    case 'tool_start': {
      const existing = prev.toolActivities.find((t) => t.toolUseId === event.toolUseId)
      if (existing) {
        return {
          ...prev,
          toolActivities: prev.toolActivities.map((t) =>
            t.toolUseId === event.toolUseId
              ? { ...t, input: event.input, intent: event.intent || t.intent, displayName: event.displayName || t.displayName }
              : t
          ),
          retrying: undefined,
        }
      }
      return {
        ...prev,
        toolActivities: [...prev.toolActivities, {
          toolUseId: event.toolUseId!,
          toolName: event.toolName!,
          input: event.input,
          intent: event.intent,
          displayName: event.displayName,
          done: false,
          parentToolUseId: event.parentToolUseId,
        }],
        retrying: undefined,
      }
    }

    case 'tool_result':
      return {
        ...prev,
        toolActivities: prev.toolActivities.map((t) =>
          t.toolUseId === event.toolUseId
            ? { ...t, result: event.result, isError: event.isError, done: true, imageAttachments: event.imageAttachments }
            : t
        ),
      }

    case 'task_backgrounded':
      return {
        ...prev,
        toolActivities: prev.toolActivities.map((t) =>
          t.toolUseId === event.toolUseId
            ? { ...t, isBackground: true, taskId: event.taskId, done: true }
            : t
        ),
      }

    case 'task_progress':
      if (event.taskId) {
        const tmIdx = prev.teammates.findIndex((t) => t.taskId === event.taskId)
        if (tmIdx >= 0) {
          const tm = prev.teammates[tmIdx]!
          const updatedTm: TeammateState = {
            ...tm,
            progressDescription: event.description ?? tm.progressDescription,
            usage: event.usage ?? tm.usage,
            ...(event.lastToolName && {
              currentToolName: event.lastToolName,
              currentToolElapsedSeconds: event.elapsedSeconds ?? tm.currentToolElapsedSeconds,
              currentToolUseId: event.toolUseId,
              toolHistory: appendToolHistory(tm.toolHistory, event.lastToolName),
            }),
            ...(!event.lastToolName && event.elapsedSeconds != null && {
              currentToolElapsedSeconds: event.elapsedSeconds,
            }),
            ...(prev.running && (tm.status === 'stopped' || tm.status === 'failed')
              ? { status: 'running' as const, endedAt: undefined }
              : {}),
          }
          const nextTeammates = [...prev.teammates]
          nextTeammates[tmIdx] = updatedTm
          return { ...prev, teammates: nextTeammates }
        }
      }
      if (event.elapsedSeconds != null) {
        return {
          ...prev,
          toolActivities: prev.toolActivities.map((t) =>
            t.toolUseId === event.toolUseId
              ? { ...t, elapsedSeconds: event.elapsedSeconds! }
              : t
          ),
        }
      }
      return prev

    case 'task_started': {
      let nextActivities = prev.toolActivities
      if (event.toolUseId) {
        const idx = prev.toolActivities.findIndex((t) => t.toolUseId === event.toolUseId)
        if (idx >= 0) {
          nextActivities = prev.toolActivities.map((t) =>
            t.toolUseId === event.toolUseId
              ? { ...t, intent: event.description, taskId: event.taskId }
              : t
          )
        }
      }
      if (prev.teammates.some((t) => t.taskId === event.taskId)) {
        return { ...prev, toolActivities: nextActivities }
      }
      const newTeammate: TeammateState = {
        taskId: event.taskId!,
        toolUseId: event.toolUseId,
        description: event.description!,
        taskType: event.taskType,
        index: prev.teammates.length + 1,
        status: 'running',
        toolHistory: [],
        startedAt: Date.now(),
      }
      return {
        ...prev,
        toolActivities: nextActivities,
        teammates: [...prev.teammates, newTeammate],
      }
    }

    case 'shell_backgrounded':
      return {
        ...prev,
        toolActivities: prev.toolActivities.map((t) =>
          t.toolUseId === event.toolUseId
            ? { ...t, isBackground: true, shellId: event.shellId, done: true }
            : t
        ),
      }

    case 'shell_killed':
      return prev

    case 'task_notification': {
      const nextTeammates = [...prev.teammates]
      let tmIdx = nextTeammates.findIndex((t) => t.taskId === event.taskId)
      if (tmIdx < 0) {
        nextTeammates.push({
          taskId: event.taskId!,
          toolUseId: event.toolUseId,
          description: event.summary || event.taskId!,
          index: nextTeammates.length + 1,
          status: 'running',
          toolHistory: [],
          startedAt: Date.now(),
        })
        tmIdx = nextTeammates.length - 1
      }
      nextTeammates[tmIdx] = {
        ...nextTeammates[tmIdx]!,
        status: event.status,
        summary: event.summary,
        outputFile: event.outputFile,
        endedAt: Date.now(),
        ...(event.usage && { usage: event.usage }),
        currentToolName: undefined,
        currentToolElapsedSeconds: undefined,
        currentToolUseId: undefined,
      }
      return { ...prev, teammates: nextTeammates }
    }

    case 'tool_use_summary':
      return prev

    case 'waiting_resume':
      return { ...prev, waitingResume: true }

    case 'resume_start':
      return { ...prev, waitingResume: false }

    case 'complete':
      return {
        ...prev,
        retrying: undefined,
        ...finalizeStreamingActivities(prev.toolActivities, prev.teammates),
      }

    case 'typed_error':
      return { ...prev, running: false, retrying: undefined }

    case 'error':
      return { ...prev, running: false }

    case 'usage_update':
      return {
        ...prev,
        inputTokens: event.usage.inputTokens,
        ...(event.usage.outputTokens != null && { outputTokens: event.usage.outputTokens }),
        ...(event.usage.cacheReadTokens != null && { cacheReadTokens: event.usage.cacheReadTokens }),
        ...(event.usage.cacheCreationTokens != null && { cacheCreationTokens: event.usage.cacheCreationTokens }),
        ...(event.usage.costUsd != null && { costUsd: event.usage.costUsd }),
        ...(event.usage.contextWindow && { contextWindow: event.usage.contextWindow }),
      }

    case 'compacting':
      return { ...prev, isCompacting: true, compactInFlight: true }

    case 'compact_complete':
      return { ...prev, isCompacting: false }

    case 'model_resolved':
      return prev

    case 'retrying':
      return {
        ...prev,
        retrying: prev.retrying ?? {
          currentAttempt: event.attempt!,
          maxAttempts: event.maxAttempts!,
          history: [],
          failed: false,
        },
      }

    case 'retry_attempt': {
      const currentHistory = prev.retrying?.history ?? []
      return {
        ...prev,
        retrying: {
          currentAttempt: event.attemptData!.attempt,
          maxAttempts: prev.retrying?.maxAttempts ?? 3,
          history: [...currentHistory, event.attemptData!],
          failed: false,
        },
      }
    }

    case 'retry_cleared':
      return { ...prev, retrying: undefined }

    case 'retry_failed': {
      const finalHistory = prev.retrying?.history ?? []
      return {
        ...prev,
        running: false,
        retrying: {
          currentAttempt: event.finalAttempt!.attempt,
          maxAttempts: prev.retrying?.maxAttempts ?? 3,
          history: [...finalHistory, event.finalAttempt!],
          failed: true,
        },
      }
    }

    case 'permission_request':
    case 'permission_resolved':
    case 'ask_user_request':
    case 'ask_user_resolved':
    case 'prompt_suggestion':
      return prev

    default:
      return prev
  }
}

/** 上下文使用量状态 */
export interface AgentContextStatus {
  isCompacting: boolean
  inputTokens?: number
  outputTokens?: number
  cacheReadTokens?: number
  cacheCreationTokens?: number
  costUsd?: number
  contextWindow?: number
  /** Skill manifest token cost (from agent:context_stats event). */
  skillsTokens?: number
}

export const agentContextStatusAtom = atom<AgentContextStatus>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return { isCompacting: false }
  const state = get(agentStreamingStatesAtom).get(currentId)
  return {
    isCompacting: state?.isCompacting ?? false,
    inputTokens: state?.inputTokens,
    outputTokens: state?.outputTokens,
    cacheReadTokens: state?.cacheReadTokens,
    cacheCreationTokens: state?.cacheCreationTokens,
    costUsd: state?.costUsd,
    contextWindow: state?.contextWindow,
    skillsTokens: state?.skillsTokens,
  }
})

export interface AgentStreamErrorPayload {
  message: string
  kind?: 'outer_timeout' | 'stream_stalled' | 'stream_failed' | 'fatal'
  timeoutSecs?: number
}

export const agentStreamErrorsAtom = atom<Map<string, AgentStreamErrorPayload>>(new Map())
export const agentMessageRefreshAtom = atom<Map<string, number>>(new Map())

export const currentAgentErrorAtom = atom<AgentStreamErrorPayload | null>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return null
  return get(agentStreamErrorsAtom).get(currentId) ?? null
})

export const agentSessionDraftsAtom = atom<Map<string, string>>(new Map())
export const agentSessionDraftHtmlAtom = atom<Map<string, string>>(new Map())

export const currentAgentSessionDraftAtom = atom(
  (get) => {
    const currentId = get(currentAgentSessionIdAtom)
    if (!currentId) return ''
    return get(agentSessionDraftsAtom).get(currentId) ?? ''
  },
  (get, set, newDraft: string) => {
    const currentId = get(currentAgentSessionIdAtom)
    if (!currentId) return
    set(agentSessionDraftsAtom, (prev) => {
      const map = new Map(prev)
      if (newDraft.trim() === '') {
        map.delete(currentId)
      } else {
        map.set(currentId, newDraft)
      }
      return map
    })
  }
)

// ===== 提示建议 Atoms =====

export const agentPromptSuggestionsAtom = atom<Map<string, string>>(new Map())

export const currentAgentSuggestionAtom = atom<string | null>((get) => {
  const currentId = get(currentAgentSessionIdAtom)
  if (!currentId) return null
  return get(agentPromptSuggestionsAtom).get(currentId) ?? null
})

// ===== 后台任务管理 =====

export interface BackgroundTask {
  id: string
  type: 'agent' | 'shell'
  toolUseId: string
  startTime: number
  elapsedSeconds: number
  intent?: string
}

export const backgroundTasksAtomFamily = atomFamily((sessionId: string) =>
  atom<BackgroundTask[]>([])
)

// ===== 用户打断状态 =====

export const stoppedByUserSessionsAtom = atom<Set<string>>(new Set<string>())

// ===== 记忆捕捉事件 =====

export interface ProactiveLearningEvent {
  scenario: 'conversation_learning' | 'skill_extraction' | 'multimodal_context'
  items_extracted: number
  categories: string[]
  timestamp: string
  summary: string
  /** Session ID that most recently sourced messages into the proactive
   *  context window when this extraction fired. Used by AgentMessages to
   *  scope the chip to that single session. May be null if the
   *  extraction ran before any user message in this app session. */
  sessionId?: string | null
}

/** 最近的记忆捕捉事件（最多保留 10 条，新的在前） */
export const proactiveLearningEventsAtom = atom<ProactiveLearningEvent[]>([])

/** 最近的 daydream 事件（最多保留 10 条，新的在前） */
export interface DaydreamEvent { content: string; createdAt: string }
export const daydreamEventsAtom = atom<DaydreamEvent[]>([])

// ===== 记忆召回事件 =====

export interface MemoryRecallItem {
  nodeId: string
  title: string
  kind: string
  source: string
}

export interface MemoryRecallEvent {
  totalCandidates: number
  skillsCount: number
  bootCount: number
  triggeredCount: number
  relevantCount: number
  expandedCount: number
  recentCount: number
  items: MemoryRecallItem[]
  conversationId: string | null
  timestamp: string
}

/** 记忆召回事件 Map，按 conversationId 索引，支持多 session 隔离 */
export const memoryRecallEventAtom = atom<Map<string, MemoryRecallEvent>>(new Map())

// ===== 初始化就绪状态 =====

export const agentSettingsReadyAtom = atom(false)

// ===== Browser preview overlay (per-session) =====

export interface BrowserPreviewState {
  /** Last successfully navigated URL */
  url: string | null
  /** Tab ID returned by browser_navigate, used to trigger auto-screenshot */
  tabId: string | null
  /** Base64 PNG data (no data: prefix) from the most recent browser_screenshot result */
  screenshotData: string | null
  /** Whether the overlay is visible (user hasn't dismissed it) */
  visible: boolean
  /** Whether the overlay is minimized (collapsed to just the URL bar) */
  minimized: boolean
}

/** Per-session browser preview state. Keyed by sessionId. */
export const sessionBrowserPreviewMapAtom = atom<Map<string, BrowserPreviewState>>(new Map())
