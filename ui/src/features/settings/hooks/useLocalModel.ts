// Owns the local MiniCPM model side effects (S2): the on-mount + on-quant-change
// presence probe, the download action wired to the source-aware progress event,
// and cancellation. All IPC goes through `settingsBridge` (no `@tauri-apps/api`
// import here) per the code-organization ADR (2026-05-31). The quant selection
// is persisted via `localModelQuantAtom`; the download lifecycle is tracked in
// `localModelStatusAtom`.
import * as React from 'react'
import { useAtom } from 'jotai'
import type { UnlistenFn } from '@tauri-apps/api/event'
import {
  settingsBridge,
  onLocalModelDownloadProgress,
  type LocalModelQuant,
} from '../../../lib/bridge/settings'
import { localModelQuantAtom, localModelStatusAtom } from '@/atoms/local-model-atoms'

export function useLocalModel() {
  const [quant, setQuant] = useAtom(localModelQuantAtom)
  const [status, setStatus] = useAtom(localModelStatusAtom)
  // True only while a download command is in flight. The backend emits the
  // "verifying" phase as its LAST progress event right before the command
  // resolves; that event is delivered asynchronously and can land AFTER the
  // command's await resolves + we set `ready`, clobbering it back to "校验中"
  // forever. The progress listener checks this ref and ignores any event that
  // arrives once the download has settled — the command result is authoritative.
  const downloadActiveRef = React.useRef(false)

  // Presence probe on mount + whenever the chosen quant changes. A download
  // in flight wins (don't clobber its progress).
  React.useEffect(() => {
    let cancelled = false
    void settingsBridge
      .isLocalModelPresent(quant)
      .then((present) => {
        if (cancelled) return
        setStatus((prev) =>
          prev.kind === 'downloading'
            ? prev
            : present
              ? { kind: 'ready' }
              : { kind: 'not-downloaded' },
        )
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [quant, setStatus])

  // Subscribe to backend download progress; drive the downloading status.
  React.useEffect(() => {
    let unlisten: UnlistenFn | undefined
    void onLocalModelDownloadProgress((p) => {
      // Ignore stragglers delivered after the command settled (see ref note).
      if (!downloadActiveRef.current) return
      const percent = p.total > 0 ? Math.round((p.downloaded / p.total) * 100) : 0
      setStatus({
        kind: 'downloading',
        phase: p.phase,
        source: p.source,
        downloaded: p.downloaded,
        total: p.total,
        percent,
      })
    }).then((fn) => {
      unlisten = fn
    })
    return () => {
      unlisten?.()
    }
  }, [setStatus])

  const handleDownload = React.useCallback(async () => {
    downloadActiveRef.current = true
    setStatus({
      kind: 'downloading',
      phase: 'probing',
      source: null,
      downloaded: 0,
      total: 0,
      percent: 0,
    })
    try {
      // No explicit source -> backend probes the fastest mirror.
      await settingsBridge.downloadLocalModel(quant, undefined)
      downloadActiveRef.current = false
      setStatus({ kind: 'ready' })
    } catch (e) {
      downloadActiveRef.current = false
      const message = String((e as Error)?.message ?? e)
      // A user-requested cancel surfaces as a backend error; re-probe presence.
      const present = await settingsBridge.isLocalModelPresent(quant).catch(() => false)
      setStatus(
        present
          ? { kind: 'ready' }
          : message.includes('cancelled')
            ? { kind: 'not-downloaded' }
            : { kind: 'error', message },
      )
    }
  }, [quant, setStatus])

  const handleCancel = React.useCallback(async () => {
    await settingsBridge.cancelLocalModelDownload().catch(() => {})
  }, [])

  const handleQuantChange = React.useCallback(
    (next: LocalModelQuant) => {
      // Instant UI: the atom drives the selector + presence probe locally.
      setQuant(next)
      // Persist backend-side so the in-process engine loads the same quant
      // (best-effort; silently no-op outside Tauri).
      settingsBridge
        .setLocalModelQuant(next)
        .catch((e) => console.warn('[useLocalModel] set_local_model_quant failed:', e))
    },
    [setQuant],
  )

  return { quant, status, handleDownload, handleCancel, handleQuantChange }
}
