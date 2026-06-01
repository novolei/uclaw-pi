/**
 * SkillsMarketplaceTab — 万花筒「技能」模块的「市场」分页(skills.sh 浏览器)。
 *
 * 空 query 时拉取 listSkillsMarketplace('trending');非空时 ~300ms 防抖后
 * searchSkillsMarketplace(query, 20)。每行复用 P3 安装卡片
 * (skill-marketplace-search-result.tsx) 的逐行状态机
 * (idle → installing → installed / error) 与 [全局]/[本工作区] 按钮。点击行体
 * (非按钮) 打开右侧详情抽屉(SkillMarketplaceDetailDrawer)。P4 of the marketplace
 * design (docs/superpowers/specs/2026-06-01-skills-sh-marketplace-design.md)。
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Loader2 } from 'lucide-react'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import {
  installSkillFromMarketplace,
  listSkillsMarketplace,
  searchSkillsMarketplace,
} from '@/lib/bridge/skills'
import type { MarketplaceSkillSummary } from '@/lib/types'
import { SkillMarketplaceDetailDrawer } from './SkillMarketplaceDetailDrawer'

type RowState =
  | { kind: 'idle' }
  | { kind: 'installing'; scope: 'global' | 'workspace' }
  | { kind: 'installed'; scope: 'global' | 'workspace' }
  | { kind: 'error'; message: string }

/** "请填 key" 提示判定:后端把缺失 key 的拒绝串里带上 "API key" / "key"。 */
function looksLikeMissingKey(message: string): boolean {
  return /api key|key/i.test(message)
}

export function SkillsMarketplaceTab({
  query,
  onError,
}: {
  query: string
  onError?: (m: string) => void
}): React.ReactElement {
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const [rows, setRows] = React.useState<MarketplaceSkillSummary[]>([])
  const [loading, setLoading] = React.useState(true)
  const [error, setError] = React.useState<string | null>(null)
  const [states, setStates] = React.useState<Record<string, RowState>>({})
  const [detailId, setDetailId] = React.useState<string | null>(null)
  const [detailOpen, setDetailOpen] = React.useState(false)

  // Empty query → trending list; non-empty → debounced (~300ms) search. The
  // effect is keyed on query; its cleanup clears the pending timer + a
  // `cancelled` guard drops late responses so a stale fetch can't overwrite a
  // newer one.
  React.useEffect(() => {
    let cancelled = false
    const q = query.trim()

    const run = async () => {
      setLoading(true)
      setError(null)
      try {
        const result = q
          ? await searchSkillsMarketplace(q, 20)
          : await listSkillsMarketplace('trending')
        if (cancelled) return
        setRows(result)
      } catch (e) {
        if (cancelled) return
        const message = looksLikeMissingKey(String(e))
          ? '请在设置填 skills.sh API key（系统诊断 tab）'
          : 'skills.sh 暂不可用，请重试'
        setError(message)
        setRows([])
        onError?.(message)
      } finally {
        if (!cancelled) setLoading(false)
      }
    }

    if (!q) {
      void run()
      return () => {
        cancelled = true
      }
    }
    const timer = setTimeout(() => void run(), 300)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
    // onError intentionally omitted: parent passes a fresh closure each render
    // and we only want to refetch when the query text changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query])

  // rowKey (id + index) de-collides the rare case of two results sharing an id,
  // so one row's install state can never drive another's spinner. (Mirrors the
  // P3 search-result card.)
  const install = async (rowKey: string, id: string, scope: 'global' | 'workspace') => {
    setStates((s) => ({ ...s, [rowKey]: { kind: 'installing', scope } }))
    try {
      await installSkillFromMarketplace(
        id,
        scope,
        scope === 'workspace' ? activeWorkspaceId ?? undefined : undefined,
      )
      setStates((s) => ({ ...s, [rowKey]: { kind: 'installed', scope } }))
    } catch (e) {
      setStates((s) => ({ ...s, [rowKey]: { kind: 'error', message: String(e) } }))
    }
  }

  const openDetail = (id: string) => {
    setDetailId(id)
    setDetailOpen(true)
  }

  return (
    <div className="titlebar-no-drag flex-1 min-h-0 overflow-y-auto px-5 md:px-8 py-4">
      {loading ? (
        <div className="flex items-center gap-2 text-xs text-muted-foreground px-1 py-2">
          <Loader2 className="size-3.5 animate-spin shrink-0" />
          加载中…
        </div>
      ) : error ? (
        <div className="text-xs text-red-400/90 bg-red-400/10 rounded-lg px-3 py-2">
          {error}
        </div>
      ) : rows.length === 0 ? (
        <div className="text-xs text-muted-foreground px-1 py-1">无匹配结果</div>
      ) : (
        <div className="flex flex-col gap-1.5">
          {rows.map((row, i) => {
            const rowKey = `${row.id}__${i}`
            const st: RowState = states[rowKey] ?? { kind: 'idle' }
            const busy = st.kind === 'installing'
            const done = st.kind === 'installed'
            return (
              <div
                key={rowKey}
                onClick={() => openDetail(row.id)}
                className="rounded-lg border border-border px-3 py-2 flex items-center justify-between gap-3 cursor-pointer hover:border-border/80 hover:bg-muted/30 transition-colors"
              >
                <div className="min-w-0">
                  <div className="text-sm text-foreground truncate">{row.name}</div>
                  <div className="text-xs text-muted-foreground truncate">
                    {row.source}
                    {typeof row.installs === 'number' && ` · ${row.installs} installs`}
                  </div>
                  {st.kind === 'error' && (
                    <div className="text-xs text-red-400/90 mt-0.5 break-words">{st.message}</div>
                  )}
                </div>
                {done ? (
                  <span className="shrink-0 text-xs px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-400">
                    已安装（{st.scope === 'global' ? '全局' : '本工作区'}）
                  </span>
                ) : (
                  <div
                    className="shrink-0 flex items-center gap-1.5"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <button
                      type="button"
                      onClick={() => install(rowKey, row.id, 'global')}
                      disabled={busy}
                      className="text-xs px-2.5 py-1 rounded-lg bg-muted text-muted-foreground hover:bg-muted/70 transition-colors disabled:opacity-50"
                    >
                      {busy && st.scope === 'global' ? '…' : '全局'}
                    </button>
                    <button
                      type="button"
                      onClick={() => install(rowKey, row.id, 'workspace')}
                      disabled={busy || !activeWorkspaceId}
                      title={activeWorkspaceId ? '安装到当前工作区' : '无活动工作区'}
                      className="text-xs px-2.5 py-1 rounded-lg bg-muted text-muted-foreground hover:bg-muted/70 transition-colors disabled:opacity-50"
                    >
                      {busy && st.scope === 'workspace' ? '…' : '本工作区'}
                    </button>
                  </div>
                )}
              </div>
            )
          })}
        </div>
      )}

      <SkillMarketplaceDetailDrawer
        id={detailId}
        open={detailOpen}
        onOpenChange={setDetailOpen}
        onError={onError}
      />
    </div>
  )
}
