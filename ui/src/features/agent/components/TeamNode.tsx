import * as React from 'react'
import { cn } from '@/lib/utils'
import type { TeamNode as TeamNodeType, NodeStatus } from '@/atoms/agent-teams'

const ROLE_ICONS: Record<TeamNodeType['role'], string> = { supervisor: '🧠', worker: '👷', reviewer: '🔍' }
const STATUS_COLORS: Record<NodeStatus, string> = {
  idle: 'text-muted-foreground',
  running: 'text-blue-500',
  done: 'text-green-500',
  failed: 'text-red-500',
}
const STATUS_LABELS: Record<NodeStatus, string> = {
  idle: 'Waiting',
  running: 'Running',
  done: 'Done',
  failed: 'Failed',
}

export function TeamNode({ node }: { node: TeamNodeType }): React.ReactElement {
  return (
    <div className="flex items-start gap-2 p-2 rounded-md bg-muted/30">
      <span className="text-lg leading-none mt-0.5">{ROLE_ICONS[node.role] ?? '🤖'}</span>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5">
          <span className="text-[12px] font-medium truncate">{node.label}</span>
          <span className={cn('text-[10px]', STATUS_COLORS[node.status] ?? 'text-muted-foreground')}>
            {STATUS_LABELS[node.status] ?? node.status}
          </span>
          {node.status === 'running' && (
            <span className="inline-flex gap-0.5">
              {[0, 1, 2].map((i) => (
                <span
                  key={i}
                  className="h-1 w-1 rounded-full bg-blue-500 animate-bounce"
                  style={{ animationDelay: `${i * 0.15}s` }}
                />
              ))}
            </span>
          )}
        </div>
        {node.lastMessage && (
          <p className="text-[11px] text-muted-foreground truncate mt-0.5">{node.lastMessage}</p>
        )}
      </div>
    </div>
  )
}
