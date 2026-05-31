// Owns a single ShortcutRow's state machine: the shortcutOverrides atom read/write,
// the capture + conflict UI state, the key-capture subscription, and the
// global-shortcut backend sync (updateGlobalShortcut). Extracted out of the
// ShortcutRow component during the features/settings migration (code-organization
// ADR 2026-05-31). The pure binding/conflict helpers live in lib/shortcut-binding;
// the typed @/lib/tauri-bridge updateGlobalShortcut IPC stays in the hook (precedent:
// useChannelSettings). Behavior preserved verbatim — same override map mutations,
// the same global-shortcut sync points, the same console.error fallbacks.
import * as React from 'react'
import { useAtom } from 'jotai'
import { shortcutOverridesAtom } from '@/atoms/shortcut-atoms'
import {
  SHORTCUT_DEFINITIONS,
  type ShortcutDefinition,
} from '@/lib/shortcut-defaults'
import { useShortcutCapture } from '@/hooks/useShortcutCapture'
import { updateGlobalShortcut } from '@/lib/tauri-bridge'
import { isMac, GLOBAL_SHORTCUT_IDS, effectiveBinding, findConflict } from '../lib/shortcut-binding'

export function useShortcutRow(def: ShortcutDefinition) {
  const [overrides, setOverrides] = useAtom(shortcutOverridesAtom)
  const [capturing, setCapturing] = React.useState(false)
  const [conflictCombo, setConflictCombo] = React.useState<string | null>(null)

  const binding = effectiveBinding(def, overrides)
  const defaultBinding = isMac ? def.mac : def.win
  const isCustomized =
    overrides[def.id] !== undefined &&
    ((isMac && overrides[def.id]!.mac !== undefined) ||
      (!isMac && overrides[def.id]!.win !== undefined))

  const writeOverride = React.useCallback(
    (combo: string) => {
      setOverrides((prev) => ({
        ...prev,
        [def.id]: {
          ...prev[def.id],
          ...(isMac ? { mac: combo } : { win: combo }),
        },
      }))
      // 同步全局快捷键到后端
      if (GLOBAL_SHORTCUT_IDS.includes(def.id)) {
        updateGlobalShortcut(def.id, combo).catch((e) =>
          console.error('[ShortcutSettings] Failed to sync global shortcut:', e),
        )
      }
    },
    [def.id, setOverrides],
  )

  const clearOverride = React.useCallback(() => {
    setOverrides((prev) => {
      if (!prev[def.id]) return prev
      const { [def.id]: _drop, ...rest } = prev
      return rest
    })
    setConflictCombo(null)
    // 重置全局快捷键为默认值
    if (GLOBAL_SHORTCUT_IDS.includes(def.id)) {
      const defaultCombo = (isMac ? def.mac : def.win) ?? ''
      updateGlobalShortcut(def.id, defaultCombo).catch((e) =>
        console.error('[ShortcutSettings] Failed to reset global shortcut:', e),
      )
    }
  }, [def.id, def.mac, def.win, setOverrides])

  useShortcutCapture({
    active: capturing,
    onCapture: (combo) => {
      setCapturing(false)
      if (combo === null) return  // Esc cancel
      if (combo === 'Backspace') {
        // Backspace alone (no modifiers) → clear binding entirely.
        writeOverride('')
        return
      }
      const conflict = findConflict(combo, def.id, overrides)
      if (conflict) {
        setConflictCombo(combo)
        return
      }
      writeOverride(combo)
    },
  })

  const conflictDef = conflictCombo ? findConflict(conflictCombo, def.id, overrides) : undefined

  const acceptConflictReplace = React.useCallback(() => {
    if (!conflictCombo || !conflictDef) return
    setOverrides((prev) => {
      const next = { ...prev }
      const otherDefault = isMac ? conflictDef.mac : conflictDef.win
      if (otherDefault === conflictCombo) {
        next[conflictDef.id] = {
          ...next[conflictDef.id],
          ...(isMac ? { mac: '' } : { win: '' }),
        }
      } else {
        const otherEntry = { ...(next[conflictDef.id] ?? {}) }
        if (isMac) delete otherEntry.mac
        else delete otherEntry.win
        if (otherEntry.mac === undefined && otherEntry.win === undefined) {
          delete next[conflictDef.id]
        } else {
          next[conflictDef.id] = otherEntry
        }
      }
      next[def.id] = {
        ...next[def.id],
        ...(isMac ? { mac: conflictCombo } : { win: conflictCombo }),
      }
      return next
    })
    // 同步全局快捷键变更
    if (GLOBAL_SHORTCUT_IDS.includes(conflictDef.id)) {
      // 被替换方的全局快捷键需要清除
      updateGlobalShortcut(conflictDef.id, '').catch((e) =>
        console.error('[ShortcutSettings] Failed to clear conflicting global shortcut:', e),
      )
    }
    if (GLOBAL_SHORTCUT_IDS.includes(def.id)) {
      // 当前快捷键重新注册新组合键
      updateGlobalShortcut(def.id, conflictCombo).catch((e) =>
        console.error('[ShortcutSettings] Failed to sync global shortcut after replace:', e),
      )
    }
    setConflictCombo(null)
  }, [conflictCombo, conflictDef, def.id, setOverrides])

  const toggleCapture = React.useCallback(() => {
    setConflictCombo(null)
    setCapturing((c) => !c)
  }, [])

  return {
    binding,
    defaultBinding,
    isCustomized,
    capturing,
    conflictCombo,
    conflictDef,
    clearOverride,
    acceptConflictReplace,
    dismissConflict: () => setConflictCombo(null),
    toggleCapture,
  }
}

// Reset-all logic shared by the panel header. Wipes the entire override map and
// resets every global shortcut to its default. Kept here (next to useShortcutRow)
// since it touches the same atom + the same global-shortcut sync. Behavior verbatim.
export function useResetAllShortcuts() {
  const [overrides, setOverrides] = useAtom(shortcutOverridesAtom)
  const hasAny = Object.keys(overrides).length > 0

  const resetAll = React.useCallback(() => {
    setOverrides({})
    // 将所有全局快捷键重置为默认值
    for (const id of GLOBAL_SHORTCUT_IDS) {
      if (overrides[id]) {
        const def = SHORTCUT_DEFINITIONS.find((d) => d.id === id)
        const defaultCombo = def ? (isMac ? def.mac : def.win) ?? '' : ''
        updateGlobalShortcut(id, defaultCombo).catch((e) =>
          console.error('[ShortcutSettings] Failed to reset global shortcut on reset-all:', e),
        )
      }
    }
  }, [overrides, setOverrides])

  return { hasAny, resetAll }
}
