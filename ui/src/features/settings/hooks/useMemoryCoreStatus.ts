// Owns the MemoryCore engine status for Settings › 记忆: the initial
// `memory_core_get_status`, the `memory-core:status` event stream (events with
// a seq ≤ the last applied one are dropped — the backend may deliver them out
// of order), and the two actions (models toggle — optimistic with rollback —
// and download retry). State lives in a jotai atom so the nav attention dot and
// the card share one copy. All IPC goes through `lib/bridge/memory-core`.
import * as React from 'react'
import { atom, useAtom } from 'jotai'
import { toast } from 'sonner'
import {
  memoryCoreBridge,
  onMemoryCoreStatus,
  type MemoryCoreStatus,
} from '@/lib/bridge/memory-core'

export const memoryCoreStatusAtom = atom<MemoryCoreStatus | null>(null)

/**
 * Seq guard. Pushed events must be strictly newer; command responses
 * (`allowEqual`) may carry the same seq when the call changed nothing.
 */
export function acceptMemoryCoreStatus(
  prev: MemoryCoreStatus | null,
  next: MemoryCoreStatus,
  allowEqual = false,
): MemoryCoreStatus | null {
  if (!prev) return next
  return next.seq > prev.seq || (allowEqual && next.seq === prev.seq) ? next : prev
}

/** What the engine is doing, from the user's point of view. */
export type MemoryEngineState = 'off' | 'queued' | 'downloading' | 'failed' | 'loading' | 'ready'

export function memoryEngineState(s: MemoryCoreStatus): MemoryEngineState {
  if (!s.modelsEnabled) return 'off'
  switch (s.download?.phase) {
    case 'failed':
      return 'failed'
    case 'queued':
      return 'queued'
    case 'downloading':
      return s.download.downloadedBytes > 0 ? 'downloading' : 'queued'
    default:
      return s.embedderLoaded ? 'ready' : 'loading'
  }
}

/** Settings nav attention dot: active while a download is queued/running, danger when it failed. */
export function memoryCoreAttention(s: MemoryCoreStatus | null): 'none' | 'active' | 'failed' {
  const phase = s?.download?.phase
  if (phase === 'failed') return 'failed'
  if (phase === 'queued' || phase === 'downloading') return 'active'
  return 'none'
}

const message = (e: unknown): string =>
  e instanceof Error ? e.message : typeof e === 'string' ? e : String(e)

export function useMemoryCoreStatus() {
  const [status, setStatus] = useAtom(memoryCoreStatusAtom)
  const [error, setError] = React.useState<string | null>(null)
  const [pending, setPending] = React.useState(false)

  const apply = React.useCallback(
    (next: MemoryCoreStatus, allowEqual = false) =>
      setStatus((prev) => acceptMemoryCoreStatus(prev, next, allowEqual)),
    [setStatus],
  )

  const reload = React.useCallback(async () => {
    setError(null)
    try {
      apply(await memoryCoreBridge.getStatus(), true)
    } catch (e) {
      setError(message(e))
    }
  }, [apply])

  React.useEffect(() => {
    void reload()
    let unlisten: (() => void) | undefined
    let disposed = false
    onMemoryCoreStatus((s) => apply(s))
      .then((fn) => {
        if (disposed) fn()
        else unlisten = fn
      })
      .catch(() => {})
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [apply, reload])

  const setModelsEnabled = React.useCallback(
    async (enabled: boolean) => {
      setPending(true)
      // Optimistic: flip the switch now; the response (or events) bring the rest.
      setStatus((prev) => (prev ? { ...prev, modelsEnabled: enabled } : prev))
      try {
        apply(await memoryCoreBridge.setModelsEnabled(enabled), true)
      } catch (e) {
        setStatus((prev) => (prev ? { ...prev, modelsEnabled: !enabled } : prev))
        toast.error(enabled ? '无法开启本地检索模型' : '无法关闭本地检索模型', {
          description: message(e),
        })
      } finally {
        setPending(false)
      }
    },
    [apply, setStatus],
  )

  const retry = React.useCallback(async () => {
    setPending(true)
    try {
      apply(await memoryCoreBridge.retryDownload(), true)
    } catch (e) {
      toast.error('重试下载失败', { description: message(e) })
    } finally {
      setPending(false)
    }
  }, [apply])

  return {
    status,
    loading: status === null && error === null,
    error: status === null ? error : null,
    pending,
    setModelsEnabled,
    retry,
    reload,
  }
}
