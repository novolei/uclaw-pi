/**
 * KbdCluster + KeyCap — the macOS-style key-cap rendering of a keybinding (one cap
 * per modifier + final key), plus the capture / "未绑定" affordances. Split out of
 * components/settings/ShortcutSettings.tsx during the features/settings migration
 * (code-organization ADR 2026-05-31). Pure presentation. Behavior preserved verbatim.
 */
import * as React from 'react'
import { parseShortcutTokens, type ShortcutToken } from '@/lib/shortcut-defaults'
import { cn } from '@/lib/utils'

function KeyCap({ token }: { token: ShortcutToken }): React.ReactElement {
  return (
    <span
      className={cn(
        'inline-flex items-center justify-center h-[17px] rounded-[3.5px]',
        'bg-card border border-foreground/15',
        'shadow-[0_0.5px_0_rgba(0,0,0,0.04),inset_0_-0.5px_0_rgba(0,0,0,0.02)]',
        'text-foreground leading-none font-medium',
        token.kind === 'mod'
          ? 'min-w-[17px] px-[3px] text-[11.5px]'
          : 'min-w-[17px] px-[4px] text-[10.5px]',
      )}
    >
      {token.display}
    </span>
  )
}

interface KbdClusterProps {
  binding: string
  capturing: boolean
  onClick: () => void
}

export function KbdCluster({ binding, capturing, onClick }: KbdClusterProps): React.ReactElement {
  const tokens = React.useMemo(() => parseShortcutTokens(binding), [binding])

  if (capturing) {
    return (
      <button
        type="button"
        onClick={onClick}
        aria-label="取消录入"
        className={cn(
          'inline-flex items-center gap-1.5 h-[24px] px-[10px] rounded-md',
          'bg-primary/10 border border-dashed border-primary/70',
          'text-primary text-[10.5px] font-medium leading-none cursor-text',
        )}
      >
        <span>按下组合键</span>
        <span className="inline-block w-[1.5px] h-[10px] bg-primary animate-pulse" />
      </button>
    )
  }

  if (tokens.length === 0) {
    return (
      <button
        type="button"
        onClick={onClick}
        aria-label="点击录入新组合"
        title="点击录入新组合"
        className={cn(
          'inline-flex items-center h-[24px] px-[10px] rounded-md',
          'bg-transparent border border-dashed border-foreground/20',
          'text-foreground/40 text-[10.5px] italic leading-none',
          'hover:border-foreground/35 hover:text-foreground/55 transition-colors',
        )}
      >
        未绑定
      </button>
    )
  }

  return (
    <button
      type="button"
      onClick={onClick}
      aria-label="点击录入新组合"
      title="点击录入新组合"
      className={cn(
        'inline-flex items-center gap-[2px] h-[24px] px-[4px] rounded-md',
        'bg-foreground/[0.025] border border-foreground/[0.06]',
        'shadow-[inset_0_-1px_0_rgba(0,0,0,0.025)]',
        'hover:bg-foreground/[0.045] hover:border-foreground/[0.10] transition-colors',
      )}
    >
      {tokens.map((t, i) => (
        <KeyCap key={i} token={t} />
      ))}
    </button>
  )
}
