/**
 * agent-message-helpers — pure (no-JSX) helpers extracted from AgentMessages.tsx
 * during the features/agent migration split. These are shared between the list
 * shell, the per-message item renderer, and the attachment chips, so they live
 * in `lib/` rather than any one component. Behavior is byte-for-byte identical to
 * the originals — this is a move, not a rewrite.
 */

import type { AgentMessage, AgentEvent } from '@/lib/agent-types'
import type { ChatToolActivity } from '@/lib/chat-types'
import type { ToolActivity } from '@/atoms/agent-atoms'

/**
 * Map persisted tool activities (the `ChatToolActivity[]` the backend hydrates
 * onto `AgentMessage.toolActivities` from `tool_activities_json` / `agent_turns`)
 * into the streaming `AgentEvent[]` shape that {@link extractToolActivities}
 * consumes. The assistant renderer keys off `message.events`, but loaded messages
 * carry `toolActivities` (a `start` + `result` entry per call, keyed by
 * `toolCallId`) and **no** `events` — so without this conversion the whole
 * tool-call process vanishes on reload / after the turn. `start → tool_start`,
 * `result → tool_result`, `toolCallId → toolUseId`.
 */
export function persistedToolActivitiesToEvents(
  activities: ChatToolActivity[] | undefined,
): AgentEvent[] {
  if (!activities || activities.length === 0) return []
  return activities.map((a) => ({
    type: a.type === 'start' ? 'tool_start' : 'tool_result',
    toolUseId: a.toolCallId,
    toolName: a.toolName,
    input: a.input,
    result: a.result,
    isError: a.isError,
  }))
}

/**
 * 把流式 agent ToolActivity[] 转换为持久化展示用的 ChatToolActivity[] start/result 配对。
 * 让流式 UI 与历史消息（已用 ChatToolActivityIndicator 渲染）展示风格一致。
 */
export function agentActivitiesToChatActivities(activities: ToolActivity[]): import('@/lib/proma-types').ChatToolActivity[] {
  const out: import('@/lib/proma-types').ChatToolActivity[] = []
  for (const a of activities) {
    out.push({
      toolCallId: a.toolUseId,
      type: 'start',
      toolName: a.toolName,
      input: a.input,
      // 携带实时输出 — 工具完成后 liveOutput 不再有意义（result 接管），仅在 running 时传递
      liveOutput: a.done ? undefined : a.liveOutput,
    })
    if (a.done) {
      out.push({
        toolCallId: a.toolUseId,
        type: 'result',
        toolName: a.toolName,
        input: a.input,
        result: a.result,
        isError: a.isError,
        status: a.isError ? 'failed' : 'completed',
      })
    }
  }
  return out
}

/** 从持久化事件中提取工具活动列表 */
export function extractToolActivities(events: AgentMessage['events']): ToolActivity[] {
  if (!events) return []

  const activities: ToolActivity[] = []
  for (const event of events) {
    if (event.type === 'tool_start') {
      const existingIdx = activities.findIndex((t) => t.toolUseId === event.toolUseId)
      if (existingIdx >= 0) {
        activities[existingIdx] = {
          ...activities[existingIdx]!,
          input: event.input,
          intent: event.intent || activities[existingIdx]!.intent,
          displayName: event.displayName || activities[existingIdx]!.displayName,
        }
      } else {
        activities.push({
          toolUseId: event.toolUseId ?? '',
          toolName: event.toolName ?? '',
          input: event.input,
          intent: event.intent,
          displayName: event.displayName,
          done: true,
          parentToolUseId: event.parentToolUseId,
        })
      }
    } else if (event.type === 'tool_result') {
      const idx = activities.findIndex((t) => t.toolUseId === event.toolUseId)
      if (idx >= 0) {
        activities[idx] = {
          ...activities[idx]!,
          result: event.result,
          isError: event.isError,
          done: true,
          imageAttachments: event.imageAttachments,
        }
      }
    } else if (event.type === 'task_backgrounded') {
      const idx = activities.findIndex((t) => t.toolUseId === event.toolUseId)
      if (idx >= 0) {
        activities[idx] = { ...activities[idx]!, isBackground: true, taskId: event.taskId }
      }
    } else if (event.type === 'shell_backgrounded') {
      const idx = activities.findIndex((t) => t.toolUseId === event.toolUseId)
      if (idx >= 0) {
        activities[idx] = { ...activities[idx]!, isBackground: true, shellId: event.shellId }
      }
    } else if (event.type === 'task_progress') {
      const idx = activities.findIndex((t) => t.toolUseId === event.toolUseId)
      if (idx >= 0) {
        activities[idx] = { ...activities[idx]!, elapsedSeconds: event.elapsedSeconds }
      }
    } else if (event.type === 'task_started' && event.toolUseId) {
      const idx = activities.findIndex((t) => t.toolUseId === event.toolUseId)
      if (idx >= 0) {
        activities[idx] = { ...activities[idx]!, intent: event.description, taskId: event.taskId }
      }
    }
  }
  return activities
}

/** 解析的附件引用 */
export interface AttachedFileRef {
  filename: string
  path: string
}

/** 解析消息中的 <attached_files> 块，返回文件列表和剩余文本 */
export function parseAttachedFiles(content: string): { files: AttachedFileRef[]; text: string } {
  const regex = /<attached_files>\n?([\s\S]*?)\n?<\/attached_files>\n*/
  const match = content.match(regex)
  if (!match) return { files: [], text: content }

  const files: AttachedFileRef[] = []
  const lines = match[1]!.split('\n')
  for (const line of lines) {
    // 格式: - filename: /path/to/file
    const lineMatch = line.match(/^-\s+(.+?):\s+(.+)$/)
    if (lineMatch) {
      files.push({ filename: lineMatch[1]!.trim(), path: lineMatch[2]!.trim() })
    }
  }

  const text = content.replace(regex, '').trim()
  return { files, text }
}

/** 判断文件是否为图片类型 */
export function isImageFile(filename: string): boolean {
  return /\.(png|jpe?g|gif|webp|svg|bmp|ico)$/i.test(filename)
}

/** 相对时间戳 — 简化显示，如 "2m ago" / "刚刚" */
export function formatRelativeShort(ts: number): string {
  const diff = Math.floor((Date.now() - ts) / 1000)
  if (diff < 60) return '刚刚'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return new Date(ts).toLocaleDateString('zh-CN', { month: 'numeric', day: 'numeric' })
}
