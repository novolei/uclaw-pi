import { atom } from 'jotai'

// ── Types ─────────────────────────────────────────────────────────────

export interface ScreencastFrameEntry {
  tabId: string
  dataB64: string
  mimeType?: string
  pageWidth: number
  pageHeight: number
  timestamp: number
}

export interface DOMElementEntry {
  index: number
  tag: string
  text: string
  isInViewport: boolean
  boundingBox?: { x: number; y: number; width: number; height: number }
}

export interface BrowserTabEntry {
  tabId: string
  url: string
  title: string
  active: boolean
}

export interface DOMStateEntry {
  url: string
  title: string
  elements: DOMElementEntry[]
  pageText: string
  tabs: BrowserTabEntry[]
  timestamp: number
}

// ── Atoms ─────────────────────────────────────────────────────────────

/** Latest CDP screencast frame per sessionId. */
export const browserScreencastFrameAtom = atom(new Map<string, ScreencastFrameEntry>())

/** Latest DOM state per sessionId (populated on demand by BrowserPanel). */
export const browserDOMStateAtom = atom(new Map<string, DOMStateEntry>())

/** Set of sessionIds that currently have an active screencast subscription. */
export const browserScreencastActiveAtom = atom(new Set<string>())

/** Whether the DOM element bounding-box overlay is visible in BrowserPanel. */
export const browserDOMOverlayVisibleAtom = atom(false)

export interface NavStateEntry {
  tabId: string
  url: string
  title: string
  isLoading: boolean
  canGoBack: boolean
  canGoForward: boolean
}

/** Latest nav state per sessionId. Populated by BrowserPanel's listenNavState subscription. */
export const browserNavStateAtom = atom(new Map<string, NavStateEntry>())

export type BrowserTaskStatus =
  | 'running'
  | 'completed'
  | 'failed'
  | 'stopped'
  | 'needs_user_intervention'
  | 'paused_waiting_for_browser_runtime'
  | 'paused_checkpointed'
export type BrowserTaskStepPhase = 'observe' | 'decide' | 'act' | 'recover' | 'user_intervention' | 'done'

export interface BrowserTaskStepEntry {
  stepIndex: number
  phase: BrowserTaskStepPhase
  observationSummary: string
  reasoning: string
  actionName: string
  actionArgs: unknown
  ok: boolean
  message: string | null
  error: string | null
  timestampMs: number
}

export interface BrowserTaskRunEntry {
  runId: string
  sessionId: string
  task: string
  status: BrowserTaskStatus
  steps: BrowserTaskStepEntry[]
}

/** Latest autonomous browser task run per sessionId. */
export const browserTaskRunAtom = atom(new Map<string, BrowserTaskRunEntry>())

// ── V1 compatibility shims (Sprint 2.2 hotfix) ────────────────────────
//
// PR #213 (Browser Agent v2) rewrote this file's atom surface from the
// V1 single-global model (`browserStateAtom`, `isBrowserLoadingAtom`,
// type `BrowserState`) to the V2 session-keyed Map model (above). Two
// legacy consumers were NOT migrated in the same PR and still import the
// V1 names:
//
//   ui/src/features/agent/components/BrowserViewer.tsx  ← mounted by RightSidePanel
//   ui/src/components/canvas/BrowserViewer.tsx          ← mounted by TabContent
//
// Without these names exported, vite fails the module graph at boot
// (`SyntaxError: Importing binding name 'isBrowserLoadingAtom' is not
// found`) and the entire app renders blank.
//
// Re-export V1 names as no-op stubs so the legacy components keep
// compiling + rendering their static "Browser Idle" UI. They lose the
// actual data flow (which now lives in V2 atoms keyed by sessionId) —
// that's acceptable because launch/shutdown control is migrating to
// BrowserPanel anyway. Proper migration: delete agent/BrowserViewer +
// canvas/BrowserViewer entirely and wire BrowserPanel into the two
// mount points. Tracked as a separate PR; this hotfix only unblocks
// the white screen.
//
// DO NOT add new code that depends on these V1 atoms. Use the V2
// session-keyed Map atoms above.

/** @deprecated V1 — use V2 session-keyed atoms above. */
export interface BrowserState {
  running: boolean
  tabs: BrowserTabEntry[]
  activeTabId: string | null
}

/** @deprecated V1 — no-op stub; superseded by V2 session-keyed atoms. */
export const browserStateAtom = atom<BrowserState>({
  running: false,
  tabs: [],
  activeTabId: null,
})

/** @deprecated V1 — no-op stub; superseded by V2 session-keyed atoms. */
export const isBrowserLoadingAtom = atom<boolean>(false)
