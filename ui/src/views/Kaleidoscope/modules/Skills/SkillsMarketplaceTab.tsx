/**
 * SkillsMarketplaceTab — 万花筒「技能」模块的「市场」分页(双 provider 浏览器)。
 *
 * provider 切换(默认 skillsmp.com,另一个为 skills.sh):
 *   - skillsmp:无 list/trending —— 空 query 时不拉取(提示输入关键词);非空时
 *     searchSkillsMarketplace(query, 20, 'skillsmp')。免 key,达匿名限流提示填 key。
 *   - skills.sh:空 query → listSkillsMarketplace('trending', 0, 'skills_sh');
 *     非空 → searchSkillsMarketplace(query, 20, 'skills_sh')。
 * 非空 query 走 ~300ms 防抖。每行复用 P3 安装卡片
 * (skill-marketplace-search-result.tsx) 的逐行状态机
 * (idle → installing → installed / error) 与 [全局]/[本工作区] 按钮,安装时透传
 * provider + 该行 installUrl(source)。点击行体(非按钮)打开右侧详情抽屉
 * (SkillMarketplaceDetailDrawer),同样透传 provider + source。P4 of the marketplace
 * design (docs/superpowers/specs/2026-06-01-skills-sh-marketplace-design.md)。
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Loader2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import {
  installSkillFromMarketplace,
  uninstallSkillFromMarketplace,
  listSkillsMarketplace,
  searchSkillsMarketplace,
} from '@/lib/bridge/skills'
import { listStandaloneInstalls, type StandaloneInstall } from '@/lib/tauri-bridge'
import type { MarketplaceProvider, MarketplaceSkillSummary } from '@/lib/types'
import { SkillMarketplaceDetailDrawer } from './SkillMarketplaceDetailDrawer'

type RowState =
  | { kind: 'idle' }
  | { kind: 'installing'; scope: 'global' | 'workspace' }
  | { kind: 'installed'; scope: 'global' | 'workspace' }
  | { kind: 'error'; message: string }

// Per-row state for the "已安装" list. `confirming` is the inline 确认卸载？gate
// before the destructive call; de-collided by slug (mirrors the install RowState).
type UninstallState =
  | { kind: 'idle' }
  | { kind: 'confirming' }
  | { kind: 'uninstalling' }
  | { kind: 'error'; message: string }

const PROVIDER_TABS: { value: MarketplaceProvider; label: string }[] = [
  { value: 'skillsmp', label: 'skillsmp.com' },
  { value: 'skills_sh', label: 'skills.sh' },
]

/** "请填 key" 提示判定:后端把缺失 key 的拒绝串里带上 "API key" / "key"。 */
function looksLikeMissingKey(message: string): boolean {
  return /api key|key/i.test(message)
}

/** 限流判定:后端把 429 / rate-limit 拒绝串里带上 "429" / "rate" / "限流"。 */
function looksLikeRateLimited(message: string): boolean {
  return /429|rate.?limit|限流/i.test(message)
}

export function SkillsMarketplaceTab({
  query,
  onError,
}: {
  query: string
  onError?: (m: string) => void
}): React.ReactElement {
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  // Default to skillsmp.com (keyless search-only); skills.sh is the alternate.
  const [provider, setProvider] = React.useState<MarketplaceProvider>('skillsmp')
  const [rows, setRows] = React.useState<MarketplaceSkillSummary[]>([])
  const [loading, setLoading] = React.useState(true)
  const [error, setError] = React.useState<string | null>(null)
  // skillsmp has no list/trending → an empty query shows a "type to search" hint
  // (not a fetch, not a "no results"). Cleared whenever a fetch starts.
  const [emptyHint, setEmptyHint] = React.useState<string | null>(null)
  const [states, setStates] = React.useState<Record<string, RowState>>({})
  const [detailId, setDetailId] = React.useState<string | null>(null)
  const [detailSource, setDetailSource] = React.useState<string | undefined>(undefined)
  const [detailOpen, setDetailOpen] = React.useState(false)
  // "已安装" list (V25 source of truth, filtered to itemType === 'skill') + its
  // per-row uninstall states. `refreshKey` is bumped after any install/uninstall
  // so the fetch effect re-runs and the list stays current.
  const [installed, setInstalled] = React.useState<StandaloneInstall[]>([])
  const [uninstallStates, setUninstallStates] = React.useState<Record<string, UninstallState>>({})
  const [refreshKey, setRefreshKey] = React.useState(0)

  // skills.sh: empty query → trending list; non-empty → debounced (~300ms) search.
  // skillsmp: no list → empty query shows a hint (no fetch); non-empty → debounced
  // search. The effect is keyed on [query, provider] so toggling provider refetches
  // and resets; cleanup clears the pending timer + a `cancelled` guard drops late
  // responses so a stale fetch can't overwrite a newer one.
  React.useEffect(() => {
    let cancelled = false
    const q = query.trim()

    const run = async () => {
      setLoading(true)
      setError(null)
      setEmptyHint(null)
      try {
        const result = q
          ? await searchSkillsMarketplace(q, 20, provider)
          : await listSkillsMarketplace('trending', 0, provider)
        if (cancelled) return
        setRows(result)
      } catch (e) {
        if (cancelled) return
        const raw = String(e)
        let message: string
        if (provider === 'skillsmp') {
          message = looksLikeRateLimited(raw)
            ? 'skillsmp.com 已达匿名限流，可在设置填 API key 提高额度'
            : 'skillsmp.com 暂不可用，请重试'
        } else {
          message = looksLikeMissingKey(raw)
            ? '请在设置填 skills.sh API key（系统诊断 tab）'
            : 'skills.sh 暂不可用，请重试'
        }
        setError(message)
        setRows([])
        onError?.(message)
      } finally {
        if (!cancelled) setLoading(false)
      }
    }

    // skillsmp + empty query: no list endpoint → hint, don't fetch.
    if (!q && provider === 'skillsmp') {
      setLoading(false)
      setError(null)
      setRows([])
      setEmptyHint('输入关键词搜索 skillsmp.com')
      return () => {
        cancelled = true
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
    // and we only want to refetch when the query text or provider changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, provider])

  // Fetch the "已安装" list (marketplace skills only). Re-runs on every refreshKey
  // bump (post-install / post-uninstall). On error we just show nothing — the
  // 已安装 section is auxiliary and must never break the search/install tab.
  React.useEffect(() => {
    let cancelled = false
    listStandaloneInstalls()
      .then((all) => {
        if (cancelled) return
        setInstalled(all.filter((it) => it.itemType === 'skill'))
      })
      .catch(() => {
        if (cancelled) return
        setInstalled([])
      })
    return () => {
      cancelled = true
    }
  }, [refreshKey])

  // rowKey (id + index) de-collides the rare case of two results sharing an id,
  // so one row's install state can never drive another's spinner. (Mirrors the
  // P3 search-result card.) `source` is the row's installUrl — skillsmp installs
  // from the GitHub URL; skills.sh ignores it.
  const install = async (
    rowKey: string,
    id: string,
    scope: 'global' | 'workspace',
    source?: string,
  ) => {
    setStates((s) => ({ ...s, [rowKey]: { kind: 'installing', scope } }))
    try {
      await installSkillFromMarketplace(
        id,
        scope,
        scope === 'workspace' ? activeWorkspaceId ?? undefined : undefined,
        provider,
        source,
      )
      setStates((s) => ({ ...s, [rowKey]: { kind: 'installed', scope } }))
      // Keep the 已安装 list current after a fresh install.
      setRefreshKey((k) => k + 1)
    } catch (e) {
      setStates((s) => ({ ...s, [rowKey]: { kind: 'error', message: String(e) } }))
    }
  }

  // Uninstall a marketplace skill by slug. Fully removes files + V25 row + registry
  // entry on the backend; on success we refetch the 已安装 list via refreshKey.
  // De-collided by slug, mirroring the install state-machine shape.
  const uninstall = async (slug: string) => {
    setUninstallStates((s) => ({ ...s, [slug]: { kind: 'uninstalling' } }))
    try {
      await uninstallSkillFromMarketplace(slug)
      setUninstallStates((s) => ({ ...s, [slug]: { kind: 'idle' } }))
      setRefreshKey((k) => k + 1)
    } catch (e) {
      setUninstallStates((s) => ({ ...s, [slug]: { kind: 'error', message: String(e) } }))
    }
  }

  const openDetail = (id: string, source?: string) => {
    setDetailId(id)
    setDetailSource(source)
    setDetailOpen(true)
  }

  return (
    <div className="titlebar-no-drag flex-1 min-h-0 overflow-y-auto px-5 md:px-8 py-4">
      {/* "已安装" — marketplace skills from the V25 table. Shown only when non-empty,
          above the provider toggle / search results. Each row: slug (+version) with a
          卸载 button that gates on an inline 确认卸载？ before the destructive call. */}
      {installed.length > 0 && (
        <div className="mb-4">
          <div className="text-xs font-medium text-muted-foreground mb-1.5 px-1">已安装</div>
          <div className="flex flex-col gap-1.5">
            {installed.map((it) => {
              const ust: UninstallState = uninstallStates[it.slug] ?? { kind: 'idle' }
              const busy = ust.kind === 'uninstalling'
              return (
                <div
                  key={it.slug}
                  className="rounded-lg border border-border px-3 py-2 flex items-center justify-between gap-3"
                >
                  <div className="min-w-0">
                    <div className="text-sm text-foreground truncate">{it.slug}</div>
                    {it.version && (
                      <div className="text-xs text-muted-foreground truncate">v{it.version}</div>
                    )}
                    {ust.kind === 'error' && (
                      <div className="text-xs text-red-400/90 mt-0.5 break-words">{ust.message}</div>
                    )}
                  </div>
                  {ust.kind === 'confirming' ? (
                    <div className="shrink-0 flex items-center gap-1.5">
                      <span className="text-xs text-muted-foreground">确认卸载？</span>
                      <button
                        type="button"
                        onClick={() => void uninstall(it.slug)}
                        className="text-xs px-2.5 py-1 rounded-lg bg-red-500/15 text-red-400 hover:bg-red-500/25 transition-colors"
                      >
                        确认
                      </button>
                      <button
                        type="button"
                        onClick={() =>
                          setUninstallStates((s) => ({ ...s, [it.slug]: { kind: 'idle' } }))
                        }
                        className="text-xs px-2.5 py-1 rounded-lg bg-muted text-muted-foreground hover:bg-muted/70 transition-colors"
                      >
                        取消
                      </button>
                    </div>
                  ) : (
                    <button
                      type="button"
                      onClick={() =>
                        setUninstallStates((s) => ({ ...s, [it.slug]: { kind: 'confirming' } }))
                      }
                      disabled={busy}
                      className="shrink-0 text-xs px-2.5 py-1 rounded-lg bg-muted text-muted-foreground hover:bg-muted/70 transition-colors disabled:opacity-50"
                    >
                      {busy ? '…' : '卸载'}
                    </button>
                  )}
                </div>
              )
            })}
          </div>
        </div>
      )}

      {/* Provider segmented toggle (mirrors SkillsModule's filter-tab styling). */}
      <div className="flex gap-1 mb-3">
        {PROVIDER_TABS.map((tab) => (
          <button
            key={tab.value}
            type="button"
            onClick={() => setProvider(tab.value)}
            className={cn(
              'rounded-full px-2.5 py-1 text-[10.5px] font-medium transition-all duration-200 whitespace-nowrap shrink-0 active:scale-95',
              provider === tab.value
                ? 'bg-primary text-primary-foreground shadow-[0_1px_4px_rgba(0,0,0,0.1)]'
                : 'text-muted-foreground hover:text-foreground hover:bg-muted/60',
            )}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {loading ? (
        <div className="flex items-center gap-2 text-xs text-muted-foreground px-1 py-2">
          <Loader2 className="size-3.5 animate-spin shrink-0" />
          加载中…
        </div>
      ) : error ? (
        <div className="text-xs text-red-400/90 bg-red-400/10 rounded-lg px-3 py-2">
          {error}
        </div>
      ) : emptyHint ? (
        <div className="text-xs text-muted-foreground px-1 py-1">{emptyHint}</div>
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
                onClick={() => openDetail(row.id, row.installUrl)}
                className="rounded-lg border border-border px-3 py-2 flex items-center justify-between gap-3 cursor-pointer hover:border-border/80 hover:bg-muted/30 transition-colors"
              >
                <div className="min-w-0">
                  <div className="text-sm text-foreground truncate">{row.name}</div>
                  <div className="text-xs text-muted-foreground truncate">
                    {row.source}
                    {typeof row.installs === 'number' && ` · ${row.installs} installs`}
                  </div>
                  {row.description && (
                    <div className="text-xs text-muted-foreground/80 truncate mt-0.5">
                      {row.description}
                    </div>
                  )}
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
                      onClick={() => install(rowKey, row.id, 'global', row.installUrl)}
                      disabled={busy}
                      className="text-xs px-2.5 py-1 rounded-lg bg-muted text-muted-foreground hover:bg-muted/70 transition-colors disabled:opacity-50"
                    >
                      {busy && st.scope === 'global' ? '…' : '全局'}
                    </button>
                    <button
                      type="button"
                      onClick={() => install(rowKey, row.id, 'workspace', row.installUrl)}
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
        provider={provider}
        source={detailSource}
        open={detailOpen}
        onOpenChange={setDetailOpen}
        onError={onError}
      />
    </div>
  )
}
