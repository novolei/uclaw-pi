// Owns the GeneralSettings IPC side effects — the interface-language load on
// mount + persist on change. Extracted out of the component during the P3 move;
// all IPC goes through `settingsBridge` (no `@tauri-apps/api` here). The other
// GeneralSettings controls (send-on-enter / show-timestamp local toggles, the
// bottom-dock atom) carry no IPC and stay in the component.
import * as React from 'react'
import { settingsBridge } from '../../../lib/bridge/settings'

export function useGeneralSettings() {
  const [language, setLanguage] = React.useState('zh-CN')

  React.useEffect(() => {
    settingsBridge.getSettings().then((s) => {
      if (s.language) setLanguage(s.language)
    })
  }, [])

  const handleLanguageChange = React.useCallback(async (value: string) => {
    setLanguage(value)
    await settingsBridge.patchSettings({ language: value })
  }, [])

  return { language, handleLanguageChange }
}
