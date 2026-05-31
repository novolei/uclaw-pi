/**
 * QueuedMessagesBanner — Codex / Claude-app style queue card stacked
 * ABOVE the composer (sibling, not child) showing messages the user
 * typed while the agent was already streaming.
 *
 * Each queued message has three actions:
 *
 *   - 引导 (steer)  — send now + interrupt current turn
 *   - 编辑 (edit)   — pop back into the composer for editing
 *   - 删除 (trash)  — discard
 *
 * If the user takes no action, the agent's current turn completes
 * naturally and AgentView auto-dispatches the oldest queued message
 * (FIFO) without an interrupt — see the streaming-transition effect
 * in AgentView.tsx.
 *
 * Visual design follows the Claude app's "queued message" pill style:
 * - rounded card with subtle border + bg, slightly elevated
 * - 1-line truncate with `title` carrying the full text
 * - 引导 reads as a primary action (text + icon); 编辑/删除 sit in a
 *   compact secondary group
 * - fade-in on mount + slight slide-up for delight
 * - fully theme-aware (uses semantic tokens — works in light + dark)
 */

import * as React from 'react'
import {
  CornerDownLeft,
  Trash2,
  Pencil,
  MoreHorizontal,
  Clock3,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { QueuedAgentMessage } from '@/atoms/agent-queue-messages'

export interface QueuedMessagesBannerProps {
  messages: QueuedAgentMessage[]
  /** Steer = send-now-with-interrupt. Fires the message into the running agent loop. */
  onSteer: (msg: QueuedAgentMessage) => void
  /** Edit = pop from queue, restore to composer for further editing. */
  onEdit: (msg: QueuedAgentMessage) => void
  /** Delete = discard from queue. */
  onDelete: (msg: QueuedAgentMessage) => void
}

export function QueuedMessagesBanner({
  messages,
  onSteer,
  onEdit,
  onDelete,
}: QueuedMessagesBannerProps): React.ReactElement | null {
  if (messages.length === 0) return null

  return (
    <div
      role="region"
      aria-label="Queued messages waiting to send"
      className={cn(
        'mb-2 flex flex-col gap-1.5',
        'animate-in fade-in slide-in-from-bottom-1 duration-200',
      )}
    >
      {messages.length > 1 && (
        <div className="px-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground/65">
          {messages.length} 条排队消息 · 将在 Agent 完成后按顺序发送
        </div>
      )}
      {messages.map((msg) => (
        <QueuedMessageRow
          key={msg.id}
          msg={msg}
          onSteer={() => onSteer(msg)}
          onEdit={() => onEdit(msg)}
          onDelete={() => onDelete(msg)}
        />
      ))}
    </div>
  )
}

interface QueuedMessageRowProps {
  msg: QueuedAgentMessage
  onSteer: () => void
  onEdit: () => void
  onDelete: () => void
}

function QueuedMessageRow({
  msg,
  onSteer,
  onEdit,
  onDelete,
}: QueuedMessageRowProps): React.ReactElement {
  const [menuOpen, setMenuOpen] = React.useState(false)
  const menuRef = React.useRef<HTMLDivElement>(null)

  // Close the more-menu on outside click. Bubble-based so we don't fight
  // the button's onClick fire order — only closes when click target is
  // outside the menu container.
  React.useEffect(() => {
    if (!menuOpen) return
    const handler = (e: MouseEvent) => {
      if (!menuRef.current?.contains(e.target as Node)) {
        setMenuOpen(false)
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [menuOpen])

  return (
    <div
      className={cn(
        'group/queued flex items-center gap-2.5 rounded-[14px]',
        'border-[0.5px] border-border/70 bg-background/85 backdrop-blur-sm',
        'pl-3 pr-1.5 py-1.5 text-sm shadow-sm',
        'transition-all duration-150',
        'hover:border-border hover:bg-background hover:shadow',
      )}
    >
      {/* Leading indent indicator — visually anchors to "queued reply" */}
      <CornerDownLeft
        className="size-4 shrink-0 -scale-y-100 text-muted-foreground/55"
        aria-hidden
      />

      {/* Message text — single line truncate, hover reveals full via title */}
      <span
        className="flex-1 min-w-0 truncate text-foreground/85"
        title={msg.text}
      >
        {msg.text}
      </span>

      {/* Primary action: 引导 (steer) — fires immediately + interrupts */}
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            onClick={onSteer}
            className={cn(
              'inline-flex shrink-0 items-center gap-1 rounded-full',
              'border border-border/70 px-2.5 py-1',
              'text-xs font-medium text-foreground/80',
              'bg-background hover:bg-accent hover:text-foreground',
              'transition-colors',
            )}
          >
            <CornerDownLeft className="size-3" />
            引导
          </button>
        </TooltipTrigger>
        <TooltipContent side="top">
          <p>立刻把这条消息引导给运行中的 Agent（会打断当前 turn）</p>
        </TooltipContent>
      </Tooltip>

      {/* Trash — quick destructive action */}
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            onClick={onDelete}
            className={cn(
              'inline-flex shrink-0 items-center justify-center rounded-md p-1',
              'text-muted-foreground/65 hover:text-destructive hover:bg-destructive/10',
              'transition-colors',
            )}
            aria-label="删除排队消息"
          >
            <Trash2 className="size-3.5" />
          </button>
        </TooltipTrigger>
        <TooltipContent side="top">
          <p>丢弃这条排队消息</p>
        </TooltipContent>
      </Tooltip>

      {/* More-menu — houses 编辑 for now; reserved for future actions */}
      <div ref={menuRef} className="relative shrink-0">
        <button
          type="button"
          onClick={() => setMenuOpen((v) => !v)}
          className={cn(
            'inline-flex items-center justify-center rounded-md p-1',
            'text-muted-foreground/65 hover:text-foreground hover:bg-accent',
            'transition-colors',
          )}
          aria-label="更多选项"
          aria-haspopup="menu"
          aria-expanded={menuOpen}
        >
          <MoreHorizontal className="size-3.5" />
        </button>
        {menuOpen && (
          <div
            role="menu"
            className={cn(
              'absolute right-0 top-full mt-1.5 z-20',
              'min-w-[180px] overflow-hidden',
              'rounded-lg border border-border/70 bg-popover shadow-lg',
              'py-1',
              'animate-in fade-in zoom-in-95 duration-100',
            )}
          >
            <MenuItem
              icon={<Pencil className="size-3.5" />}
              label="编辑后重发"
              hint="放回输入框，可继续修改"
              onClick={() => {
                setMenuOpen(false)
                onEdit()
              }}
            />
            <MenuItem
              icon={<Clock3 className="size-3.5" />}
              label={`排队 ${formatRelativeAge(msg.queuedAt)}`}
              hint="将在当前 turn 结束后自动发送"
              disabled
            />
          </div>
        )}
      </div>
    </div>
  )
}

function MenuItem({
  icon,
  label,
  hint,
  onClick,
  disabled,
}: {
  icon: React.ReactNode
  label: string
  hint?: string
  onClick?: () => void
  disabled?: boolean
}): React.ReactElement {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        'flex w-full flex-col items-start gap-0.5 px-2.5 py-1.5',
        'text-left transition-colors',
        disabled
          ? 'cursor-default text-muted-foreground/60'
          : 'text-foreground/85 hover:bg-accent hover:text-foreground',
      )}
    >
      <span className="flex items-center gap-2 text-xs">
        {icon}
        {label}
      </span>
      {hint && (
        <span className="pl-5 text-[10px] text-muted-foreground/55">
          {hint}
        </span>
      )}
    </button>
  )
}

/** "5秒前" / "1分钟前" style relative age for menu hint. */
function formatRelativeAge(ts: number): string {
  const diffSec = Math.max(0, Math.floor((Date.now() - ts) / 1000))
  if (diffSec < 5) return '刚刚'
  if (diffSec < 60) return `${diffSec}秒`
  const mins = Math.floor(diffSec / 60)
  if (mins < 60) return `${mins}分钟`
  const hrs = Math.floor(mins / 60)
  return `${hrs}小时`
}
