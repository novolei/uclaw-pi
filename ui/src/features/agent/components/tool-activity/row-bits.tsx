/**
 * Small presentational bits shared across the tool-activity rows: the status
 * icon, the error badge, and the Edit/Write diff-marker coloring. Also hosts the
 * TodoWrite list + intermediate-thinking row helpers (kept here unchanged from
 * the original ToolActivityItem.tsx during the features/agent migration split).
 */

import * as React from 'react'
import {
  CheckCircle2,
  XCircle,
  Loader2,
  Circle,
  MessageCircleDashed,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { getToolIcon } from '@/shared/tool-rendering'
import { type ActivityStatus } from '@/atoms/agent-atoms'
import { SIZE } from './constants'

// ===== 状态图标 =====

export function StatusIcon({ status, toolName }: { status: ActivityStatus; toolName?: string }): React.ReactElement {
  const key = `${status}-${toolName}`

  if (status === 'running' || status === 'backgrounded') {
    return (
      <span key={key} className="relative flex size-3 items-center justify-center animate-in fade-in duration-200 shrink-0">
        <span className="absolute inset-0 rounded-full bg-blue-400/20 animate-ping" style={{ animationDuration: '1.5s' }} />
        <Loader2 className={cn(SIZE.spinner, status === 'backgrounded' ? 'text-primary' : 'text-blue-500 animate-spin')} />
      </span>
    )
  }

  if (status === 'error') {
    return (
      <span key={key} className={cn(SIZE.icon, 'flex items-center justify-center shrink-0 animate-in fade-in zoom-in-75 duration-200')}>
        <XCircle className={cn(SIZE.icon, 'text-destructive/80')} />
      </span>
    )
  }

  if (status === 'completed') {
    const ToolIcon = toolName ? getToolIcon(toolName) : null
    if (ToolIcon && (toolName === 'Edit' || toolName === 'Write')) {
      return (
        <span key={key} className={cn(SIZE.icon, 'flex items-center justify-center shrink-0 animate-in fade-in zoom-in-75 duration-200')}>
          <ToolIcon className={cn(SIZE.icon, 'text-primary/70')} />
        </span>
      )
    }
    return (
      <span key={key} className={cn(SIZE.icon, 'flex items-center justify-center shrink-0 animate-in fade-in zoom-in-75 duration-200')}>
        <CheckCircle2 className={cn(SIZE.icon, 'text-emerald-500/80')} />
      </span>
    )
  }

  return (
    <span key={key} className={cn(SIZE.icon, 'flex items-center justify-center shrink-0')}>
      <Circle className={cn(SIZE.icon, 'text-muted-foreground/30')} />
    </span>
  )
}

// ===== Diff 标记着色 =====

/** 将 Edit/Write label 中末尾的 +N / -N 标记渲染为绿/红色 */
export function renderLabelWithDiff(label: string, toolName: string): React.ReactNode {
  if (toolName !== 'Edit' && toolName !== 'Write') return label
  const match = label.match(/^(.+?)(\s+[+-]\d+(?:\s+[+-]\d+)?)$/)
  if (!match) return label
  const [, text, diffPart] = match
  const tokens = diffPart!.trim().split(/\s+/)
  return (
    <>
      {text}{' '}
      {tokens.map((tok, i) => (
        <span key={i} className={tok.startsWith('+') ? 'text-green-500' : 'text-red-500'}>
          {tok}{i < tokens.length - 1 ? ' ' : ''}
        </span>
      ))}
    </>
  )
}

// ===== 错误 Badge =====

export function ErrorBadge(): React.ReactElement {
  return (
    <span className="shrink-0 px-1.5 py-0.5 rounded text-[10px] bg-destructive/5 text-destructive font-medium leading-none shadow-sm">
      Error
    </span>
  )
}

// ===== TodoWrite 可视化 =====

interface TodoItem {
  content: string
  status: 'pending' | 'in_progress' | 'completed'
  activeForm?: string
}

export function parseTodoItems(input: Record<string, unknown>): TodoItem[] | null {
  if (input.todos && Array.isArray(input.todos)) {
    return (input.todos as Array<Record<string, unknown>>).map((t) => ({
      content: String(t.subject ?? t.content ?? ''),
      status: (t.status as TodoItem['status']) ?? 'pending',
      activeForm: typeof t.activeForm === 'string' ? t.activeForm : undefined,
    }))
  }
  return null
}

export function TodoList({ items }: { items: TodoItem[] }): React.ReactElement {
  return (
    <div className="pl-5 space-y-0.5 border-l-2 border-muted ml-[5px]">
      {items.map((todo, i) => (
        <div
          key={i}
          className={cn(
            'flex items-center gap-2 text-[13px]',
            SIZE.row,
            todo.status === 'completed' && 'opacity-50',
          )}
        >
          {todo.status === 'pending' && <Circle className={cn(SIZE.icon, 'text-muted-foreground/50')} />}
          {todo.status === 'in_progress' && <Loader2 className={cn(SIZE.spinner, 'animate-spin text-blue-500')} />}
          {todo.status === 'completed' && <CheckCircle2 className={cn(SIZE.icon, 'text-green-500')} />}
          <span className={cn('truncate flex-1', todo.status === 'completed' && 'line-through')}>
            {todo.status === 'in_progress' && todo.activeForm ? todo.activeForm : todo.content}
          </span>
        </div>
      ))}
    </div>
  )
}

// ===== 中间思考行 =====

export function IntermediateRow({ text, index, animate }: { text: string; index: number; animate: boolean }): React.ReactElement {
  const delay = animate && index < SIZE.staggerLimit ? `${index * 30}ms` : '0ms'
  return (
    <div
      className={cn(
        'flex items-center gap-2 text-[13px] text-foreground/50',
        SIZE.row,
        animate && 'animate-in fade-in slide-in-from-left-2 duration-200 fill-mode-both',
      )}
      style={animate ? { animationDelay: delay } : undefined}
    >
      <MessageCircleDashed className={cn(SIZE.icon, 'text-muted-foreground/50')} />
      <span className="truncate flex-1">{text}</span>
    </div>
  )
}
