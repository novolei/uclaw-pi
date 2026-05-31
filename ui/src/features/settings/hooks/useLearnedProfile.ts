// Owns the LearnedProfileTab data: the facet list + the loading/rebuilding/error/
// per-row-busy flags, the five IPC actions (fetch / rebuild / dismiss / promote /
// demote), the class-grouping memo, and the active/provisional counts. Extracted
// out of the component during the features/settings migration (code-organization
// ADR 2026-05-31). The typed @/lib/tauri-bridge memoryLearning* helpers stay in the
// hook (precedent: useChannelSettings). Behavior preserved verbatim — same
// optimistic local state flips, the same toast copy, the same disabled-error branch.
import * as React from 'react'
import { toast } from 'sonner'
import {
  memoryLearningListFacets,
  memoryLearningDismissFacet,
  memoryLearningRebuildNow,
  memoryLearningPromoteFacet,
  memoryLearningDemoteFacet,
} from '@/lib/tauri-bridge'
import type { FacetDto } from '@/lib/types'

export function useLearnedProfile() {
  const [facets, setFacets] = React.useState<FacetDto[]>([])
  const [loading, setLoading] = React.useState<boolean>(true)
  const [rebuilding, setRebuilding] = React.useState<boolean>(false)
  const [error, setError] = React.useState<string | null>(null)
  const [dismissing, setDismissing] = React.useState<Set<string>>(new Set())

  const fetchFacets = React.useCallback(async (): Promise<void> => {
    setLoading(true)
    setError(null)
    try {
      const list = await memoryLearningListFacets({})
      setFacets(Array.isArray(list) ? list : [])
    } catch (e) {
      setError(`加载失败: ${String(e)}`)
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => {
    void fetchFacets()
  }, [fetchFacets])

  const handleRebuild = React.useCallback(async (): Promise<void> => {
    setRebuilding(true)
    setError(null)
    try {
      await memoryLearningRebuildNow({})
      await fetchFacets()
      toast.success('已重建 — Profile 已根据最新候选刷新')
    } catch (e) {
      const msg = String(e)
      setError(msg)
      // The structured "learning disabled" error is friendlier as a toast.
      if (msg.toLowerCase().includes('disabled')) {
        toast.error('学习管线已关闭 — 在「智能」页打开 memory_os.learning_enabled')
      } else {
        toast.error(`重建失败: ${msg}`)
      }
    } finally {
      setRebuilding(false)
    }
  }, [fetchFacets])

  const handleDismiss = React.useCallback(async (facetId: string): Promise<void> => {
    setDismissing((prev) => new Set(prev).add(facetId))
    try {
      await memoryLearningDismissFacet({ facetId })
      // Optimistic: flip state to "forgotten" locally so the row dims
      // but stays — matches the backend (it doesn't delete, just flags).
      setFacets((prev) =>
        prev.map((f) =>
          f.facetId === facetId ? { ...f, state: 'forgotten' } : f,
        ),
      )
    } catch (e) {
      toast.error(`移除失败: ${String(e)}`)
    } finally {
      setDismissing((prev) => {
        const next = new Set(prev)
        next.delete(facetId)
        return next
      })
    }
  }, [])

  // Sprint 2.3 — promote / demote share the same shape as dismiss. The
  // optimistic local update mirrors the backend's UPDATE: state column
  // flips, the row stays. Next stability rebuild can override.
  const handlePromote = React.useCallback(async (facetId: string): Promise<void> => {
    setDismissing((prev) => new Set(prev).add(facetId))
    try {
      await memoryLearningPromoteFacet({ facetId })
      setFacets((prev) =>
        prev.map((f) =>
          f.facetId === facetId ? { ...f, state: 'active' } : f,
        ),
      )
    } catch (e) {
      toast.error(`提升失败: ${String(e)}`)
    } finally {
      setDismissing((prev) => {
        const next = new Set(prev)
        next.delete(facetId)
        return next
      })
    }
  }, [])

  const handleDemote = React.useCallback(async (facetId: string): Promise<void> => {
    setDismissing((prev) => new Set(prev).add(facetId))
    try {
      await memoryLearningDemoteFacet({ facetId })
      setFacets((prev) =>
        prev.map((f) =>
          f.facetId === facetId ? { ...f, state: 'provisional' } : f,
        ),
      )
    } catch (e) {
      toast.error(`降级失败: ${String(e)}`)
    } finally {
      setDismissing((prev) => {
        const next = new Set(prev)
        next.delete(facetId)
        return next
      })
    }
  }, [])

  // ─── Group facets by class ───────────────────────────────────────
  const grouped = React.useMemo(() => {
    const buckets = new Map<string, FacetDto[]>()
    for (const f of facets) {
      const key = f.class.toLowerCase()
      const arr = buckets.get(key) ?? []
      arr.push(f)
      buckets.set(key, arr)
    }
    // Sort each bucket: active first, then provisional, then by
    // stability descending so the strongest evidence sits on top.
    for (const [k, arr] of buckets) {
      arr.sort((a, b) => {
        const stateOrder: Record<string, number> = {
          active: 0,
          provisional: 1,
          candidate: 2,
          forgotten: 3,
        }
        const sa = stateOrder[a.state.toLowerCase()] ?? 99
        const sb = stateOrder[b.state.toLowerCase()] ?? 99
        if (sa !== sb) return sa - sb
        return b.stability - a.stability
      })
      buckets.set(k, arr)
    }
    return buckets
  }, [facets])

  const activeCount = facets.filter(
    (f) => f.state.toLowerCase() === 'active',
  ).length
  const provisionalCount = facets.filter(
    (f) => f.state.toLowerCase() === 'provisional',
  ).length

  return {
    facets,
    loading,
    rebuilding,
    error,
    dismissing,
    grouped,
    activeCount,
    provisionalCount,
    fetchFacets,
    handleRebuild,
    handleDismiss,
    handlePromote,
    handleDemote,
  }
}
