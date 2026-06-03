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

// The built-in local MiniCPM provider is a zero-config, always-available option:
// it has no API key and is never added to `selected_models`, so it does NOT come
// back from `getAllConfiguredModels()`. Surface it here so users can route roles
// (esp. summarizer / utility) to it from 模型分配 without any manual provider setup —
// matching the "无需手动配置" goal. Scoped to role assignment ONLY (the chat
// quick-picker and IM channels deliberately don't offer the local model).
// The `ref` stays `local-minicpm/minicpm5-1b` so backend routing resolves correctly.
const LOCAL_PROVIDER_ID = 'local-minicpm'
const LOCAL_MODEL_ID = 'minicpm5-1b'

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

    // Always offer the built-in local model (it isn't a "configured" model — see note above).
    const localGroup = g.find((grp) => grp.providerId === LOCAL_PROVIDER_ID)
    if (localGroup) {
      if (!localGroup.models.includes(LOCAL_MODEL_ID)) localGroup.models.push(LOCAL_MODEL_ID)
    } else {
      g.push({ providerId: LOCAL_PROVIDER_ID, models: [LOCAL_MODEL_ID] })
    }
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
