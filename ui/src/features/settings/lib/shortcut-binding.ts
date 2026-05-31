// Pure keybinding helpers for ShortcutSettings: platform detection, the global-
// shortcut id list (these sync to the backend on change), and the effective-
// binding / conflict-lookup functions. Extracted out of
// components/settings/ShortcutSettings.tsx during the features/settings migration
// (code-organization ADR 2026-05-31). No React, no IPC — behavior preserved verbatim.
import {
  SHORTCUT_DEFINITIONS,
  type ShortcutDefinition,
} from '@/lib/shortcut-defaults'
import type { ShortcutOverrides } from '@/lib/chat-types'

export const isMac =
  typeof navigator !== 'undefined' && /Mac|iPod|iPhone|iPad/.test(navigator.userAgent)

/** 全局快捷键 ID 列表：这些快捷键通过系统级全局注册，修改时需同步后端 */
export const GLOBAL_SHORTCUT_IDS = ['quick-memory-voice', 'clipboard-capture-silent']

export function effectiveBinding(def: ShortcutDefinition, overrides: ShortcutOverrides): string {
  const override = overrides[def.id]
  if (override) {
    const v = isMac ? override.mac : override.win
    if (v !== undefined) return v  // empty string is a legitimate "unbound" override
  }
  return (isMac ? def.mac : def.win) ?? ''
}

export function findConflict(
  combo: string,
  selfId: string,
  overrides: ShortcutOverrides,
): ShortcutDefinition | undefined {
  if (!combo) return undefined
  for (const d of SHORTCUT_DEFINITIONS) {
    if (d.id === selfId) continue
    if (effectiveBinding(d, overrides) === combo) return d
  }
  return undefined
}
