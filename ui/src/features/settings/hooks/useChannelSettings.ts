// Owns the ChannelSettings provider-list data: the providers list, the
// configured-id set, the per-provider model counts, and refreshData. Extracted
// out of the 455-line ChannelSettings during the split. IPC stays in the typed
// `@/lib/tauri-bridge` provider helpers (a model-provider domain bridge, NOT
// settings-domain — so it stays there rather than moving into settingsBridge;
// the component imports no Tauri API). Behavior preserved verbatim.
import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  listProviders,
  listConfiguredProviders,
  getAllConfiguredModels,
} from '@/lib/tauri-bridge'
import type { ProviderInfo } from '@/lib/types'

export function useChannelSettings() {
  const [providers, setProviders] = useState<ProviderInfo[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [configuredIds, setConfiguredIds] = useState<Set<string>>(new Set())
  const [modelCounts, setModelCounts] = useState<Map<string, number>>(new Map())

  const refreshData = useCallback(async () => {
    const [allProviders, ids, allModels] = await Promise.all([
      listProviders(),
      listConfiguredProviders(),
      getAllConfiguredModels(),
    ])
    setProviders(allProviders)
    setConfiguredIds(new Set(ids))
    const counts = new Map<string, number>()
    allModels.forEach(([pid, mids]) => counts.set(pid, mids.length))
    setModelCounts(counts)
  }, [])

  useEffect(() => {
    void refreshData()
  }, [refreshData])

  const selected = useMemo(
    () => providers.find((p) => p.id === selectedId) ?? null,
    [providers, selectedId],
  )

  const grouped = useMemo(() => {
    const map = new Map<string, ProviderInfo[]>()
    for (const p of providers) {
      const cat = p.serviceCategory || 'Api'
      if (!map.has(cat)) map.set(cat, [])
      map.get(cat)!.push(p)
    }
    return map
  }, [providers])

  return {
    selectedId, setSelectedId,
    configuredIds, modelCounts,
    selected, grouped,
    refreshData,
  }
}
