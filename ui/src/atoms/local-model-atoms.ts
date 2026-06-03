/**
 * local-model-atoms — Jotai state for the local MiniCPM model (S2).
 *
 * - `localModelQuantAtom`: the chosen quantization, persisted to localStorage so
 *   the engine + presence checks agree across sessions. Default Q4_K_M.
 * - `localModelStatusAtom`: the download lifecycle the settings UI renders.
 */
import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'
import type { LocalModelQuant } from '@/lib/bridge/settings'

export const DEFAULT_QUANT: LocalModelQuant = 'q4_k_m'

/** Persisted quant selection (advanced selector writes this). */
export const localModelQuantAtom = atomWithStorage<LocalModelQuant>(
  'uclaw.localModel.quant',
  DEFAULT_QUANT,
)

/** Download lifecycle for the local-model settings section. */
export type LocalModelStatus =
  | { kind: 'unknown' }
  | { kind: 'not-downloaded' }
  | { kind: 'ready' }
  | {
      kind: 'downloading'
      phase: 'probing' | 'downloading' | 'verifying'
      source: string | null
      downloaded: number
      total: number
      percent: number
    }
  | { kind: 'error'; message: string }

export const localModelStatusAtom = atom<LocalModelStatus>({ kind: 'unknown' })
