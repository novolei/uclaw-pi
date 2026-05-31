import * as React from 'react'
import { useAtomValue } from 'jotai'
import { currentAgentSessionIdAtom } from '@/atoms/agent-atoms'
import { BrowserPanel } from '@/components/browser/BrowserPanel'

export function BrowserViewer(): React.ReactElement {
  const sessionId = useAtomValue(currentAgentSessionIdAtom)
  if (!sessionId) {
    return (
      <div className="flex h-full items-center justify-center p-4 text-center text-[12px] text-muted-foreground">
        当前没有选中的 Agent 会话
      </div>
    )
  }
  return <BrowserPanel agentSessionId={sessionId} />
}
