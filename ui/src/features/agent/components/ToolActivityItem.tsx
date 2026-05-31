/**
 * ToolActivityItem — 紧凑列表式工具活动展示（orchestrator）
 *
 * 核心设计：
 * - 收起行：语义化短语（如 "读取 foo.ts 第 10-60 行"），替代工具名 + Badge + 摘要
 * - 展开面板：按工具类型结构化渲染结果，无输入区
 * - Loading 态：语义化进行时（如 "正在读取 foo.ts..."）
 * - Task/Agent 子代理折叠分组 + 左边框层级
 * - CSS 动画（交错入场 / 状态切换）
 *
 * features/agent migration split (was 693 lines): the row family was extracted
 * into `tool-activity/` (ActivityRow / ActivityGroupRow / ActivityDetails +
 * row-bits + constants), and the attachment-image decode into
 * `hooks/useAttachmentImage`. This file is now the list orchestration only
 * (grouping, the Tools card header, collapse/auto-scroll, the task-card hand-off).
 */

import * as React from 'react'
import { CheckCircle2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  type ToolActivity,
  groupActivities,
  isActivityGroup,
} from '@/atoms/agent-atoms'
import { TaskProgressCard, TASK_TOOL_NAMES } from '@/features/agent'
import { formatElapsed } from '@/shared/tool-rendering'
import { SIZE } from './tool-activity/constants'
import { ActivityRow } from './tool-activity/ActivityRow'
import { ActivityGroupRow } from './tool-activity/ActivityGroupRow'
import { ActivityDetails } from './tool-activity/ActivityDetails'

// ===== 主导出：活动列表 =====

interface ToolActivityListProps {
  activities: ToolActivity[]
  animate?: boolean
}

export function ToolActivityList({ activities, animate = false }: ToolActivityListProps): React.ReactElement | null {
  const [detailsId, setDetailsId] = React.useState<string | null>(null)
  const [expanded, setExpanded] = React.useState(false)
  const listRef = React.useRef<HTMLDivElement>(null)

  const grouped = React.useMemo(() => groupActivities(activities), [activities])

  // 提取所有 task 相关的活动用于聚合卡片
  const taskActivities = React.useMemo(
    () => activities.filter((a) => TASK_TOOL_NAMES.has(a.toolName)),
    [activities]
  )
  const hasTaskCard = taskActivities.length > 0
  // 从原始 activities 查找第一个 task 工具的 toolUseId，定位卡片插入点
  const firstTaskToolUseId = React.useMemo(
    () => activities.find((a) => TASK_TOOL_NAMES.has(a.toolName))?.toolUseId ?? null,
    [activities]
  )

  const visibleRows = React.useMemo(() => {
    let count = 0
    for (const item of grouped) {
      if (!isActivityGroup(item) && TASK_TOOL_NAMES.has((item as ToolActivity).toolName)) {
        // task 工具聚合为一张卡片，不计入独立行数
        continue
      }
      count += 1
      if (isActivityGroup(item)) {
        count += item.children.length
      }
    }
    // 卡片占 1 行
    if (hasTaskCard) count += 1
    return count
  }, [grouped, hasTaskCard])

  const needsCollapse = visibleRows > SIZE.autoScrollThreshold

  // 流式模式：自动滚动到底部
  React.useEffect(() => {
    if (animate && listRef.current && needsCollapse) {
      listRef.current.scrollTop = listRef.current.scrollHeight
    }
  }, [visibleRows, needsCollapse, animate])

  if (activities.length === 0) return null

  const detailActivity = detailsId ? activities.find((a) => a.toolUseId === detailsId) : null

  const handleOpenDetails = (activity: ToolActivity): void => {
    setDetailsId((prev) => (prev === activity.toolUseId ? null : activity.toolUseId))
  }

  const isCollapsed = !animate && needsCollapse && !expanded

  // Count running vs done for header badge
  const doneCount = activities.filter((a) => a.done).length
  const totalCount = activities.length
  const allDone = doneCount === totalCount

  return (
    <div className="w-full rounded-xl border border-border/40 bg-card/40 overflow-hidden shadow-sm">
      {/* Card header */}
      <div className="flex items-center justify-between px-3 py-1.5 border-b border-border/30 bg-muted/20">
        <div className="flex items-center gap-1.5">
          {!allDone && (
            <span className="relative flex size-2 shrink-0">
              <span className="absolute inset-0 rounded-full bg-blue-400/40 animate-ping" style={{ animationDuration: '1.5s' }} />
              <span className="size-2 rounded-full bg-blue-400/70" />
            </span>
          )}
          {allDone && <CheckCircle2 className="size-3 text-emerald-500/70 shrink-0" />}
          <span className="text-[11px] font-medium text-muted-foreground/60 uppercase tracking-wide select-none">
            Tools
          </span>
        </div>
        <span className="text-[10.5px] text-muted-foreground/40 tabular-nums font-mono">
          {doneCount}/{totalCount}
        </span>
      </div>

      {/* Activity rows */}
      <div
        ref={listRef}
        className={cn(
          'py-1',
          animate && needsCollapse && 'overflow-y-auto',
          isCollapsed && 'overflow-hidden',
        )}
        style={
          animate && needsCollapse
            ? { maxHeight: SIZE.autoScrollThreshold * SIZE.rowHeight }
            : isCollapsed
              ? { maxHeight: SIZE.autoScrollThreshold * SIZE.rowHeight }
              : undefined
        }
      >
        {grouped.map((item, i) => {
          if (isActivityGroup(item)) {
            return (
              <ActivityGroupRow
                key={item.parent.toolUseId}
                group={item}
                index={i}
                animate={animate}
                onOpenDetails={handleOpenDetails}
                detailsId={detailsId}
                onCloseDetails={() => setDetailsId(null)}
              />
            )
          }

          const activity = item as ToolActivity

          if (TASK_TOOL_NAMES.has(activity.toolName)) {
            if (activity.toolUseId === firstTaskToolUseId) {
              return (
                <TaskProgressCard
                  key="task-progress-card"
                  activities={taskActivities}
                  animate={animate}
                  streamEnded={!animate}
                />
              )
            }
            return null
          }

          return (
            <React.Fragment key={activity.toolUseId}>
              <ActivityRow
                activity={activity}
                index={i}
                animate={animate}
                onOpenDetails={handleOpenDetails}
              />
              {detailsId === activity.toolUseId && detailActivity && (
                <div className="mx-2 mb-1">
                  <ActivityDetails activity={detailActivity} onClose={() => setDetailsId(null)} />
                </div>
              )}
            </React.Fragment>
          )
        })}
      </div>

      {/* Expand/collapse footer */}
      {!animate && needsCollapse && (
        <button
          type="button"
          onClick={() => setExpanded(!expanded)}
          className="w-full px-3 py-1 text-[11px] text-muted-foreground/50 hover:text-foreground/70 hover:bg-muted/20 transition-colors border-t border-border/30 text-left"
        >
          {expanded ? '收起' : `展开全部 ${visibleRows} 项`}
        </button>
      )}
    </div>
  )
}

// 保留单项导出（向后兼容 AgentMessages 中的旧引用）
export function ToolActivityItem({ activity }: { activity: ToolActivity }): React.ReactElement {
  return <ToolActivityList activities={[activity]} />
}

// 重新导出 ActivityRow（供测试与潜在外部引用）
export { ActivityRow } from './tool-activity/ActivityRow'

// 导出格式化耗时（供外部组件引用）
export { formatElapsed }
