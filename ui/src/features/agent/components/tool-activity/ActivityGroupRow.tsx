/**
 * ActivityGroupRow — a Task/Agent sub-agent group: a collapsible header row
 * (derived status across children, subagent-type pill, elapsed, done/total) that
 * expands to the child ActivityRows (+ their ActivityDetails). Extracted from
 * ToolActivityItem.tsx during the features/agent migration split.
 */

import * as React from 'react'
import { ChevronRight } from 'lucide-react'
import { cn } from '@/lib/utils'
import { formatElapsed, getToolPhrase } from '@/shared/tool-rendering'
import {
  type ToolActivity,
  type ActivityGroup,
  type ActivityStatus,
  getActivityStatus,
} from '@/atoms/agent-atoms'
import { SIZE } from './constants'
import { StatusIcon } from './row-bits'
import { ActivityRow } from './ActivityRow'
import { ActivityDetails } from './ActivityDetails'

interface ActivityGroupRowProps {
  group: ActivityGroup
  index?: number
  animate?: boolean
  onOpenDetails?: (activity: ToolActivity) => void
  detailsId?: string | null
  onCloseDetails?: () => void
}

export function ActivityGroupRow({ group, index = 0, animate = false, onOpenDetails, detailsId, onCloseDetails }: ActivityGroupRowProps): React.ReactElement {
  const { parent, children } = group
  // Agent 子代理默认折叠，Task 子代理默认展开
  const [expanded, setExpanded] = React.useState(parent.toolName !== 'Agent')

  const derivedStatus = React.useMemo((): ActivityStatus => {
    const selfStatus = getActivityStatus(parent)
    if (selfStatus === 'completed' || selfStatus === 'error') return selfStatus
    if (children.length > 0 && children.every((c) => c.done)) {
      if (children.some((c) => c.isError)) return 'error'
      if (parent.done) return 'completed'
    }
    return selfStatus
  }, [parent, children])

  const phrase = getToolPhrase(parent.toolName, parent.input)
  const isRunning = derivedStatus === 'running' || derivedStatus === 'backgrounded'
  const displayLabel = isRunning ? phrase.loadingLabel : phrase.label

  const subagentType = typeof parent.input.subagent_type === 'string'
    ? parent.input.subagent_type
    : undefined

  const delay = animate && index < SIZE.staggerLimit ? `${index * 30}ms` : '0ms'

  return (
    <div
      className={cn(
        'w-full',
        animate && 'animate-in fade-in slide-in-from-left-2 duration-200 fill-mode-both',
      )}
      style={animate ? { animationDelay: delay } : undefined}
    >
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className={cn(
          'w-full flex items-center gap-2 px-2.5 text-left text-[12px] rounded-lg hover:bg-muted/50 transition-colors duration-100 cursor-pointer',
          SIZE.row,
        )}
      >
        <ChevronRight
          className={cn(
            'size-2.5 text-muted-foreground/60 transition-transform duration-150',
            expanded && 'rotate-90',
          )}
        />

        <StatusIcon status={derivedStatus} toolName={parent.toolName} />

        {subagentType && (
          <span className="shrink-0 px-1.5 py-0.5 rounded bg-primary/10 text-primary text-[9px] font-medium leading-none">
            {subagentType}
          </span>
        )}

        <span className="truncate flex-1 min-w-0 text-foreground/80">{displayLabel}</span>

        {parent.elapsedSeconds !== undefined && parent.elapsedSeconds > 0 && (
          <span className="shrink-0 text-[11px] text-muted-foreground/60 tabular-nums">
            {formatElapsed(parent.elapsedSeconds)}
          </span>
        )}

        {children.length > 0 && (
          <span className="shrink-0 text-[10px] text-muted-foreground/50 tabular-nums">
            {children.filter((c) => c.done).length}/{children.length}
          </span>
        )}
      </button>

      {expanded && children.length > 0 && (
        <div
          className={cn(
            'pl-6 pr-1 space-y-0 border-l-2 border-muted ml-[7px]',
            'animate-in fade-in slide-in-from-top-1 duration-150',
          )}
        >
          {children.map((child, ci) => (
            <React.Fragment key={child.toolUseId}>
              <ActivityRow
                activity={child}
                index={ci}
                animate={animate}
                onOpenDetails={onOpenDetails}
              />
              {detailsId === child.toolUseId && (
                <ActivityDetails activity={child} onClose={onCloseDetails ?? (() => {})} />
              )}
            </React.Fragment>
          ))}
        </div>
      )}
    </div>
  )
}
