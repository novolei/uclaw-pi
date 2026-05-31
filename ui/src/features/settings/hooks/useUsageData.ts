// Owns the UsageSettings data: the last-30-days daily/model/session rollups + the
// loading flag, the month-total / workspace-rollup / budget atoms, and the live
// re-fetch wiring (subscribes to agent:turn_cost, debounced 1s, re-fetching both
// the daily rollup AND the monthly totals). Extracted out of the component during
// the features/settings migration (code-organization ADR 2026-05-31). The typed
// @/lib/tauri-bridge cost helpers + the @/atoms/cost atoms stay in the hook
// (precedent: useChannelSettings). Behavior preserved verbatim.
import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import {
  monthTotalUsdAtom,
  workspaceRollupAtom,
  monthlyBudgetUsdAtom,
  refreshCostsAtom,
  loadBudgetAtom,
  setBudgetAtom,
} from '@/atoms/cost'
import {
  getDailyCosts, getModelCosts, getSessionCosts, onTurnCost,
} from '@/lib/tauri-bridge'
import type {
  DailyCostRollup, ModelCostRollup, SessionCostRollup,
} from '@/lib/types'

export function useUsageData() {
  const [daily, setDaily] = React.useState<DailyCostRollup[]>([])
  const [models, setModels] = React.useState<ModelCostRollup[]>([])
  const [sessions, setSessions] = React.useState<SessionCostRollup[]>([])
  const [loading, setLoading] = React.useState(true)

  const monthTotal = useAtomValue(monthTotalUsdAtom)
  const wsRollup = useAtomValue(workspaceRollupAtom)
  const budget = useAtomValue(monthlyBudgetUsdAtom)
  const refreshCosts = useSetAtom(refreshCostsAtom)
  const loadBudget = useSetAtom(loadBudgetAtom)
  const saveBudget = useSetAtom(setBudgetAtom)

  const refetch = React.useCallback(async () => {
    setLoading(true)
    try {
      const [d, m, s] = await Promise.all([
        getDailyCosts(30),
        getModelCosts(30),
        getSessionCosts(30, 50),
      ])
      setDaily(d)
      setModels(m)
      setSessions(s)
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => {
    void refetch()
    void refreshCosts()
    void loadBudget()
  }, [refetch, refreshCosts, loadBudget])

  // Debounced re-fetch on new turn_cost events
  React.useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null
    const unlistenP = onTurnCost(() => {
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => {
        void refetch()
        void refreshCosts()  // Phase 6-C: keep the monthly view live too
      }, 1000)
    })
    return () => {
      if (timer) clearTimeout(timer)
      void unlistenP.then((u) => u())
    }
  }, [refetch, refreshCosts])

  const totals = React.useMemo(() => {
    const cost = daily.reduce((a, d) => a + d.costUsd, 0)
    const inTok = daily.reduce((a, d) => a + d.inputTokens, 0)
    const outTok = daily.reduce((a, d) => a + d.outputTokens, 0)
    const turns = daily.reduce((a, d) => a + d.turnCount, 0)
    return { cost, inTok, outTok, turns }
  }, [daily])

  return {
    daily, models, sessions, loading, totals,
    monthTotal, wsRollup, budget, saveBudget,
  }
}
