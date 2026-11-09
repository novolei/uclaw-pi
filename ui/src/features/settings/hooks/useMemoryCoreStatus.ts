// Owns the MemoryCore engine status for Settings › 记忆: the initial
// `memory_core_get_status`, the `memory-core:status` event stream (events with
// a seq ≤ the last applied one are dropped — the backend may deliver them out
// of order), and the two actions (models toggle — optimistic with rollback —
// and download retry).
//
// One subscription per jotai store: the fetch + listener live in
// `memoryCoreSyncAtom.onMount`, which jotai runs when the FIRST component reads
// the atom and tears down after the last one unmounts. So the nav dot
// (SettingsPanel) and the card share one get_status call and one listener.
// All IPC goes through `lib/bridge/memory-core`.
import * as React from 'react'
import { atom, useAtomValue, useSetAtom } from 'jotai'
import { toast } from 'sonner'
import {
  memoryCoreBridge,
  onMemoryCoreStatus,
  type MemoryCoreStatus,
} from '@/lib/bridge/memory-core'

export const memoryCoreStatusAtom = atom<MemoryCoreStatus | null>(null)
/** Initial-load error (only meaningful while no status has been applied). */
const memoryCoreLoadErrorAtom = atom<string | null>(null)
/**
 * The toggle value while `memory_core_set_models_enabled` is in flight. The
 * backend persists before it flips `modelsEnabled`, so download-progress events
 * (newer seq, OLD modelsEnabled) keep arriving meanwhile; `apply` overrides
 * their `modelsEnabled` with this so the switch doesn't flip back.
 */
const memoryCorePendingEnabledAtom = atom<boolean | null>(null)

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

const message = (e: unknown): string =>
  e instanceof Error ? e.message : typeof e === 'string' ? e : String(e)

type SyncAction =
  | { type: 'load' }
  | { type: 'event'; status: MemoryCoreStatus }
  | { type: 'response'; status: MemoryCoreStatus }

const memoryCoreSyncAtom = atom(null, async (get, set, action: SyncAction) => {
  switch (action.type) {
    case 'load':
      set(memoryCoreLoadErrorAtom, null)
      try {
        const s = await memoryCoreBridge.getStatus()
        set(memoryCoreStatusAtom, (prev) => acceptMemoryCoreStatus(prev, s, true))
      } catch (e) {
        set(memoryCoreLoadErrorAtom, message(e))
      }
      return
    case 'event': {
      const pending = get(memoryCorePendingEnabledAtom)
      const s = pending === null ? action.status : { ...action.status, modelsEnabled: pending }
      set(memoryCoreStatusAtom, (prev) => acceptMemoryCoreStatus(prev, s))
      return
    }
    case 'response':
      set(memoryCoreStatusAtom, (prev) => acceptMemoryCoreStatus(prev, action.status, true))
      return
  }
})

memoryCoreSyncAtom.onMount = (dispatch) => {
  void dispatch({ type: 'load' })
  let unlisten: (() => void) | undefined
  let disposed = false
  onMemoryCoreStatus((status) => void dispatch({ type: 'event', status }))
    .then((fn) => {
      if (disposed) fn()
      else unlisten = fn
    })
    .catch(() => {})
  return () => {
    disposed = true
    unlisten?.()
  }
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
  if (!s?.modelsEnabled) return 'none'
  const phase = s.download?.phase
  if (phase === 'failed') return 'failed'
  if (phase === 'queued' || phase === 'downloading') return 'active'
  return 'none'
}

export function useMemoryCoreStatus() {
  useAtomValue(memoryCoreSyncAtom) // mounts the shared subscription
  const dispatch = useSetAtom(memoryCoreSyncAtom)
  const status = useAtomValue(memoryCoreStatusAtom)
  const loadError = useAtomValue(memoryCoreLoadErrorAtom)
  const setStatus = useSetAtom(memoryCoreStatusAtom)
  const setPendingEnabled = useSetAtom(memoryCorePendingEnabledAtom)
  const [pending, setPending] = React.useState(false)

  const reload = React.useCallback(() => dispatch({ type: 'load' }), [dispatch])

  const setModelsEnabled = React.useCallback(
    async (enabled: boolean) => {
      setPending(true)
      setPendingEnabled(enabled)
      // Optimistic: flip the switch now; the response (or events) bring the rest.
      setStatus((prev) => (prev ? { ...prev, modelsEnabled: enabled } : prev))
      try {
        const s = await memoryCoreBridge.setModelsEnabled(enabled)
        setPendingEnabled(null)
        await dispatch({ type: 'response', status: s })
      } catch (e) {
        setPendingEnabled(null)
        setStatus((prev) => (prev ? { ...prev, modelsEnabled: !enabled } : prev))
        toast.error(enabled ? '无法开启本地检索模型' : '无法关闭本地检索模型', {
          description: message(e),
        })
      } finally {
        setPending(false)
      }
    },
    [dispatch, setPendingEnabled, setStatus],
  )

  const retry = React.useCallback(async () => {
    setPending(true)
    try {
      await dispatch({ type: 'response', status: await memoryCoreBridge.retryDownload() })
    } catch (e) {
      toast.error('重试下载失败', { description: message(e) })
    } finally {
      setPending(false)
    }
  }, [dispatch])

  return {
    status,
    loading: status === null && loadError === null,
    error: status === null ? loadError : null,
    pending,
    setModelsEnabled,
    retry,
    reload,
  }
}
