// Owns the AgentGrowthTab data: reflections, user model, daydreams (history),
// and user model evolution history. All four are fetched in parallel on mount
// and on explicit refresh(). Mirrors useLearnedProfile's exact shape
// (useCallback/useEffect, loading/error pattern).
import * as React from 'react'
import {
  listReflections,
  getAgentUserModel,
  listDaydreams,
  listUserModelHistory,
} from '@/lib/tauri-bridge'
import type { ReflectionDto, UserModelDto, DaydreamDto, UserModelHistoryDto } from '@/lib/tauri-bridge'

export function useAgentMemory() {
  const [loading, setLoading] = React.useState<boolean>(true)
  const [error, setError] = React.useState<string | null>(null)
  const [reflections, setReflections] = React.useState<ReflectionDto[]>([])
  const [userModel, setUserModel] = React.useState<UserModelDto | null>(null)
  const [daydreams, setDaydreams] = React.useState<DaydreamDto[]>([])
  const [history, setHistory] = React.useState<UserModelHistoryDto[]>([])

  const refresh = React.useCallback(async (): Promise<void> => {
    setLoading(true)
    setError(null)
    try {
      const [refl, um, dd, hist] = await Promise.all([
        listReflections(20),
        getAgentUserModel(),
        listDaydreams(20),
        listUserModelHistory(10),
      ])
      setReflections(Array.isArray(refl) ? refl : [])
      setUserModel(um ?? null)
      setDaydreams(Array.isArray(dd) ? dd : [])
      setHistory(Array.isArray(hist) ? hist : [])
    } catch (e) {
      setError(`加载失败: ${String(e)}`)
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => {
    void refresh()
  }, [refresh])

  return { loading, error, reflections, userModel, daydreams, history, refresh }
}
