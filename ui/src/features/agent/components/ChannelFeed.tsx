import * as React from 'react'
import type { TeamChannelMessage } from '@/lib/bridge/agent'

function roleLabel(role: string): string {
  if (role.toLowerCase().includes('worker')) return `👷 Worker`
  if (role.toLowerCase().includes('supervisor')) return '🧠 Supervisor'
  if (role.toLowerCase().includes('reviewer')) return '🔍 Reviewer'
  return role
}

export function ChannelFeed({ messages }: { messages: TeamChannelMessage[] }): React.ReactElement {
  const bottomRef = React.useRef<HTMLDivElement>(null)
  const isFirstRender = React.useRef(true)

  React.useEffect(() => {
    if (isFirstRender.current) {
      isFirstRender.current = false
      bottomRef.current?.scrollIntoView({ behavior: 'instant' as ScrollBehavior })
      return
    }
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages.length])

  return (
    <div className="flex flex-col gap-1.5 overflow-y-auto max-h-[200px] pr-1">
      {messages.length === 0 && (
        <p className="text-[11px] text-muted-foreground">No messages yet.</p>
      )}
      {messages.map((msg) => (
        <div key={msg.id} className="text-[11px] flex gap-1.5">
          <span className="text-muted-foreground shrink-0 whitespace-nowrap">
            {roleLabel(msg.fromRole)}
          </span>
          <span className="text-foreground">{msg.message}</span>
        </div>
      ))}
      <div ref={bottomRef} />
    </div>
  )
}
