// Owns the memu/gbrain/recovery bridge actions — per-action busy flags + a
// `run(command, key)` that invokes the action. Extracted out of `SystemTab`
// during the P1 split. All IPC goes through `settingsBridge`; no
// `@tauri-apps/api` here.
import * as React from 'react'
import { settingsBridge } from '../../../lib/bridge/settings'

export type BridgeActionKey = 'memu' | 'gbrain' | 'reset' | 'restart'

export function useBridgeAction(onError?: (m: string) => void) {
  const [busy, setBusy] = React.useState<Record<BridgeActionKey, boolean>>({
    memu: false,
    gbrain: false,
    reset: false,
    restart: false,
  })

  const run = React.useCallback(async (command: string, key: BridgeActionKey) => {
    setBusy(prev => ({ ...prev, [key]: true }))
    try {
      await settingsBridge.bridgeAction(command)
    } catch (e) {
      onError?.(String(e))
    } finally {
      setBusy(prev => ({ ...prev, [key]: false }))
    }
  }, [onError])

  return { busy, run }
}
