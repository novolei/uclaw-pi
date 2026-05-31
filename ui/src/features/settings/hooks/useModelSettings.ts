// Owns the ModelSettings IPC side effects — the configured-models + role
// assignments load on mount, and the optimistic role→model write (with revert on
// failure). Extracted out of the component during the migration; all IPC goes
// through `settingsBridge` (no `@tauri-apps/api` / catch-all bridge here). The
// optimistic-update + toast-on-failure + reload-to-revert behavior is preserved
// verbatim. The dropdown open/close + outside-click UI state stays in the
// component (pure DOM, ref-coupled, no IPC).
import * as React from 'react'
import { toast } from 'sonner'
import type { ModelRoleConfig } from '@/lib/bridge/settings'
import { settingsBridge } from '../../../lib/bridge/settings'

interface ModelGroup {
  providerId: string
  models: string[]
}

const ALL_ROLES = ['chat', 'utility', 'utility_large', 'summarizer', 'compiler']

export function useModelSettings() {
  const [groups, setGroups] = React.useState<ModelGroup[]>([])
  const [roleConfigs, setRoleConfigs] = React.useState<ModelRoleConfig[]>([])

  const loadData = React.useCallback(async () => {
    const [allModels, roles] = await Promise.all([
      settingsBridge.getAllConfiguredModels(),
      settingsBridge.getRoleModels(),
    ])

    // Build groups from [providerId, modelIds[]][]
    const g: ModelGroup[] = allModels
      .filter(([, mids]) => mids.length > 0)
      .map(([pid, mids]) => ({ providerId: pid, models: mids }))
    setGroups(g)

    // Merge roles with defaults
    const merged = ALL_ROLES.map((role) => {
      const existing = roles.find((r) => r.role === role)
      return existing ?? { role, model_ref: null }
    })
    setRoleConfigs(merged)
  }, [])

  React.useEffect(() => { void loadData() }, [loadData])

  const handleChange = React.useCallback(async (role: string, modelRef: string | null) => {
    // Optimistic update
    setRoleConfigs((prev) =>
      prev.map((r) => (r.role === role ? { ...r, model_ref: modelRef } : r)),
    )
    try {
      await settingsBridge.setRoleModel(role, modelRef)
    } catch (e) {
      toast.error(`保存失败: ${(e as Error).message ?? e}`)
      void loadData() // revert
    }
  }, [loadData])

  return { groups, roleConfigs, allRoles: ALL_ROLES, handleChange }
}
