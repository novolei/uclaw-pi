/**
 * ShortcutSettings — keybinding management UI (v3).
 *
 * Thin panel root: data-driven from SHORTCUT_DEFINITIONS, grouped, composing the
 * split pieces — shortcuts/ShortcutRow (per binding, state in useShortcutRow),
 * shortcuts/KbdCluster (the key-cap rendering), shortcuts/ResetAllButton (header
 * action) + lib/shortcut-binding helpers. Split out of the 433-line
 * legacy settings/ShortcutSettings.tsx during the features/settings migration
 * (code-organization ADR 2026-05-31). Behavior preserved verbatim.
 *
 * Interaction (unchanged): click a kbd cluster → capture mode; press a combo →
 * captured (Esc cancels, Backspace alone clears); conflicts show an amber banner
 * with Replace / Cancel; "重置全部" wipes the override map. Global shortcuts
 * (quick-memory-voice / clipboard-capture-silent) sync to the backend on change.
 */
import * as React from 'react'
import { getShortcutsByGroup } from '@/lib/shortcut-defaults'
import { ShortcutRow } from './shortcuts/ShortcutRow'
import { ResetAllButton } from './shortcuts/ResetAllButton'

export function ShortcutSettings(): React.ReactElement {
  const groups = React.useMemo(() => getShortcutsByGroup(), [])
  const groupNames = Object.keys(groups)
  return (
    <div className="space-y-4">
      {/* No h2 — the settings nav rail already labels this section "快捷键". */}
      <div className="flex items-center justify-between gap-3 pb-3 border-b border-border/50">
        <p className="text-[11px] text-muted-foreground leading-[1.55] m-0">
          点击组合键卡片可录入新组合 ·{' '}
          <kbd className="font-mono text-[10px] bg-foreground/[0.06] px-[5px] py-[0.5px] rounded">Esc</kbd>{' '}
          取消 ·{' '}
          <kbd className="font-mono text-[10px] bg-foreground/[0.06] px-[5px] py-[0.5px] rounded">⌫</kbd>{' '}
          清除绑定
        </p>
        <ResetAllButton />
      </div>
      {groupNames.map((group) => (
        <section key={group}>
          <div className="flex items-baseline gap-[7px] px-[2px] mb-1.5">
            <span className="text-[10.5px] uppercase tracking-[0.55px] font-semibold text-muted-foreground">
              {group}
            </span>
            <span className="text-[10px] text-foreground/30 tabular-nums">
              {groups[group]!.length} 项
            </span>
          </div>
          <div className="rounded-[10px] bg-card border border-border/60 overflow-hidden">
            {groups[group]!.map((def, i) => (
              <React.Fragment key={def.id}>
                {i > 0 && <div className="border-t border-border/40" />}
                <ShortcutRow def={def} />
              </React.Fragment>
            ))}
          </div>
        </section>
      ))}
    </div>
  )
}
