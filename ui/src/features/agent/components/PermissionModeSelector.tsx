/**
 * PermissionModeSelector — input-bar trigger button that opens the
 * 5-mode PermissionModeMenu popover. Backed by the real SafetyManager
 * (PR #42 wired this — see tauri-bridge.ts::setSafetyMode).
 *
 * Keyboard: Shift+Cmd+M (Mac) / Shift+Ctrl+M (other) opens the menu.
 */

import * as React from 'react'
import { useAtom, useSetAtom } from 'jotai'
import { safetyModeAtom } from '@/atoms/safety-atoms'
import { silencedPlanModeSessionsAtom } from '@/atoms/plan-mode-suggest-atoms'
import { getSafetyPolicy, setSafetyMode, type SafetyModeWire } from '@/lib/bridge/agent'
import { PermissionModeMenu, MODE_ITEMS } from './PermissionModeMenu'

export interface PermissionModeSelectorProps {
  /** Kept for prop compat — global SafetyMode is workspace-agnostic. */
  sessionId?: string
}

export function PermissionModeSelector(_: PermissionModeSelectorProps): React.ReactElement | null {
  const [mode, setMode] = useAtom(safetyModeAtom)
  const setSilenced = useSetAtom(silencedPlanModeSessionsAtom)
  const [open, setOpen] = React.useState(false)
  const [busy, setBusy] = React.useState(false)

  // Hydrate from backend on mount.
  React.useEffect(() => {
    getSafetyPolicy()
      .then((p) => setMode(p.globalMode as SafetyModeWire))
      .catch((e) => console.error('[PermissionModeSelector] getSafetyPolicy failed:', e))
    // eslint-disable-next-line react-hooks/exhaustive-deps -- run once
  }, [])

  // Global keyboard shortcut: Shift+Cmd+M opens the menu.
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.shiftKey && (e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'm') {
        e.preventDefault()
        setOpen((v) => !v)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const onPick = React.useCallback(async (next: SafetyModeWire) => {
    if (busy) return
    setBusy(true)
    try {
      await setSafetyMode({ mode: next })
      setMode(next)
      // User explicitly changed mode → clear silenced sessions so the
      // banner can re-fire if the next message matches again.
      setSilenced(new Set())
    } catch (err) {
      console.error('[PermissionModeSelector] setSafetyMode failed:', err)
    } finally {
      setBusy(false)
      requestAnimationFrame(() => document.querySelector<HTMLElement>('.ProseMirror')?.focus())
    }
  }, [busy, setMode])

  const current = MODE_ITEMS.find((m) => m.wire === mode) ?? MODE_ITEMS[3]!  // default to Auto
  const Icon = current.icon
  const isNonDefault = current.wire !== 'supervised'

  const trigger = (
    <button
      type="button"
      disabled={busy}
      className={`flex items-center gap-1 px-1.5 py-1 rounded text-xs font-medium transition-colors hover:text-foreground disabled:opacity-50 ${
        isNonDefault ? current.triggerColorClass : 'text-muted-foreground'
      }`}
    >
      <Icon className="size-3.5" />
      <span className="hidden sm:inline">{current.label}</span>
      <span className="text-[10px] opacity-60">▾</span>
    </button>
  )

  return (
    <PermissionModeMenu
      current={mode}
      onPick={(m) => void onPick(m)}
      open={open}
      onOpenChange={setOpen}
      trigger={trigger}
    />
  )
}
