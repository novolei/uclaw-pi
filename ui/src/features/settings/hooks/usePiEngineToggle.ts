// Owns the experimental PiEngine backend toggle — load + persist via the
// settings bridge. Mirrors `useHttpApiToggle`. All IPC goes through
// `settingsBridge`; no `@tauri-apps/api` here.
import * as React from 'react'
import { settingsBridge } from '../../../lib/bridge/settings'

export function usePiEngineToggle(onError?: (m: string) => void) {
  // null = still loading the persisted backend setting.
  const [enabled, setEnabled] = React.useState<boolean | null>(null)
  React.useEffect(() => {
    settingsBridge.getPiEngineEnabled().then(setEnabled).catch(() => setEnabled(false))
  }, [])
  const toggle = React.useCallback(async () => {
    if (enabled === null) return
    const next = !enabled
    try {
      await settingsBridge.setPiEngineEnabled(next)
      setEnabled(next)
    } catch (e) {
      onError?.(String(e))
    }
  }, [enabled, onError])
  return { enabled, toggle }
}
