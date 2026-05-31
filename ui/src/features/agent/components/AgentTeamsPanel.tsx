import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { activeTeamAtom, appendTeamMessageAtom } from '@/atoms/agent-teams'
import { onTeamMessage } from '@/lib/bridge/agent'
import { TeamNode } from './TeamNode'
import { ChannelFeed } from './ChannelFeed'

export function AgentTeamsPanel(): React.ReactElement | null {
  const team = useAtomValue(activeTeamAtom)
  const appendMessage = useSetAtom(appendTeamMessageAtom)

  React.useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | null = null

    onTeamMessage((payload) => {
      appendMessage(payload)
    }).then((fn) => {
      if (cancelled) fn()
      else unlisten = fn
    })

    return () => {
      cancelled = true
      unlisten?.()
    }
    // appendMessage is a stable Jotai write-atom setter — safe to omit
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  if (!team) {
    return (
      <div className="p-3 text-[12px] text-muted-foreground">
        No active Agent Teams session. Start a team run to see progress here.
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-3 p-3">
      <div>
        <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground mb-1">Task</p>
        <p className="text-[12px] text-foreground line-clamp-2">{team.task}</p>
      </div>

      <div>
        <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground mb-1.5">Agents</p>
        <div className="flex flex-col gap-1.5">
          {team.nodes.map((node) => (
            <TeamNode key={node.id} node={node} />
          ))}
          {team.nodes.length === 0 && (
            <p className="text-[11px] text-muted-foreground">Waiting for agents to start...</p>
          )}
        </div>
      </div>

      <div>
        <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground mb-1.5">
          Channel ({team.messages.length})
        </p>
        <ChannelFeed messages={team.messages} />
      </div>
    </div>
  )
}
