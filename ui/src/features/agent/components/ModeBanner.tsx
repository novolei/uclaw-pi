/**
 * ModeBanner — inline pill at session top, only shown for Plan / AcceptEdits.
 * Other modes (Ask / Auto / Bypass) are self-evident from agent behavior or
 * obvious from the user's deliberate choice.
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Pencil, Map as MapIcon } from 'lucide-react'
import { cn } from '@/lib/utils'
import { safetyModeAtom } from '@/atoms/safety-atoms'

export function ModeBanner(): React.ReactElement | null {
  const mode = useAtomValue(safetyModeAtom)

  if (mode === 'acceptedits') {
    return (
      <div className={cn(
        'flex items-center gap-2 px-3 py-1.5 text-[12px] border-b',
        'border-blue-500/30 bg-blue-500/8 text-blue-700 dark:text-blue-400'
      )}>
        <Pencil className="size-3.5 shrink-0" />
        <span>Accept edits — file edits auto-pass; other tools ask</span>
      </div>
    )
  }

  if (mode === 'plan') {
    return (
      <div className={cn(
        'flex items-center gap-2 px-3 py-1.5 text-[12px] border-b',
        'border-purple-500/30 bg-purple-500/8 text-purple-700 dark:text-purple-400'
      )}>
        <MapIcon className="size-3.5 shrink-0" />
        <span>Plan mode — investigating only, no execution</span>
      </div>
    )
  }

  return null
}
