import * as React from 'react'
import { Bot } from 'lucide-react'

interface RunMeta {
  origin?: string
  spec_id?: string
  prev_run_session_id?: string | null
}

/**
 * Shown at the top of the Agent view when the loaded session is an
 * automation run (origin starts with "automation:"). Identifies the run
 * as automation-produced and surfaces the trigger. Renders nothing for
 * ordinary human sessions.
 */
export function AutomationRunBanner({
  metadataJson,
}: {
  metadataJson: string | null | undefined
}): React.ReactElement | null {
  const meta = React.useMemo<RunMeta>(() => {
    if (!metadataJson) return {}
    try {
      return JSON.parse(metadataJson) as RunMeta
    } catch {
      return {}
    }
  }, [metadataJson])

  const origin = meta.origin ?? ''
  if (!origin.startsWith('automation:')) return null

  const trigger = origin.slice('automation:'.length)

  return (
    <div className="mx-4 mb-2 flex items-center gap-2 px-3 py-2 rounded-lg bg-primary/5 text-primary text-sm">
      <Bot className="size-4" />
      <span className="font-medium">Automation run</span>
      <span className="text-xs text-muted-foreground">
        触发源: {trigger || 'unknown'}
      </span>
    </div>
  )
}
