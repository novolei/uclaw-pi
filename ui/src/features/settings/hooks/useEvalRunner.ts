// Owns the eval-suite runner — `evalReports` + `busy` + `run(kind)` + `runAll`.
// Extracted out of `SystemTab` during the P1 split. All IPC goes through
// `settingsBridge`; no `@tauri-apps/api` here.
import * as React from 'react'
import { settingsBridge } from '../../../lib/bridge/settings'
import {
  evalCommands,
  normalizeEvalReport,
  type EvalKind,
  type EvalSuiteReport,
} from '../lib/diagnostics-types'

export function useEvalRunner(onError?: (m: string) => void) {
  const [busy, setBusy] = React.useState<EvalKind | 'all' | null>(null)
  const [evalReports, setEvalReports] = React.useState<Record<EvalKind, EvalSuiteReport | null>>({
    browser: null,
    memory: null,
    agent: null,
    self: null,
  })

  const run = React.useCallback(async (kind: EvalKind) => {
    setBusy(kind)
    try {
      const result = await settingsBridge.runEval<unknown>(evalCommands[kind])
      setEvalReports(prev => ({ ...prev, [kind]: normalizeEvalReport(kind, result) }))
    } catch (e) {
      onError?.(String(e))
    } finally {
      setBusy(null)
    }
  }, [onError])

  const runAll = React.useCallback(async () => {
    setBusy('all')
    try {
      for (const kind of Object.keys(evalCommands) as EvalKind[]) {
        const result = await settingsBridge.runEval<unknown>(evalCommands[kind])
        setEvalReports(prev => ({ ...prev, [kind]: normalizeEvalReport(kind, result) }))
      }
    } catch (e) {
      onError?.(String(e))
    } finally {
      setBusy(null)
    }
  }, [onError])

  return { evalReports, busy, run, runAll }
}
