/**
 * PlanModeSuggestBanner — advisory banner for plan-mode auto-suggest
 *
 * Listens for the `agent:plan_mode_suggest` IPC event and shows a light banner
 * above the input when the backend (keyword match or LLM tool) suggests the user
 * would benefit from switching to Plan mode.
 *
 * Three actions:
 *   切到 Plan 模式 → setSafetyMode({ mode: 'plan' }) + outcome=accepted
 *   本次不用       → outcome=skipped (no mode change)
 *   不再建议       → planModeSuggestEnabledAtom=false + outcome=silenced
 *
 * Payload fields are snake_case, matching what serde emits from Rust without
 * rename_all: "camelCase" on the JSON payload object.
 *
 * Per-session dedupe: once the user acts on the banner (any action), the session
 * is added to silencedPlanModeSessionsAtom. Future backend fires for the same
 * session are dropped at the listener level. The silenced set is cleared when the
 * user manually changes safety mode via PermissionModeSelector — user intent has
 * clearly changed, so re-evaluation is warranted on the next matching message.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { Button } from '@/components/ui/button'
import {
  pendingPlanModeSuggestsAtom,
  silencedPlanModeSessionsAtom,
  type PlanModeSuggestRequest,
} from '@/atoms/plan-mode-suggest-atoms'
import { planModeSuggestEnabledAtom } from '@/atoms/ui-preferences'
import { onPlanModeSuggest, respondPlanModeSuggest, setSafetyMode } from '@/lib/bridge/agent'

interface Props { sessionId: string }

export function PlanModeSuggestBanner({ sessionId }: Props): React.ReactElement | null {
  const [queue, setQueue] = useAtom(pendingPlanModeSuggestsAtom)
  const [enabled, setEnabled] = useAtom(planModeSuggestEnabledAtom)
  const [silenced, setSilenced] = useAtom(silencedPlanModeSessionsAtom)
  const req = queue[sessionId] ?? null

  // Use a ref so the listener callback always reads the latest silenced set
  // without needing to be in the useEffect dependency array (which would cause
  // the listener to re-register on every set-change).
  const silencedRef = React.useRef(silenced)
  silencedRef.current = silenced

  React.useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | null = null
    onPlanModeSuggest<PlanModeSuggestRequest>((payload) => {
      // Backend emits snake_case keys via serde_json::json! macro.
      if (payload.session_id !== sessionId) return
      if (silencedRef.current.has(sessionId)) return  // skip silenced sessions
      setQueue((q) => ({ ...q, [sessionId]: payload }))
    }).then((fn) => { if (cancelled) fn(); else unlisten = fn })
    return () => { cancelled = true; unlisten?.() }
  }, [sessionId, setQueue])

  if (!enabled || !req) return null

  const clear = () => setQueue((q) => ({ ...q, [sessionId]: null }))

  const addToSilenced = () => setSilenced((s: Set<string>) => {
    const next = new Set(s)
    next.add(sessionId)
    return next
  })

  const handleSwitch = async (): Promise<void> => {
    // NOTE: handleSwitch adds to silenced but does NOT reset the full silenced
    // set — that would re-arm the banner. The mode switch already prevents
    // future fires (keyword detector gates on Yolo/Supervised modes).
    addToSilenced()
    try {
      await setSafetyMode({ mode: 'plan' })
      await respondPlanModeSuggest(req.id, 'accepted')
    } catch (e) {
      console.error('[PlanModeSuggestBanner] 切换到 Plan 模式失败:', e)
    } finally {
      clear()
    }
  }

  const handleSkip = async (): Promise<void> => {
    addToSilenced()
    try {
      await respondPlanModeSuggest(req.id, 'skipped')
    } finally {
      clear()
    }
  }

  const handleNever = async (): Promise<void> => {
    addToSilenced()
    setEnabled(false)
    try {
      await respondPlanModeSuggest(req.id, 'silenced')
    } finally {
      clear()
    }
  }

  return (
    <div
      role="status"
      aria-live="polite"
      className="mx-4 mb-3 rounded-lg border border-border bg-popover px-4 py-3 text-sm shadow-sm animate-in slide-in-from-bottom-2 duration-200"
    >
      <div className="flex items-start gap-2">
        <span className="text-base leading-none">💡</span>
        <div className="flex-1 min-w-0">
          <p className="text-foreground">
            {req.reason ?? '这个任务看起来是多步骤构建。先切到 Plan 模式让 agent 把方案敲定再执行？'}
          </p>
          {req.preview_steps && req.preview_steps.length > 0 && (
            <ul className="mt-2 list-disc pl-5 text-xs text-muted-foreground space-y-0.5">
              {req.preview_steps.map((s, i) => <li key={i}>{s}</li>)}
            </ul>
          )}
        </div>
      </div>
      <div className="mt-3 flex items-center justify-end gap-2">
        <Button variant="ghost" size="sm" onClick={handleNever} aria-label="不再建议">
          不再建议
        </Button>
        <Button variant="outline" size="sm" onClick={handleSkip}>本次不用</Button>
        <Button variant="default" size="sm" onClick={handleSwitch}>切到 Plan 模式</Button>
      </div>
    </div>
  )
}
