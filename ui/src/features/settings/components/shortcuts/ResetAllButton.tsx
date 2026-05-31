/**
 * ResetAllButton — the panel header's "重置全部" action (wipes every override).
 * Split out of legacy settings/ShortcutSettings.tsx during the features/settings
 * migration (code-organization ADR 2026-05-31); the atom + global-shortcut reset
 * loop live in useResetAllShortcuts. Pure presentation. Behavior preserved verbatim.
 */
import * as React from 'react'
import { RotateCcw } from 'lucide-react'
import { cn } from '@/lib/utils'
import { useResetAllShortcuts } from '../../hooks/useShortcutRow'

export function ResetAllButton(): React.ReactElement {
  const { hasAny, resetAll } = useResetAllShortcuts()

  return (
    <button
      type="button"
      onClick={resetAll}
      disabled={!hasAny}
      title={hasAny ? '清除全部自定义快捷键，恢复默认' : '没有自定义项'}
      aria-label="重置全部"
      className={cn(
        'inline-flex items-center gap-[5px] px-2 py-[3px] rounded-md text-[11px]',
        'border border-transparent transition-all whitespace-nowrap',
        hasAny
          ? 'text-muted-foreground hover:text-foreground hover:bg-foreground/[0.04] hover:border-border/60 cursor-pointer'
          : 'text-foreground/25 cursor-not-allowed',
      )}
    >
      <RotateCcw className="size-[11px]" />
      重置全部
    </button>
  )
}
