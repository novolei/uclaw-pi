import { useEffect } from 'react'
import { useStore } from 'jotai'

/** Jotai 没有公开导出 Store 类型，这里通过 useStore 的返回类型推导 */
type Store = ReturnType<typeof useStore>
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'sonner'
import {
  agentStreamingStatesAtom,
  unviewedCompletedSessionIdsAtom,
  workingDoneSessionIdsAtom,
  agentStreamErrorsAtom,
  stoppedByUserSessionsAtom,
  currentAgentSessionIdAtom,
  agentSessionsAtom,
  proactiveLearningEventsAtom,
  daydreamEventsAtom,
  memoryRecallEventAtom,
  sessionBrowserPreviewMapAtom,
  liveMessagesMapAtom,
  skillRecallsMapAtom,
  type AgentStreamState,
  type ProactiveLearningEvent,
  type DaydreamEvent,
  type MemoryRecallEvent,
  type AgentStreamErrorPayload,
  type BrowserPreviewState,
  appendLiveOutput,
} from '@/atoms/agent-atoms'
import {
  autoPreviewEnabledAtom,
  autoPreviewDismissedSessionsAtom,
  pendingWriteToolsAtom,
  openPreviewTabAction,
  openBrowserTabAction,
  type PreviewFileTarget,
} from '@/atoms/preview-panel-atoms'
import { workspaceSessionsAtom, updateSessionTitleAtom, type WorkspaceSession } from '@/atoms/workspace'
import { tabsAtom } from '@/atoms/tab-atoms'
import { conversationsAtom } from '@/atoms/chat-atoms'
import {
  browserTaskRunAtom,
  type BrowserTaskRunEntry,
  type BrowserTaskStatus,
  type BrowserTaskStepEntry,
} from '@/atoms/browser-atoms'
import type { AgentSessionMeta } from '@/lib/agent-types'
import type { TabItem } from '@/atoms/tab-atoms'

/**
 * Per-session monotonic counter used to drop stale async path resolutions.
 * Each tool_start bumps the session's seq; the resolution callback captures
 * the seq it saw and refuses to apply if the session's current seq has
 * moved on (user navigated, stream-complete cleared state, etc.). Over-Proma
 * improvement — Proma races and occasionally opens the wrong file.
 */
const autoPreviewSeq = new Map<string, number>()

/** Raw (unresolved) preview target paths captured at tool_start, keyed by
 *  toolCallId. Used by tool_result to retry resolution when the tool_start
 *  pre-stake failed (typical for write_file creating a new file — the file
 *  doesn't exist until tool_result, so the resolver couldn't return mount
 *  info on the first try). */
const pendingWriteRawPaths = new Map<string, string>()

interface ChipResolutionIpcPayload {
  input: string
  exists: boolean
  mountId: string | null
  relPath: string | null
  absolutePath: string | null
}

interface BrowserTaskRunPayload extends BrowserTaskRunEntry {}

interface BrowserTaskStepPayload {
  runId: string
  sessionId: string
  status: BrowserTaskStatus
  step: BrowserTaskStepEntry
}

function createInitialStreamState(): AgentStreamState {
  return {
    running: true,
    content: '',
    toolActivities: [],
    teammates: [],
    startedAt: Date.now(),
  }
}

function upsertBrowserTaskStep(
  current: BrowserTaskRunEntry | undefined,
  payload: BrowserTaskStepPayload,
): BrowserTaskRunEntry {
  const run: BrowserTaskRunEntry = current ?? {
    runId: payload.runId,
    sessionId: payload.sessionId,
    task: '',
    status: payload.status,
    steps: [],
  }
  const steps = [...run.steps]
  const idx = steps.findIndex((step) => step.stepIndex === payload.step.stepIndex)
  if (idx >= 0) {
    steps[idx] = payload.step
  } else {
    steps.push(payload.step)
    steps.sort((a, b) => a.stepIndex - b.stepIndex)
  }
  return { ...run, status: payload.status, steps }
}

// ─── Module-level singleton ───────────────────────────────────────────────────
// Listeners are global for the app's lifetime. Using a singleton prevents
// React StrictMode (which double-fires effects) and Vite HMR module reloads
// from stacking up duplicate Tauri event listeners.

let cleanupFns: Array<() => void> = []
let initialized = false

// HMR 清理：模块热替换前调用所有 cleanupFns，否则旧 listener 仍挂在 Tauri 事件
// 总线上，造成每个事件触发多次回调（症状是 streaming 文字 2x/3x 重复）。
// 没有这段，每次 .ts 保存重载都会"再加一份监听"。
if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    cleanupFns.forEach((fn) => {
      try { fn() } catch (e) { console.error('[useGlobalAgentListeners] HMR cleanup error', e) }
    })
    cleanupFns = []
    initialized = false
    lastReasoningSeq.clear()
    lastChunkSeq.clear()
    autoPreviewSeq.clear()
    pendingWriteRawPaths.clear()
  })
}

// Per-session last-processed seq numbers for chat:stream-reasoning and chat:stream-chunk deduplication.
// The backend includes a monotonically increasing `seq` with each delta; we skip
// any event whose seq is not strictly greater than the last one we processed.
// This defends against double-delivery that would otherwise cause word-by-word
// duplication in the streaming blocks.
const lastReasoningSeq = new Map<string, number>()
const lastChunkSeq = new Map<string, number>()

function startAgentListeners(store: Store): void {
  if (initialized) return
  initialized = true

  // Helper: register a Tauri listener and collect its unlisten fn.
  // listen() is async, so we always store the unlisten fn once the Promise
  // settles — if dispose() already ran we call it immediately.
  let disposed = false
  function reg(p: Promise<() => void>): void {
    p.then((fn) => {
      if (disposed) fn()
      else cleanupFns.push(fn)
    }).catch(console.error)
  }

  // chat:stream-chunk → append streaming content
  reg(
    listen<{ conversationId: string; delta: string; seq?: number }>('chat:stream-chunk', ({ payload }) => {
      const sid = payload.conversationId

      if (payload.seq !== undefined) {
        if (payload.seq === 0) {
          lastChunkSeq.delete(sid)
        }
        const last = lastChunkSeq.get(sid)
        if (last !== undefined && payload.seq <= last) return
        lastChunkSeq.set(sid, payload.seq)
      }

      store.set(agentStreamingStatesAtom, (prev) => {
        const existing = prev.get(sid) ?? createInitialStreamState()
        const next = new Map(prev)
        const isNewStream = payload.seq === 0
        const content = isNewStream ? payload.delta : (existing.content + payload.delta)
        next.set(sid, { ...existing, content })
        return next
      })
      store.set(agentStreamErrorsAtom, (prev) => {
        if (!prev.has(sid)) return prev
        const next = new Map(prev)
        next.delete(sid)
        return next
      })
      store.set(stoppedByUserSessionsAtom, (prev) => {
        if (!prev.has(sid)) return prev
        const next = new Set(prev)
        next.delete(sid)
        return next
      })
    })
  )

  // agent:stream-reset → backend is retrying a failed stream from scratch;
  // clear accumulated content so duplicate tokens don't pile up.
  reg(
    listen<{ conversationId: string; timestamp: string }>('agent:stream-reset', ({ payload }) => {
      const sid = payload.conversationId
      lastReasoningSeq.delete(sid)
      lastChunkSeq.delete(sid)
      store.set(agentStreamingStatesAtom, (prev) => {
        const existing = prev.get(sid)
        if (!existing) return prev
        const next = new Map(prev)
        next.set(sid, { ...existing, content: '', reasoning: '' })
        return next
      })
    })
  )

  // chat:stream-complete → mark session done, finalize stuck activities
  reg(
    listen<{
      conversationId: string
      text: string
      compact?: { removed: number; remaining: number; before: number }
      truncated?: boolean
    }>('chat:stream-complete', ({ payload }) => {
      const sid = payload.conversationId
      const wasCompacting = store.get(agentStreamingStatesAtom).get(sid)?.isCompacting === true
      store.set(agentStreamingStatesAtom, (prev) => {
        const existing = prev.get(sid)
        if (!existing) return prev
        const next = new Map(prev)
        const finalActivities = existing.toolActivities.map((a) =>
          a.done ? a : { ...a, done: true }
        )
        // Clear `reasoning` once the turn completes — the persisted
        // assistant message carries it in `message.reasoning`, and
        // AgentMessageItem renders it inline from that field. Leaving
        // streamState.reasoning truthy after running=false keeps the
        // streaming bubble alive with only a ThinkingBlock once the
        // AgentView post-persist effect (which clears content + tool
        // activities) finishes — producing the orphan "THINKING >"
        // ghost row reported visually.
        next.set(sid, {
          ...existing,
          running: false,
          isCompacting: false,
          compactInFlight: false,
          content: payload.text || existing.content,
          reasoning: undefined,
          toolActivities: finalActivities,
          truncated: payload.truncated === true,
        })
        return next
      })
      // 压缩完成 → 两件事：(1) 弹 toast 给即时反馈；(2) 在 liveMessages
      // 尾部注入一个 compact_boundary 标记，由 AgentMessages 渲染成持久
      // 的"上下文已压缩"分隔符（区别于 toast 的瞬时通知）。
      if (payload.compact || wasCompacting) {
        const removed = payload.compact?.removed ?? 0
        const remaining = payload.compact?.remaining ?? 0
        toast.success('上下文已压缩', {
          description: removed > 0
            ? `已移除 ${removed} 条早期消息，保留 ${remaining} 条`
            : payload.compact
              ? `已是最简上下文，当前保留 ${remaining} 条`
              : undefined,
          duration: 3500,
        })

        store.set(liveMessagesMapAtom, (prev) => {
          const map = new Map(prev)
          // Filter out any compacting indicators — they are replaced by the
          // compact_boundary below (clean transition from "正在压缩..." → "上下文已压缩").
          const current = (map.get(sid) ?? []).filter(
            (item: any) => !(item.type === 'system' && item.subtype === 'compacting')
          )
          const marker = {
            type: 'system',
            subtype: 'compact_boundary',
            uuid: `compact-boundary-${Date.now()}`,
            _createdAt: Date.now(),
            removed,
            remaining,
          }
          map.set(sid, [...current, marker])
          return map
        })
      }
      const currentSid = store.get(currentAgentSessionIdAtom)
      if (sid !== currentSid) {
        store.set(unviewedCompletedSessionIdsAtom, (prev) => {
          const next = new Set(prev)
          next.add(sid)
          return next
        })
      }
      store.set(workingDoneSessionIdsAtom, (prev) => {
        const next = new Set(prev)
        next.add(sid)
        return next
      })

      // Re-arm auto-preview for the next turn: clear the per-session
      // dismiss memory and drop any stuck pending-writes entries. The
      // seq stays so any still-in-flight resolver call from this turn
      // sees a stale seq and drops itself.
      store.set(autoPreviewDismissedSessionsAtom, (prev: Set<string>) => {
        if (!prev.has(sid)) return prev
        const next = new Set(prev)
        next.delete(sid)
        return next
      })
      store.set(pendingWriteToolsAtom, (prev) => {
        if (!prev.has(sid)) return prev
        const next = new Map(prev)
        next.delete(sid)
        return next
      })
      autoPreviewSeq.set(sid, (autoPreviewSeq.get(sid) ?? 0) + 1)
    })
  )

  // chat:stream-error → record error and stop
  reg(
    listen<{
      conversationId: string
      error: string
      kind?: AgentStreamErrorPayload['kind']
      timeoutSecs?: number
    }>('chat:stream-error', ({ payload }) => {
      const sid = payload.conversationId
      store.set(agentStreamErrorsAtom, (prev) => {
        const next = new Map(prev)
        next.set(sid, {
          message: payload.error,
          kind: payload.kind,
          timeoutSecs: payload.timeoutSecs,
        })
        return next
      })
      store.set(agentStreamingStatesAtom, (prev) => {
        const existing = prev.get(sid)
        if (!existing) return prev
        const next = new Map(prev)
        // Clear `reasoning` for the same reason as the stream-complete
        // handler — without this, an error mid-thinking would leave an
        // orphan ThinkingBlock in the streaming bubble after the persisted
        // message arrives.
        next.set(sid, { ...existing, running: false, isCompacting: false, compactInFlight: false, reasoning: undefined })
        return next
      })
    })
  )

  // chat:stream-reasoning → append thinking content
  reg(
    listen<{ conversationId: string; delta: string; seq?: number }>('chat:stream-reasoning', ({ payload }) => {
      const sid = payload.conversationId

      // Deduplicate: if the backend includes a seq number, skip events we've already processed.
      if (payload.seq !== undefined) {
        if (payload.seq === 0) {
          lastReasoningSeq.delete(sid)
        }
        const last = lastReasoningSeq.get(sid)
        if (last !== undefined && payload.seq <= last) return
        lastReasoningSeq.set(sid, payload.seq)
      }

      store.set(agentStreamingStatesAtom, (prev) => {
        const existing = prev.get(sid) ?? createInitialStreamState()
        const next = new Map(prev)
        const isNewStream = payload.seq === 0
        const reasoning = isNewStream ? payload.delta : ((existing.reasoning ?? '') + payload.delta)
        next.set(sid, {
          ...existing,
          running: true,
          reasoning,
          content: isNewStream ? '' : existing.content,
        })
        return next
      })
    })
  )

  // chat:stream-tool-activity → record tool activity
  reg(
    listen<{ conversationId: string; activity: any }>('chat:stream-tool-activity', ({ payload }) => {
      const sid = payload.conversationId
      const ev = payload.activity
      store.set(agentStreamingStatesAtom, (prev) => {
        const existing = prev.get(sid) ?? createInitialStreamState()
        const activities = [...existing.toolActivities]

        if (ev.type === 'tool_start') {
          const newId = ev.toolCallId ?? ''
          if (!activities.some((a) => a.toolUseId === newId)) {
            activities.push({
              toolUseId: newId,
              toolName: ev.toolName ?? '',
              input: ev.input ?? {},
              done: false,
            })
          }
        } else if (ev.type === 'tool_output_chunk') {
          const idx = activities.findIndex((a) => a.toolUseId === ev.toolCallId)
          if (idx >= 0) {
            const streamKind = ev.stream === 'stderr' ? 'stderr' : 'stdout'
            activities[idx] = {
              ...activities[idx]!,
              liveOutput: appendLiveOutput(activities[idx]!.liveOutput, streamKind, String(ev.chunk ?? '')),
            }
          }
        } else if (ev.type === 'tool_result') {
          const idx = activities.findIndex((a) => a.toolUseId === ev.toolCallId)
          if (idx >= 0) {
            const raw = ev.result
            const resultStr: string =
              typeof raw === 'string'
                ? raw
                : (raw?.output ?? raw?.content ?? raw?.error ?? JSON.stringify(raw ?? ''))
            activities[idx] = {
              ...activities[idx]!,
              result: resultStr,
              isError: ev.isError ?? (raw?.ok === false),
              done: true,
            }
          }
        }

        const next = new Map(prev)
        next.set(sid, { ...existing, toolActivities: activities })
        return next
      })
    })
  )

  // auto-preview: when a write/edit tool fires, auto-open the preview panel
  // for its target file. Resolution happens on tool_result (file exists by
  // then for write_file; already existed for edit). tool_start kicks off an
  // optimistic resolve to populate the progress-indicator path for the
  // common edit-existing-file case.
  //
  // Backend tools declare their target by overriding Tool::preview_target_path
  // (see agent/tools/tool.rs). The dispatcher includes it as `previewTarget`
  // in the activity payload, so this listener never needs a hardcoded
  // WRITE_TOOLS Set (Proma's main maintenance footgun).
  const buildResolvedTarget = (
    r: ChipResolutionIpcPayload,
    sid: string,
  ): PreviewFileTarget | null => {
    if (!r.mountId || !r.relPath || !r.absolutePath) return null
    const name = r.relPath.split('/').pop() ?? r.relPath
    return {
      mountId: r.mountId,
      relPath: r.relPath,
      name,
      sessionId: sid,
      absolutePath: r.absolutePath,
    }
  }

  reg(
    listen<{ conversationId: string; activity: any }>('chat:stream-tool-activity', ({ payload }) => {
      const sid = payload.conversationId
      const ev = payload.activity

      if (ev.type === 'tool_start') {
        const target = ev.previewTarget
        const toolCallId = ev.toolCallId
        if (typeof target !== 'string' || !target || !toolCallId) return

        const seq = (autoPreviewSeq.get(sid) ?? 0) + 1
        autoPreviewSeq.set(sid, seq)
        pendingWriteRawPaths.set(toolCallId, target)

        // Mark write in flight immediately so the progress indicator can
        // render even before path resolution completes. The string stored
        // is the raw target; the optimistic resolve below upgrades it to
        // the absolute path once available.
        store.set(pendingWriteToolsAtom, (prev) => {
          const next = new Map(prev)
          const inner = new Map(next.get(sid) ?? new Map<string, string>())
          inner.set(toolCallId, target)
          next.set(sid, inner)
          return next
        })

        // Optimistic resolve — only succeeds for files that already exist
        // (the edit-existing-file case). New writes will fail here; the
        // tool_result handler retries.
        void invoke<ChipResolutionIpcPayload[]>('preview_resolve_chips', {
          paths: [target],
          sessionId: sid,
        })
          .then((results) => {
            if (autoPreviewSeq.get(sid) !== seq) return
            const r = results[0]
            if (!r || !r.absolutePath) return
            // Upgrade pendingWriteTools entry to the absolute path so the
            // PreviewHeader progress badge can match by absolutePath.
            store.set(pendingWriteToolsAtom, (prev) => {
              const inner = prev.get(sid)
              if (!inner || !inner.has(toolCallId)) return prev
              const next = new Map(prev)
              const nextInner = new Map(inner)
              nextInner.set(toolCallId, r.absolutePath!)
              next.set(sid, nextInner)
              return next
            })
          })
          .catch(() => { /* swallow — tool_result retries */ })
      } else if (ev.type === 'tool_result') {
        const toolCallId = ev.toolCallId
        if (!toolCallId) return
        const rawPath = pendingWriteRawPaths.get(toolCallId)
        pendingWriteRawPaths.delete(toolCallId)

        // Always clear the pending entry so the progress indicator stops.
        store.set(pendingWriteToolsAtom, (prev) => {
          const inner = prev.get(sid)
          if (!inner || !inner.has(toolCallId)) return prev
          const next = new Map(prev)
          const nextInner = new Map(inner)
          nextInner.delete(toolCallId)
          if (nextInner.size === 0) next.delete(sid)
          else next.set(sid, nextInner)
          return next
        })

        if (ev.isError) return
        if (!rawPath) return
        if (!store.get(autoPreviewEnabledAtom)) return
        if (store.get(autoPreviewDismissedSessionsAtom).has(sid)) return

        const seq = autoPreviewSeq.get(sid) ?? 0

        // Re-resolve now that the write has landed — file exists, so the
        // resolver can return full mount/rel/abs triple.
        void invoke<ChipResolutionIpcPayload[]>('preview_resolve_chips', {
          paths: [rawPath],
          sessionId: sid,
        })
          .then((results) => {
            if (autoPreviewSeq.get(sid) !== seq) return
            // Re-check gates at apply-time — user may have toggled / dismissed
            // / navigated during the async hop.
            if (!store.get(autoPreviewEnabledAtom)) return
            if (store.get(autoPreviewDismissedSessionsAtom).has(sid)) return
            if (store.get(currentAgentSessionIdAtom) !== sid) return
            const r = results[0]
            if (!r) return
            const resolved = buildResolvedTarget(r, sid)
            if (!resolved) return
            store.set(openPreviewTabAction, { target: resolved, source: 'agent' })
          })
          .catch(() => { /* silent — auto-preview is best-effort */ })
      }
    })
  )

  // browser tool events → update per-session browser preview overlay
  reg(
    listen<{ conversationId: string; activity: any }>('chat:stream-tool-activity', ({ payload }) => {
      const sid = payload.conversationId
      const ev = payload.activity
      const toolName: string = ev.toolName ?? ''

      if (
        ev.type === 'tool_start' &&
        (toolName === 'browser_task' || toolName === 'retry_with_browser_agent')
      ) {
        const initialUrl = typeof ev.input?.start_url === 'string' ? ev.input.start_url : ''
        const task = typeof ev.input?.task === 'string' ? ev.input.task : ''
        const runId = typeof ev.toolCallId === 'string' && ev.toolCallId.length > 0
          ? ev.toolCallId
          : `pending-${Date.now()}`
        store.set(browserTaskRunAtom, (prev) => {
          const next = new Map(prev)
          next.set(sid, {
            runId,
            sessionId: sid,
            task,
            status: 'running',
            steps: [],
          })
          return next
        })
        store.set(openBrowserTabAction, { agentSessionId: sid, initialUrl })
        return
      }

      if (ev.type !== 'tool_result') return
      if (toolName !== 'browser_navigate' && toolName !== 'browser_screenshot') return

      // ToolOutput::success wraps the text as { ok: true, content: "..." }.
      // Extract the inner content string regardless of whether result is already
      // a plain string or the wrapped object form.
      const rawResult = ev.result
      const contentStr: string =
        typeof rawResult === 'string'
          ? rawResult
          : (rawResult?.content ?? rawResult?.output ?? JSON.stringify(rawResult ?? ''))

      let resolvedTabId: string | null = null

      store.set(sessionBrowserPreviewMapAtom, (prev) => {
        const existing: BrowserPreviewState = prev.get(sid) ?? {
          url: null, tabId: null, screenshotData: null, visible: true, minimized: false,
        }
        let next = { ...existing, visible: true }

        if (toolName === 'browser_navigate' && !ev.isError) {
          // contentStr: "Navigated to <url>. tab_id=..."
          const urlMatch = contentStr.match(/Navigated to (\S+?)\.?\s/)
          if (urlMatch) next = { ...next, url: urlMatch[1] }
          const tabMatch = contentStr.match(/tab_id=(\S+)/)
          if (tabMatch) {
            next = { ...next, tabId: tabMatch[1] }
            resolvedTabId = tabMatch[1]
          }
        } else if (toolName === 'browser_screenshot' && !ev.isError) {
          // ToolOutput::new → result is { ok, width, height, data } directly.
          // Also handle legacy string-wrapped form just in case.
          const data: string | undefined =
            rawResult?.data ??
            (() => { try { return JSON.parse(contentStr)?.data } catch { return undefined } })()
          if (data) next = { ...next, screenshotData: data }
        }

        const map = new Map(prev)
        map.set(sid, next)
        return map
      })

      // On first successful browser_navigate: open the browser panel tab.
      // The actual screencast lifecycle (subscribe-then-start) is owned by
      // BrowserPanel's mount effect — it guarantees the frontend listener
      // is attached before the backend emits the first frame.
      if (toolName === 'browser_navigate' && !ev.isError && resolvedTabId) {
        const currentUrl = (() => {
          const urlMatch = contentStr.match(/Navigated to (\S+?)\.?\s/)
          return urlMatch ? urlMatch[1] : ''
        })()
        store.set(openBrowserTabAction, { agentSessionId: sid, initialUrl: currentUrl })
      }
    })
  )

  // browser task events are emitted directly by the backend, not as ordinary
  // chat tool results. Keep this listener global so early task steps are not
  // lost while the BrowserPanel tab is still being opened.
  reg(
    listen<BrowserTaskRunPayload>('browser:task-run', ({ payload }) => {
      store.set(browserTaskRunAtom, (prev) => {
        const next = new Map(prev)
        next.set(payload.sessionId, payload)
        return next
      })
    })
  )

  reg(
    listen<BrowserTaskStepPayload>('browser:task-step', ({ payload }) => {
      store.set(browserTaskRunAtom, (prev) => {
        const next = new Map(prev)
        next.set(payload.sessionId, upsertBrowserTaskStep(prev.get(payload.sessionId), payload))
        return next
      })
    })
  )

  // agent:skill-recalled → push into skillRecallsMapAtom (dedup by toolCallId)
  reg(
    listen<{
      conversationId: string
      toolCallId: string
      kind: 'search' | 'load'
      timestamp: string
      query?: string
      results?: Array<{ name: string; summary: string; score: number; provenance: 'learned' | 'builtin'; cited_count?: number }>
      name?: string
      reason?: string
      provenance?: 'learned' | 'builtin'
    }>('agent:skill-recalled', ({ payload }) => {
      const sid = payload.conversationId
      store.set(skillRecallsMapAtom, (prev) => {
        const current = prev.get(sid) ?? []
        // Dedup by toolCallId. The dispatcher injects `tc.id` as `_tool_call_id`
        // into tool args before spawn (agent/dispatcher.rs), so the Rust
        // skill_search / load_skill tools have a stable per-call ID to echo in
        // the `agent:skill-recalled` event. Same ID arriving twice (e.g. event
        // double-delivery under HMR) collapses to one chip — intended.
        if (current.some((r) => r.toolCallId === payload.toolCallId)) {
          return prev
        }
        const next = new Map(prev)
        next.set(sid, [...current, {
          toolCallId: payload.toolCallId,
          kind: payload.kind,
          timestamp: payload.timestamp,
          query: payload.query,
          results: payload.results,
          name: payload.name,
          reason: payload.reason,
          provenance: payload.provenance,
        }])
        return next
      })
    })
  )

  // session:title-pending → mark session title as generating (skeleton UI)
  reg(
    listen<string>('session:title-pending', ({ payload: sessionId }) => {
      // Update agentSessionsAtom
      store.set(agentSessionsAtom, (prev: AgentSessionMeta[]) =>
        prev.map((s: AgentSessionMeta) =>
          s.id === sessionId ? { ...s, titlePending: true } : s
        )
      )
      // Update workspaceSessionsAtom
      store.set(workspaceSessionsAtom, (prev: Record<string, WorkspaceSession[]>) => {
        const next = { ...prev }
        for (const spaceId of Object.keys(next)) {
          next[spaceId] = next[spaceId].map((s: WorkspaceSession) =>
            s.id === sessionId ? { ...s, titlePending: true } : s
          )
        }
        return next
      })
    })
  )

  // session:title-updated → apply generated title + emoji
  reg(
    listen<{ sessionId: string; title: string; emoji: string }>(
      'session:title-updated',
      ({ payload }) => {
        const { sessionId, title, emoji } = payload
        // Update agentSessionsAtom
        store.set(agentSessionsAtom, (prev: AgentSessionMeta[]) =>
          prev.map((s: AgentSessionMeta) =>
            s.id === sessionId
              ? { ...s, title, titleEmoji: emoji, titlePending: false }
              : s
          )
        )
        // Update workspaceSessionsAtom via the dedicated write-atom
        store.set(updateSessionTitleAtom, { sessionId, title, emoji })
        // Update tab bar: show emoji + title so the open tab reflects the new name
        const tabTitle = emoji ? `${emoji} ${title}` : title
        store.set(tabsAtom, (prev: TabItem[]) =>
          prev.map((t: TabItem) =>
            t.sessionId === sessionId ? { ...t, title: tabTitle } : t
          )
        )
        // Chat conversations rename via this same backend emit (for chat,
        // sessionId is the conversation id). Replaces the removed frontend
        // generate_title path so chat sessions get live titles too.
        store.set(conversationsAtom, (prev) =>
          prev.map((c) => (c.id === sessionId ? { ...c, title } : c))
        )
      }
    )
  )

  // agent:turn_cost → store per-turn token usage in streaming state
  reg(
    listen<{
      conversationId: string;
      inputTokens: number;
      outputTokens: number;
      cacheReadTokens?: number;
      cacheCreationTokens?: number;
      costUsd: string;
    }>(
      'agent:turn_cost',
      ({ payload }) => {
        const sid = payload.conversationId
        store.set(agentStreamingStatesAtom, (prev) => {
          const existing = prev.get(sid)
          if (!existing) return prev
          const next = new Map(prev)
          next.set(sid, {
            ...existing,
            inputTokens: payload.inputTokens,
            outputTokens: payload.outputTokens,
            cacheReadTokens: payload.cacheReadTokens ?? 0,
            cacheCreationTokens: payload.cacheCreationTokens ?? 0,
            costUsd: parseFloat(payload.costUsd.replace('$', '')),
          })
          return next
        })
      }
    )
  )

  // agent:context_stats → capture skill manifest token cost AND context window
  // bounds into streaming state. Previously only skillsTokens was captured;
  // modelContextLength + freeTokens were ignored, so ContextUsageBadge never
  // received contextWindow → ratio ring never rendered. (P0 fix: 2026-05-16)
  reg(
    listen<{
      conversationId: string
      skillsTokens: number
      modelContextLength: number
      freeTokens: number
    }>(
      'agent:context_stats',
      ({ payload }) => {
        const sid = payload.conversationId
        store.set(agentStreamingStatesAtom, (prev) => {
          const existing = prev.get(sid)
          if (!existing) return prev
          const next = new Map(prev)
          // contextWindow: model's actual max context window (e.g. 200K for Claude).
          // Persisted once per session — doesn't change between turns.
          const contextWindow: number = payload.modelContextLength || existing.contextWindow || 0
          // inputTokens: estimated total context usage computed by backend.
          // Only set when the per-turn agent:turn_cost hasn't already populated it
          // (turn_cost fires first in on_usage, so this acts as fallback).
          const estimatedInput = contextWindow > 0
            ? Math.max(0, contextWindow - payload.freeTokens)
            : 0
          next.set(sid, {
            ...existing,
            skillsTokens: payload.skillsTokens,
            contextWindow,
            ...(existing.inputTokens != null && existing.inputTokens > 0
              ? {} // turn_cost already provided the API-counted value — keep it
              : { inputTokens: estimatedInput }),
          })
          return next
        })
      }
    )
  )

  // agent:proactive-learning → prepend to events list (cap at 10)
  reg(
    listen<ProactiveLearningEvent>('agent:proactive-learning', ({ payload }) => {
      // Diagnostic: log every received event so we can correlate with
      // backend emit logs when the chip doesn't show. Includes
      // sessionId so we can spot filter mismatches.
      console.info('[proactive-learning] received', {
        scenario: payload.scenario,
        items: payload.items_extracted,
        sessionId: payload.sessionId,
        timestamp: payload.timestamp,
      })
      store.set(proactiveLearningEventsAtom, (prev) =>
        [payload, ...prev].slice(0, 10)
      )
    })
  )

  // agent:daydream → live-prepend to the daydream ring buffer (cap 10)
  reg(
    listen<{ content: string; created_at?: string }>('agent:daydream', ({ payload }) => {
      const ev: DaydreamEvent = {
        content: payload.content,
        createdAt: payload.created_at ?? new Date().toISOString(),
      }
      store.set(daydreamEventsAtom, (prev) => [ev, ...prev].slice(0, 10))
    }),
  )

  // agent:memory-recall → update latest recall event
  reg(
    listen<MemoryRecallEvent>('agent:memory-recall', ({ payload }) => {
      console.info('[memory-recall] received', {
        total: payload.totalCandidates,
        skills: payload.skillsCount,
        conversationId: payload.conversationId,
      })
      store.set(memoryRecallEventAtom, (prev) => {
        const next = new Map(prev)
        next.set(payload.conversationId || '__global__', payload)
        return next
      })
    })
  )

  // Dispose function: unlisten everything and reset for next HMR cycle
  const dispose = () => {
    disposed = true
    initialized = false
    for (const fn of cleanupFns) fn()
    cleanupFns = []
    lastReasoningSeq.clear()
  }

  // Vite HMR: tear down listeners before this module is hot-replaced so the
  // next module evaluation starts with a clean slate.
  if (import.meta.hot) {
    import.meta.hot.dispose(dispose)
  }
}

// ─── React hook ──────────────────────────────────────────────────────────────
// Just a mount trigger; the real work happens in startAgentListeners().
// StrictMode's double-run is harmless because startAgentListeners() guards
// against re-entry with the `initialized` flag.

export function useGlobalAgentListeners(): void {
  const store = useStore()

  useEffect(() => {
    startAgentListeners(store)
    // No cleanup returned — listeners are intentionally global for the app lifetime.
  }, [store])
}
