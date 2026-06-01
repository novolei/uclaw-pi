// skills.sh marketplace search result card — renders skill_marketplace_search
// candidates with [全局]/[本工作区] install buttons. Shared renderer (chat + agent,
// via ToolResultRenderer). Mirrors bash-result.tsx's invoke+useState button
// pattern. P3 of the marketplace design
// (docs/superpowers/specs/2026-06-01-skills-sh-marketplace-design.md). Audit
// badges (spec §5) are a deferred follow-up.
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import { installSkillFromMarketplace } from '@/lib/bridge/skills'
import type { MarketplaceProvider } from '@/lib/types'

interface SkillRow {
  id: string
  name: string
  source: string
  installs?: number
  installUrl?: string
  description?: string
}
type RowState =
  | { kind: 'idle' }
  | { kind: 'installing'; scope: 'global' | 'workspace' }
  | { kind: 'installed'; scope: 'global' | 'workspace' }
  | { kind: 'error'; message: string }

export function SkillMarketplaceSearchResultCard({
  result,
  isError,
}: {
  result: string
  isError?: boolean
}) {
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const [states, setStates] = React.useState<Record<string, RowState>>({})

  const parsed = React.useMemo(() => {
    try {
      return JSON.parse(result) as {
        results?: SkillRow[]
        note?: string
        provider?: MarketplaceProvider
      }
    } catch {
      return null
    }
  }, [result])

  if (isError || !parsed) {
    return (
      <div className="text-xs text-red-400/90 bg-red-400/10 rounded-lg px-3 py-2">
        skills.sh 搜索失败{!parsed && '（结果解析失败）'}。本地 skill_search 不受影响。
      </div>
    )
  }
  const rows = parsed.results ?? []
  // The tool result now carries a top-level provider; default to 'skillsmp'
  // (the keyless search-only provider) for older results that predate the field.
  const provider: MarketplaceProvider = parsed.provider ?? 'skillsmp'
  if (rows.length === 0) {
    return <div className="text-xs text-muted-foreground px-1 py-1">skills.sh 无匹配结果。</div>
  }

  // rowKey (id + index) de-collides the rare case of two results sharing an id,
  // so one row's install state can never drive another's spinner. `source` is the
  // row's installUrl (skillsmp installs/previews from the GitHub URL).
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
    } catch (e) {
      setStates((s) => ({ ...s, [rowKey]: { kind: 'error', message: String(e) } }))
    }
  }

  return (
    <div className="flex flex-col gap-1.5">
      {rows.map((row, i) => {
        const rowKey = `${row.id}__${i}`
        const st: RowState = states[rowKey] ?? { kind: 'idle' }
        const busy = st.kind === 'installing'
        const done = st.kind === 'installed'
        return (
          <div
            key={rowKey}
            className="rounded-lg border border-border px-3 py-2 flex items-center justify-between gap-3"
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
              <div className="shrink-0 flex items-center gap-1.5">
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
  )
}
