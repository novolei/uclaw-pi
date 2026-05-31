/**
 * ActivityRow — one tool-activity row: status icon + semantic phrase (running →
 * progressive form), optional Error badge / elapsed time / 预览 (preview) button,
 * and an expand chevron when there is a result to open. Live bash output streams
 * inline under the row via BashStreamView. Extracted from ToolActivityItem.tsx
 * during the features/agent migration split (it is the unit covered by
 * ToolActivityItem.test.tsx).
 */

import * as React from 'react'
import { ChevronRight } from 'lucide-react'
import { useSetAtom } from 'jotai'
import { cn } from '@/lib/utils'
import { formatElapsed, getToolPhrase, BashStreamView } from '@/shared/tool-rendering'
import { type ToolActivity, getActivityStatus } from '@/atoms/agent-atoms'
import { openPreviewTabAction } from '@/atoms/preview-panel-atoms'
import { SIZE } from './constants'
import { StatusIcon, ErrorBadge, renderLabelWithDiff } from './row-bits'

// ===== 预览按钮资格判断 =====

const PREVIEW_ELIGIBLE_TOOLS = new Set(['write_file', 'edit', 'plan_write'])

function shouldShowPreviewButton(
  toolName: string,
  input: Record<string, unknown>,
): boolean {
  if (!PREVIEW_ELIGIBLE_TOOLS.has(toolName)) return false
  const path = (input.path ?? input.file_path) as string | undefined
  return Boolean(path && path.length > 0)
}

// ===== 活动行 =====

export interface ActivityRowProps {
  activity: ToolActivity
  index?: number
  animate?: boolean
  onOpenDetails?: (activity: ToolActivity) => void
}

export function ActivityRow({ activity, index = 0, animate = false, onOpenDetails }: ActivityRowProps): React.ReactElement {
  const status = getActivityStatus(activity)
  const phrase = getToolPhrase(activity.toolName, activity.input)
  const isRunning = status === 'running' || status === 'backgrounded'

  // 运行中显示进行时短语，完成后显示完成态短语
  const displayLabel = isRunning ? phrase.loadingLabel : phrase.label

  const delay = animate && index < SIZE.staggerLimit ? `${index * 30}ms` : '0ms'

  const canExpand = !!onOpenDetails && activity.done && !!(activity.result || Object.keys(activity.input).length > 0)

  const openPreviewTab = useSetAtom(openPreviewTabAction)

  const handlePreview = React.useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation()
      const path = (activity.input.path ?? activity.input.file_path) as string | undefined
      if (!path) return
      openPreviewTab({
        target: {
          mountId: 'workspace:default',
          relPath: path,
          name: path.split('/').pop() ?? path,
          absolutePath: path,
          sessionId: undefined,
        },
        source: 'agent',
      })
    },
    [activity.input, openPreviewTab],
  )

  const rowContent = (
    <>
      <StatusIcon status={status} toolName={activity.toolName} />
      <span className={cn(
        'truncate min-w-0 flex-1 text-[12px] transition-colors duration-150',
        isRunning ? 'text-foreground/55' : 'text-foreground/70',
        canExpand && 'group-hover/row:text-foreground/90',
      )}>
        {renderLabelWithDiff(displayLabel, activity.toolName)}
      </span>
      {activity.isError && <ErrorBadge />}
      {activity.elapsedSeconds !== undefined && activity.elapsedSeconds > 0 && (
        <span className="shrink-0 text-[10.5px] text-muted-foreground/45 tabular-nums font-mono">
          {formatElapsed(activity.elapsedSeconds)}
        </span>
      )}
      {shouldShowPreviewButton(activity.toolName, activity.input) && (
        <button
          type="button"
          onClick={handlePreview}
          className="shrink-0 px-2 py-0.5 text-[11px] text-muted-foreground hover:text-foreground hover:bg-muted/60 rounded border border-border/40 transition-colors"
          aria-label={`预览 ${(activity.input.path ?? activity.input.file_path) as string}`}
        >
          预览
        </button>
      )}
      {canExpand && (
        <ChevronRight className="size-2.5 shrink-0 text-muted-foreground/30 group-hover/row:text-muted-foreground/70 transition-all duration-150" />
      )}
    </>
  )

  const liveBash =
    activity.toolName === 'bash' && !activity.done && activity.liveOutput && activity.liveOutput.segments.length > 0
      ? activity.liveOutput
      : null

  return (
    <div
      className={cn(
        animate && 'animate-in fade-in slide-in-from-left-2 duration-200 fill-mode-both',
      )}
      style={animate ? { animationDelay: delay } : undefined}
    >
      {canExpand ? (
        // Use div[role=button] instead of <button> so we never get a button-in-button
        // when the 预览 button is also present in rowContent (invalid DOM per spec).
        <div
          role="button"
          tabIndex={0}
          className={cn(
            'group/row w-full flex items-center gap-2 px-2.5 rounded-lg cursor-pointer transition-colors duration-100 hover:bg-muted/50',
            SIZE.row,
          )}
          onClick={(e) => { e.stopPropagation(); onOpenDetails(activity) }}
          onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.stopPropagation(); onOpenDetails(activity) } }}
        >
          {rowContent}
        </div>
      ) : (
        <div className={cn('group/row flex items-center gap-2 px-2.5 rounded-lg', SIZE.row)}>
          {rowContent}
        </div>
      )}
      {liveBash && (
        <div className="mx-2 mt-1">
          <BashStreamView command={(activity.input.command as string) ?? ''} live={liveBash} logPath={undefined} />
        </div>
      )}
    </div>
  )
}
