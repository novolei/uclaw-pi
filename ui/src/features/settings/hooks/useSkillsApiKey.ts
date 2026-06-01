// Owns the skills.sh API key setting — load its status (is-a-key-stored) and
// save/clear it via the settings bridge. The key itself never leaves the
// backend; the hook only knows whether one is set. Mirrors `usePiEngineToggle`.
// All IPC goes through `settingsBridge`; no `@tauri-apps/api` here.
import * as React from 'react'
import { settingsBridge } from '../../../lib/bridge/settings'

export function useSkillsApiKey(onError?: (m: string) => void) {
  // null = still loading the persisted status.
  const [isSet, setIsSet] = React.useState<boolean | null>(null)
  const [saving, setSaving] = React.useState(false)
  const reload = React.useCallback(() => {
    settingsBridge.getSkillsShApiKeySet().then(setIsSet).catch(() => setIsSet(false))
  }, [])
  React.useEffect(reload, [reload])
  // Store the key (or clear it with ''). Returns whether it succeeded so the
  // card can clear its draft input only on success.
  const save = React.useCallback(
    async (key: string): Promise<boolean> => {
      setSaving(true)
      try {
        await settingsBridge.setSkillsShApiKey(key)
        reload()
        return true
      } catch (e) {
        onError?.(String(e))
        return false
      } finally {
        setSaving(false)
      }
    },
    [onError, reload],
  )
  return { isSet, saving, save }
}
