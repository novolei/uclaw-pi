// Owns the AgentGrowthTab data: reflections, user model, daydreams (history),
// user model evolution history, and profile facts. All are fetched in parallel
// on mount and on explicit refresh(). Mirrors useLearnedProfile's exact shape
// (useCallback/useEffect, loading/error pattern).
import * as React from 'react'
import {
  listReflections,
  getAgentUserModel,
  listDaydreams,
  listUserModelHistory,
  listProfileFacts,
  archiveReflection,
  restoreReflection,
  triggerMemoryRefresh,
  memoryGraphDeleteNode,
} from '@/lib/tauri-bridge'
import type { ReflectionDto, UserModelDto, DaydreamDto, UserModelHistoryDto, ProfileFactDto } from '@/lib/tauri-bridge'

export function useAgentMemory() {
  const [loading, setLoading] = React.useState<boolean>(true)
  const [error, setError] = React.useState<string | null>(null)
  const [reflections, setReflections] = React.useState<ReflectionDto[]>([])
  const [userModel, setUserModel] = React.useState<UserModelDto | null>(null)
  const [daydreams, setDaydreams] = React.useState<DaydreamDto[]>([])
  const [history, setHistory] = React.useState<UserModelHistoryDto[]>([])
  const [facts, setFacts] = React.useState<ProfileFactDto[]>([])
  const [showArchived, setShowArchived] = React.useState<boolean>(false)

  const refresh = React.useCallback(async (): Promise<void> => {
    setLoading(true)
    setError(null)
    try {
      const [refl, um, dd, hist, fcts] = await Promise.all([
        listReflections(20, showArchived),
        getAgentUserModel(),
        listDaydreams(20),
        listUserModelHistory(10),
        listProfileFacts(),
      ])
      setReflections(Array.isArray(refl) ? refl : [])
      setUserModel(um ?? null)
      setDaydreams(Array.isArray(dd) ? dd : [])
      setHistory(Array.isArray(hist) ? hist : [])
      setFacts(Array.isArray(fcts) ? fcts : [])
    } catch (e) {
      setError(`加载失败: ${String(e)}`)
    } finally {
      setLoading(false)
    }
  }, [showArchived])

  React.useEffect(() => {
    void refresh()
  }, [refresh])

  const deleteFact = React.useCallback(async (id: string): Promise<void> => {
    await memoryGraphDeleteNode({ nodeId: id })
    void refresh()
  }, [refresh])

  const archiveRefl = React.useCallback(async (id: string): Promise<void> => {
    await archiveReflection(id)
    void refresh()
  }, [refresh])

  const restoreRefl = React.useCallback(async (id: string): Promise<void> => {
    await restoreReflection(id)
    void refresh()
  }, [refresh])

  const refreshMemory = React.useCallback(async (): Promise<void> => {
    await triggerMemoryRefresh()
    setTimeout(() => void refresh(), 3000)
  }, [refresh])

  const toggleArchived = React.useCallback((): void => {
    setShowArchived((prev) => !prev)
  }, [])

  return {
    loading,
    error,
    reflections,
    userModel,
    daydreams,
    history,
    facts,
    showArchived,
    refresh,
    deleteFact,
    archiveRefl,
    restoreRefl,
    refreshMemory,
    toggleArchived,
  }
}
