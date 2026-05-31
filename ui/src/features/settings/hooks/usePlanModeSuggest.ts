// Owns the "suggest Plan mode for complex tasks" preference — the jotai atom
// pairing + the persist IPC. Extracted out of `AgentSettings` during the P2
// move. All IPC goes through `settingsBridge`; no `@tauri-apps/api` here.
import * as React from 'react'
import { useAtom } from 'jotai'
import { planModeSuggestEnabledAtom } from '@/atoms/ui-preferences'
import { settingsBridge } from '../../../lib/bridge/settings'

export function usePlanModeSuggest() {
  const [enabled, setEnabled] = useAtom(planModeSuggestEnabledAtom)
  const setPlanSuggestEnabled = React.useCallback(
    async (v: boolean) => {
      setEnabled(v)
      try {
        await settingsBridge.setPlanModeSuggestEnabled(v)
      } catch (e) {
        console.error('[AgentSettings] set_plan_mode_suggest_enabled failed', e)
      }
    },
    [setEnabled],
  )
  return { enabled, setPlanSuggestEnabled }
}
