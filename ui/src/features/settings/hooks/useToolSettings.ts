// Owns the ToolSettings side effect — the active-skill manifest load on mount +
// the manual refresh. Extracted out of the component during the migration; all
// IPC goes through `settingsBridge` (no `@tauri-apps/api` / catch-all bridge
// here). Error handling (toast on failure) is preserved verbatim.
import * as React from 'react'
import { toast } from 'sonner'
import type { ActiveManifestSkill } from '@/lib/types'
import { settingsBridge } from '../../../lib/bridge/settings'

export function useToolSettings() {
  const [activeManifest, setActiveManifest] = React.useState<ActiveManifestSkill[] | null>(null)
  const [manifestLoading, setManifestLoading] = React.useState(false)

  const refreshActiveManifest = React.useCallback(async () => {
    setManifestLoading(true)
    try {
      const rows = await settingsBridge.listActiveManifestSkills()
      setActiveManifest(rows)
    } catch (e) {
      toast.error('加载活动技能清单失败', { description: String(e) })
    } finally {
      setManifestLoading(false)
    }
  }, [])

  React.useEffect(() => {
    refreshActiveManifest()
  }, [refreshActiveManifest])

  return { activeManifest, manifestLoading, refreshActiveManifest }
}
