/**
 * ShortcutRow — one keybinding row: label + customized badge, reset-to-default
 * icon, the KbdCluster capture chip, and the inline conflict banner. Split out of
 * components/settings/ShortcutSettings.tsx during the features/settings migration
 * (code-organization ADR 2026-05-31); all state + the global-shortcut IPC sync live
 * in useShortcutRow. Pure presentation. Behavior preserved verbatim.
 */
import * as React from 'react'
import { RotateCcw, AlertTriangle } from 'lucide-react'
import type { ShortcutDefinition } from '@/lib/shortcut-defaults'
import { cn } from '@/lib/utils'
import { useShortcutRow } from '../../hooks/useShortcutRow'
import { KbdCluster } from './KbdCluster'

export function ShortcutRow({ def }: { def: ShortcutDefinition }): React.ReactElement {
  const {
    binding,
    defaultBinding,
    isCustomized,
    capturing,
    conflictCombo,
    conflictDef,
    clearOverride,
    acceptConflictReplace,
    dismissConflict,
    toggleCapture,
  } = useShortcutRow(def)

  return (
    <div className="flex flex-col group">
      <div
        className={cn(
          'flex items-center justify-between gap-3 px-3 py-[7px] min-h-[30px]',
          'transition-colors hover:bg-foreground/[0.012]',
        )}
      >
        <span className="text-[12px] text-foreground inline-flex items-center gap-1.5 min-w-0">
          <span className="truncate">{def.label}</span>
          {isCustomized && (
            <span className="shrink-0 px-[5px] py-[0.5px] rounded-full bg-primary/10 text-primary text-[9px] font-medium leading-[1.5] tracking-wide">
              已自定义
            </span>
          )}
        </span>
        <div className="flex items-center gap-[5px] shrink-0">
          <button
            type="button"
            onClick={clearOverride}
            disabled={!isCustomized}
            aria-label="重置为默认"
            title={isCustomized ? `重置为默认（${defaultBinding}）` : '已是默认值'}
            className={cn(
              'inline-flex w-[22px] h-[22px] items-center justify-center rounded-[5px]',
              'transition-all',
              isCustomized
                ? 'text-muted-foreground opacity-0 group-hover:opacity-70 hover:!opacity-100 hover:bg-foreground/[0.05] hover:text-foreground cursor-pointer'
                : 'text-foreground/15 opacity-0 cursor-not-allowed',
            )}
          >
            <RotateCcw className="size-[11px]" />
          </button>
          <KbdCluster
            binding={binding}
            capturing={capturing}
            onClick={toggleCapture}
          />
        </div>
      </div>
      {conflictCombo && conflictDef && (
        <div
          className={cn(
            'mx-3 mb-2 flex items-start gap-2 rounded-md px-2.5 py-1.5',
            'bg-amber-50/85 dark:bg-amber-900/20',
            'border border-amber-200/70 dark:border-amber-700/35',
            'text-amber-900 dark:text-amber-200 text-[10.5px] leading-relaxed',
          )}
        >
          <AlertTriangle className="size-3 shrink-0 mt-[2px]" aria-hidden />
          <div className="flex-1">
            <span className="font-mono">{conflictCombo}</span>
            <span className="mx-1">已被</span>
            <span className="font-medium">「{conflictDef.label}」</span>
            <span>使用。要替换吗？被替换方将清除其当前绑定。</span>
          </div>
          <div className="flex gap-1 shrink-0">
            <button
              type="button"
              onClick={acceptConflictReplace}
              className="px-2 py-[1px] rounded bg-amber-600 text-white text-[10px] font-medium hover:opacity-90"
            >
              替换
            </button>
            <button
              type="button"
              onClick={dismissConflict}
              className="px-2 py-[1px] rounded text-[10px] hover:bg-amber-100/70 dark:hover:bg-amber-800/30"
            >
              取消
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
