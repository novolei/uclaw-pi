/**
 * useLivePlanContent — keeps a plan file's markdown live as the agent rewrites
 * it. Extracted from PlanViewer during the features/agent migration so the
 * component stays presentational.
 *
 * Behavior (unchanged from the inline version):
 *   - Seeds local state from `planContent`.
 *   - Resets to the latest `planContent` when the user switches to a different
 *     plan file (keyed on `planFilename`), reading the value from a ref so the
 *     reset effect doesn't re-run on every `planContent` change for the same file.
 *   - Subscribes to `plan:updated` (through the agent bridge — no
 *     `@tauri-apps/api` here) and applies the new content when the event's
 *     `filename` matches the open plan.
 */

import * as React from 'react'
import { onPlanUpdated } from '@/lib/bridge/agent'

export function useLivePlanContent(planContent: string, planFilename: string): string {
  const [liveContent, setLiveContent] = React.useState(planContent)

  // Track latest planContent in a ref so the filename-change effect can read it
  // without being re-triggered whenever planContent changes for the same file.
  const planContentRef = React.useRef(planContent)
  planContentRef.current = planContent

  // Reset local content when the user switches to a different plan file.
  React.useEffect(() => {
    setLiveContent(planContentRef.current)
  }, [planFilename])

  // Subscribe to live plan:updated events.
  React.useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | null = null

    onPlanUpdated((payload) => {
      if (payload.filename === planFilename) {
        setLiveContent(payload.content)
      }
    }).then((fn) => {
      if (cancelled) fn()
      else unlisten = fn
    })

    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [planFilename])

  return liveContent
}
