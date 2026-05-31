/**
 * PermissionModeMenu — Radix Popover dropdown matching Claude Code's
 * 5-mode selector. Listens for keyboard shortcuts:
 *   - Shift+Cmd+M (Mac) / Shift+Ctrl+M (Win/Linux) — open
 *   - 1-5 (when open) — select corresponding mode
 *   - Esc — close
 *
 * The popover content is owned by this component; the trigger button
 * (with the current-mode label + chevron) is rendered by PermissionModeSelector.
 */

import * as React from 'react'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { ShieldQuestion, Pencil, Map as MapIcon, Compass, Zap, Check } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { SafetyModeWire } from '@/lib/bridge/agent'

export interface ModeMenuItem {
  wire: SafetyModeWire
  label: string
  icon: React.ComponentType<{ className?: string }>
  numberKey: '1' | '2' | '3' | '4' | '5'
  triggerColorClass: string  // applied to the trigger button when this is current
}

export const MODE_ITEMS: ModeMenuItem[] = [
  { wire: 'ask',         label: 'Ask permissions',   icon: ShieldQuestion, numberKey: '1', triggerColorClass: 'text-yellow-600' },
  { wire: 'acceptedits', label: 'Accept edits',      icon: Pencil,         numberKey: '2', triggerColorClass: 'text-blue-600' },
  { wire: 'plan',        label: 'Plan mode',         icon: MapIcon,        numberKey: '3', triggerColorClass: 'text-purple-600' },
  { wire: 'supervised',  label: 'Auto mode',         icon: Compass,        numberKey: '4', triggerColorClass: 'text-foreground/70' },
  { wire: 'yolo',        label: 'Bypass permissions',icon: Zap,            numberKey: '5', triggerColorClass: 'text-amber-600' },
]

export interface PermissionModeMenuProps {
  current: SafetyModeWire
  onPick: (mode: SafetyModeWire) => void
  open: boolean
  onOpenChange: (open: boolean) => void
  trigger: React.ReactNode
}

export function PermissionModeMenu({ current, onPick, open, onOpenChange, trigger }: PermissionModeMenuProps): React.ReactElement {
  // Keyboard handler when open
  React.useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      const item = MODE_ITEMS.find((m) => m.numberKey === e.key)
      if (item) {
        e.preventDefault()
        onPick(item.wire)
        onOpenChange(false)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [open, onPick, onOpenChange])

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>{trigger}</PopoverTrigger>
      <PopoverContent side="top" align="start" className="w-[280px] p-1">
        <div className="flex items-center justify-between px-2 py-1.5 border-b border-border/50 mb-1">
          <span className="text-[11px] font-medium text-muted-foreground/70">Mode</span>
          <span className="flex items-center gap-1">
            <kbd className="rounded bg-muted px-1 py-0.5 text-[10px] font-mono">⇧</kbd>
            <kbd className="rounded bg-muted px-1 py-0.5 text-[10px] font-mono">⌘</kbd>
            <kbd className="rounded bg-muted px-1 py-0.5 text-[10px] font-mono">M</kbd>
          </span>
        </div>
        <ul role="menu" className="space-y-px">
          {MODE_ITEMS.map((m) => {
            const Icon = m.icon
            const active = m.wire === current
            return (
              <li key={m.wire}>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => { onPick(m.wire); onOpenChange(false) }}
                  className={cn(
                    'flex w-full items-center gap-2 px-2 py-1.5 rounded text-[12.5px] hover:bg-muted',
                    active && 'bg-muted/60'
                  )}
                >
                  <Icon className={cn('size-3.5 shrink-0', m.triggerColorClass)} />
                  <span className="flex-1 text-left">{m.label}</span>
                  {active && <Check className="size-3.5 text-foreground/70 mr-1" />}
                  <span className="text-[10.5px] text-muted-foreground/60 tabular-nums w-3 text-right">
                    {m.numberKey}
                  </span>
                </button>
              </li>
            )
          })}
        </ul>
      </PopoverContent>
    </Popover>
  )
}
