// Owns the optional local HTTP API server toggle — load + persist via the
// settings bridge. Extracted out of `SystemTab` during the P1 split.
// All IPC goes through `settingsBridge`; no `@tauri-apps/api` here.
import * as React from 'react'
import { settingsBridge } from '../../../lib/bridge/settings'

export function useHttpApiToggle(onError?: (m: string) => void) {
  // null = still loading the persisted backend setting.
  const [enabled, setEnabled] = React.useState<boolean | null>(null)
  React.useEffect(() => {
    settingsBridge.getHttpApiEnabled().then(setEnabled).catch(() => setEnabled(false))
  }, [])
  const toggle = React.useCallback(async () => {
    if (enabled === null) return
    const next = !enabled
    try {
      await settingsBridge.setHttpApiEnabled(next)
      setEnabled(next)
    } catch (e) {
      onError?.(String(e))
    }
  }, [enabled, onError])
  return { enabled, toggle }
}
