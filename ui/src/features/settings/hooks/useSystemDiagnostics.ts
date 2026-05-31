// Owns the system-diagnostics report fetch — `report` + `loading` +
// `lastChecked` + `runDiagnostics`. Extracted out of `SystemTab` during the
// P1 split. All IPC goes through `settingsBridge`; no `@tauri-apps/api` here.
import * as React from 'react'
import { settingsBridge } from '../../../lib/bridge/settings'
import type { SystemDiagnosticsReport } from '../lib/diagnostics-types'

export function useSystemDiagnostics(onError?: (m: string) => void) {
  const [report, setReport] = React.useState<SystemDiagnosticsReport | null>(null)
  const [loading, setLoading] = React.useState(false)
  const [lastChecked, setLastChecked] = React.useState<Date | null>(null)

  const runDiagnostics = React.useCallback(async () => {
    setLoading(true)
    try {
      const r = await settingsBridge.getSystemDiagnostics<SystemDiagnosticsReport>()
      setReport(r)
      setLastChecked(new Date())
    } catch (e) {
      onError?.(String(e))
    } finally {
      setLoading(false)
    }
  }, [onError])

  return { report, loading, lastChecked, runDiagnostics }
}
