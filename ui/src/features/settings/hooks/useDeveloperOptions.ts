// Owns all DeveloperOptionsSection side effects — the expand/collapse gate, the
// per-script run state machine, the two setup-script event subscriptions, the
// elapsed-time progress timer, and `handleRun`. Extracted out of the component
// during the migration. The event subscriptions go through settingsBridge's
// `onSetupScriptOutput`/`onSetupScriptEnd` wrappers (was `@tauri-apps/api/event`
// `listen` directly in the component) so no settings component imports
// @tauri-apps/api — satisfying the hard constraint. `runSetupScript` itself stays
// in @/lib/embedding-endpoint (a dev/setup domain helper, not settings-domain).
// The run_id-before-invoke ordering + the 500ms progress cadence + the
// MAX_LOG_LINES ring buffer are preserved verbatim.
import * as React from 'react'
import {
  SETUP_SCRIPTS,
  SETUP_SCRIPT_DESCRIPTORS,
  runSetupScript,
  type SetupScriptName,
} from '@/lib/embedding-endpoint'
import { onSetupScriptOutput, onSetupScriptEnd } from '@/lib/bridge/settings'

// The unlisten handle type, derived from the bridge wrappers so this hook needs
// no @tauri-apps/api import of its own.
type UnlistenFn = Awaited<ReturnType<typeof onSetupScriptOutput>>

export interface ScriptState {
  running: boolean
  runId: string | null
  log: string[]
  exitCode: number | null
  progressPct: number
  startedAtMs: number | null
  error: string | null
}

const EMPTY_STATE: ScriptState = {
  running: false,
  runId: null,
  log: [],
  exitCode: null,
  progressPct: 0,
  startedAtMs: null,
  error: null,
}

const MAX_LOG_LINES = 500

function makeInitial(): Record<SetupScriptName, ScriptState> {
  const r = {} as Record<SetupScriptName, ScriptState>
  for (const n of SETUP_SCRIPTS) {
    r[n] = { ...EMPTY_STATE }
  }
  return r
}

export function useDeveloperOptions() {
  const [expanded, setExpanded] = React.useState(false)
  const [states, setStates] = React.useState<Record<SetupScriptName, ScriptState>>(makeInitial())
  const [forceConfirm, setForceConfirm] = React.useState<SetupScriptName | null>(null)

  React.useEffect(() => {
    if (!expanded) return
    let unlistenOutput: UnlistenFn | null = null
    let unlistenEnd: UnlistenFn | null = null
    ;(async () => {
      unlistenOutput = await onSetupScriptOutput((payload) => {
        const { run_id, line } = payload
        setStates((prev) => {
          const next = { ...prev }
          for (const n of SETUP_SCRIPTS) {
            if (prev[n].runId === run_id) {
              const log = [...prev[n].log, line]
              if (log.length > MAX_LOG_LINES) log.splice(0, log.length - MAX_LOG_LINES)
              next[n] = { ...prev[n], log }
              break
            }
          }
          return next
        })
      })
      unlistenEnd = await onSetupScriptEnd((payload) => {
        const { run_id, exit_code, success } = payload
        setStates((prev) => {
          const next = { ...prev }
          for (const n of SETUP_SCRIPTS) {
            if (prev[n].runId === run_id) {
              next[n] = {
                ...prev[n],
                running: false,
                exitCode: exit_code,
                progressPct: success ? 100 : prev[n].progressPct,
                error: success ? null : `exit ${exit_code ?? 'killed'}`,
              }
              break
            }
          }
          return next
        })
      })
    })()
    return () => {
      unlistenOutput?.()
      unlistenEnd?.()
    }
  }, [expanded])

  React.useEffect(() => {
    const anyRunning = SETUP_SCRIPTS.some((n) => states[n].running)
    if (!anyRunning) return
    const timer = setInterval(() => {
      setStates((prev) => {
        const next = { ...prev }
        let changed = false
        for (const n of SETUP_SCRIPTS) {
          if (!prev[n].running || prev[n].startedAtMs == null) continue
          const elapsedSecs = (Date.now() - prev[n].startedAtMs) / 1000
          const expected = SETUP_SCRIPT_DESCRIPTORS[n].expectedDurationSecs
          const pct = Math.min(95, Math.floor((elapsedSecs / expected) * 95))
          if (pct !== prev[n].progressPct) {
            next[n] = { ...prev[n], progressPct: pct }
            changed = true
          }
        }
        return changed ? next : prev
      })
    }, 500)
    return () => clearInterval(timer)
  }, [states])

  const handleRun = React.useCallback(async (name: SetupScriptName, force: boolean) => {
    // Generate the run_id BEFORE invoke so the event listeners can
    // route output to this card from the very first emit. Without
    // this, runSetupScript's promise only resolves at child exit
    // (because backend awaits the wait) — and during the entire run
    // the card's runId would be null, dropping every output line.
    const runId = `setup-${name}-${Date.now()}`
    setStates((prev) => ({
      ...prev,
      [name]: {
        running: true,
        runId,
        log: [],
        exitCode: null,
        progressPct: 1,
        startedAtMs: Date.now(),
        error: null,
      },
    }))
    setForceConfirm(null)
    try {
      await runSetupScript(name, { force, runId })
    } catch (e) {
      setStates((prev) => ({
        ...prev,
        [name]: {
          ...prev[name],
          running: false,
          error: String(e),
        },
      }))
    }
  }, [])

  return {
    expanded, setExpanded,
    states,
    forceConfirm, setForceConfirm,
    handleRun,
  }
}
