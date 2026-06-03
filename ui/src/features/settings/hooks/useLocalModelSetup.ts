// Onboarding orchestration for the local MiniCPM model (S3). Encapsulates the
// first-launch state machine (env check → download → warmup → role-assign) so
// the onboarding `LocalModelStep` and the Settings re-entry share ONE source of
// truth for the flow. All IPC goes through `settingsBridge` (no `@tauri-apps/api`
// import here) per the code-organization ADR (2026-05-31).
//
// Download itself reuses the S2 building blocks: the same `downloadLocalModel`
// bridge call + the `local-model:download-progress` event subscription pattern
// from `useLocalModel`. The persisted quant lives in `localModelQuantAtom`.
import * as React from 'react'
import { useAtom } from 'jotai'
import type { UnlistenFn } from '@tauri-apps/api/event'
import {
  settingsBridge,
  onLocalModelDownloadProgress,
  type EnvReport,
} from '../../../lib/bridge/settings'
import { localModelQuantAtom } from '@/atoms/local-model-atoms'

/** Download progress projected for the step UI (mirrors the S2 progress shape). */
export interface SetupProgress {
  phase: 'probing' | 'downloading' | 'verifying'
  source: string | null
  downloaded: number
  total: number
  percent: number
}

/**
 * The onboarding state machine:
 * - `intro`      — initial; nothing run yet.
 * - `checking`   — env preflight in flight.
 * - `report`     — env report rendered; awaiting the user's 下载并启用 / 跳过.
 * - `downloading`— GGUF download in flight (progress bar live).
 * - `warming`    — load + JIT the runtime.
 * - `done`       — downloaded, warmed, and assigned to roles.
 * - `skipped`    — the user opted out (cloud-only).
 * - `blocked`    — disk too small; download disabled.
 */
export type SetupPhase =
  | 'intro'
  | 'checking'
  | 'report'
  | 'downloading'
  | 'warming'
  | 'done'
  | 'skipped'
  | 'blocked'

export interface LocalModelSetupState {
  phase: SetupPhase
  report: EnvReport | null
  progress: SetupProgress | null
  error: string | null
}

const INITIAL: LocalModelSetupState = {
  phase: 'intro',
  report: null,
  progress: null,
  error: null,
}

export interface UseLocalModelSetup extends LocalModelSetupState {
  /** Run the env preflight → `report` (or `blocked` if disk fails). */
  runChecks: () => Promise<void>
  /** Download → warmup → assign roles → `done`. No-op when `blocked`. */
  downloadAndEnable: () => Promise<void>
  /** Mark the step skipped (cloud-only). */
  skip: () => void
  /** Reset back to `intro` (used by the Settings re-entry to re-run). */
  reset: () => void
}

export function useLocalModelSetup(): UseLocalModelSetup {
  const [quant] = useAtom(localModelQuantAtom)
  const [state, setState] = React.useState<LocalModelSetupState>(INITIAL)

  // Subscribe to the S2 download progress event for the whole hook lifetime;
  // ticks only update the projected progress (the phase machine owns transitions).
  React.useEffect(() => {
    let unlisten: UnlistenFn | undefined
    void onLocalModelDownloadProgress((p) => {
      const percent = p.total > 0 ? Math.round((p.downloaded / p.total) * 100) : 0
      setState((prev) => ({
        ...prev,
        progress: {
          phase: p.phase,
          source: p.source,
          downloaded: p.downloaded,
          total: p.total,
          percent,
        },
      }))
    }).then((fn) => {
      unlisten = fn
    })
    return () => {
      unlisten?.()
    }
  }, [])

  const runChecks = React.useCallback(async () => {
    setState((prev) => ({ ...prev, phase: 'checking', error: null }))
    try {
      const report = await settingsBridge.checkLocalModelEnvironment(quant)
      setState((prev) => ({
        ...prev,
        report,
        // Disk too small is a hard block; everything else warns-but-proceeds.
        phase: report.diskOk ? 'report' : 'blocked',
      }))
    } catch (e) {
      const message = String((e as Error)?.message ?? e)
      setState((prev) => ({ ...prev, phase: 'report', error: message }))
    }
  }, [quant])

  const downloadAndEnable = React.useCallback(async () => {
    // Don't let a disk-blocked machine start a download.
    if (state.phase === 'blocked') return
    setState((prev) => ({
      ...prev,
      phase: 'downloading',
      error: null,
      progress: { phase: 'probing', source: null, downloaded: 0, total: 0, percent: 0 },
    }))
    try {
      // No explicit source -> backend probes the fastest mirror (S2 behavior).
      await settingsBridge.downloadLocalModel(quant, undefined)
      setState((prev) => ({ ...prev, phase: 'warming' }))
      await settingsBridge.warmupLocalModel()
      await settingsBridge.assignLocalModelToRoles()
      setState((prev) => ({ ...prev, phase: 'done', progress: null }))
    } catch (e) {
      const message = String((e as Error)?.message ?? e)
      // Surface the error but fall back to the report so the user can retry/skip.
      setState((prev) => ({ ...prev, phase: 'report', error: message, progress: null }))
    }
  }, [quant, state.phase])

  const skip = React.useCallback(() => {
    setState((prev) => ({ ...prev, phase: 'skipped' }))
  }, [])

  const reset = React.useCallback(() => {
    setState(INITIAL)
  }, [])

  return { ...state, runChecks, downloadAndEnable, skip, reset }
}
