// Owns the optional skillsmp.com API key setting — load its status (is-a-key-stored)
// and save/clear it via the settings bridge. The key itself never leaves the
// backend; the hook only knows whether one is set. Mirrors `useSkillsApiKey`
// (skillsmp is keyless-by-default; a key only raises the anon rate limit).
// All IPC goes through `settingsBridge`; no `@tauri-apps/api` here.
import * as React from 'react'
import { settingsBridge } from '../../../lib/bridge/settings'

export function useSkillsmpApiKey(onError?: (m: string) => void) {
  // null = still loading the persisted status.
  const [isSet, setIsSet] = React.useState<boolean | null>(null)
  const [saving, setSaving] = React.useState(false)
  const reload = React.useCallback(() => {
    settingsBridge.getSkillsmpApiKeySet().then(setIsSet).catch(() => setIsSet(false))
  }, [])
  React.useEffect(reload, [reload])
  // Store the key (or clear it with ''). Returns whether it succeeded so the
  // card can clear its draft input only on success.
  const save = React.useCallback(
    async (key: string): Promise<boolean> => {
      setSaving(true)
      try {
        await settingsBridge.setSkillsmpApiKey(key)
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
