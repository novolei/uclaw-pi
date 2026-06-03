// skills.sh / skillsmp marketplace search result card — renders
// skill_marketplace_search candidates as ranked cards with a single primary
// Install action and a Global / This-workspace scope menu. Shared renderer
// (chat + agent, via ToolResultRenderer). Mirrors bash-result.tsx's
// invoke+useState button pattern. P3 of the marketplace design
// (docs/superpowers/specs/2026-06-01-skills-sh-marketplace-design.md); card
// redesign + popularity sort per
// docs/superpowers/specs/2026-06-02-skill-marketplace-sort-and-card-redesign-design.md.
// Audit badges (skills.sh spec §5) remain a deferred follow-up.
import * as React from 'react'
import { useAtomValue } from 'jotai'
import {
  AlertCircle,
  Check,
  ChevronDown,
  Download,
  FolderPlus,
  Globe,
  Loader2,
  PackageSearch,
} from 'lucide-react'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import { installSkillFromMarketplace } from '@/lib/bridge/skills'
import type { MarketplaceProvider } from '@/lib/types'
import { cn } from '@/lib/utils'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

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

// Compact popularity formatting: 1240 → "1.2k", 38 → "38".
const compact = new Intl.NumberFormat('en', {
  notation: 'compact',
  maximumFractionDigits: 1,
})

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
      <div className="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
        <AlertCircle className="mt-px size-3.5 shrink-0" aria-hidden />
        <span>
          市场搜索失败{!parsed && '（结果解析失败）'}。本地 skill_search 不受影响。
        </span>
      </div>
    )
  }
  // The tool result now carries a top-level provider; default to 'skills_sh'
  // (the default keyless provider) for older results that predate the field.
  const provider: MarketplaceProvider = parsed.provider ?? 'skills_sh'
  // Defensive client-side popularity sort: the backend already ranks by
  // installs desc, but older/cached results may predate that — so re-sort here
  // too. Stable (toSorted preserves order among equal-install ties).
  const rows = [...(parsed.results ?? [])].sort(
    (a, b) => (b.installs ?? 0) - (a.installs ?? 0),
  )
  if (rows.length === 0) {
    return (
      <div className="flex items-center gap-2 rounded-lg border border-border/60 bg-muted/30 px-3 py-2.5 text-xs text-muted-foreground">
        <PackageSearch className="size-3.5 shrink-0" aria-hidden />
        无匹配结果。换个关键词，或用本地 skill_search 试试。
      </div>
    )
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
        const rank = i + 1
        const isTop = rank === 1
        return (
          <div
            key={rowKey}
            className="group flex items-start gap-3 rounded-lg border border-border bg-card/40 px-3 py-2.5 transition-colors duration-200 hover:border-border/80 hover:bg-muted/40"
          >
            {/* Rank badge — #1 gets a faint emerald "top" accent. */}
            <span
              className={cn(
                'mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-md text-[11px] font-semibold tabular-nums',
                isTop
                  ? 'bg-emerald-500/15 text-emerald-400 ring-1 ring-inset ring-emerald-500/30'
                  : 'bg-muted text-muted-foreground',
              )}
              aria-label={`排名第 ${rank}`}
            >
              {rank}
            </span>

            <div className="min-w-0 flex-1">
              <div
                className="truncate text-sm font-medium text-foreground"
                title={row.name}
              >
                {row.name}
              </div>
              {/* Trust row: author/source + popularity metric pill. */}
              <div className="mt-0.5 flex items-center gap-2 text-xs text-muted-foreground">
                <span className="truncate" title={row.source}>
                  {row.source}
                </span>
                {typeof row.installs === 'number' && (
                  <span
                    className="inline-flex shrink-0 items-center gap-1 rounded bg-muted px-1.5 py-0.5 font-medium tabular-nums text-muted-foreground"
                    title={`${row.installs.toLocaleString()} 安装量`}
                  >
                    <Download className="size-3" aria-hidden />
                    {compact.format(row.installs)}
                  </span>
                )}
              </div>
              {row.description && (
                <div className="mt-1 line-clamp-2 text-xs leading-relaxed text-muted-foreground/80">
                  {row.description}
                </div>
              )}
              {st.kind === 'error' && (
                <div className="mt-1 flex items-start gap-1.5 break-words text-xs text-destructive">
                  <AlertCircle className="mt-px size-3 shrink-0" aria-hidden />
                  <span>{st.message}</span>
                </div>
              )}
            </div>

            {/* Single primary action with a scope menu. */}
            {done ? (
              <span className="mt-0.5 inline-flex shrink-0 items-center gap-1 rounded-full bg-emerald-500/15 px-2 py-0.5 text-xs font-medium text-emerald-400">
                <Check className="size-3" aria-hidden />
                已安装（{st.scope === 'global' ? '全局' : '本工作区'}）
              </span>
            ) : (
              <DropdownMenu>
                <DropdownMenuTrigger
                  disabled={busy}
                  className={cn(
                    'mt-0.5 inline-flex shrink-0 cursor-pointer items-center gap-1 rounded-md px-2.5 py-1 text-xs font-medium transition-colors duration-200',
                    'bg-emerald-500/90 text-white hover:bg-emerald-500',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500/50 focus-visible:ring-offset-1 focus-visible:ring-offset-background',
                    'disabled:cursor-default disabled:opacity-60',
                  )}
                  aria-label={`安装 ${row.name}`}
                >
                  {busy ? (
                    <Loader2 className="size-3 animate-spin" aria-hidden />
                  ) : (
                    <Download className="size-3" aria-hidden />
                  )}
                  安装
                  {!busy && <ChevronDown className="size-3 opacity-80" aria-hidden />}
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="min-w-[10rem]">
                  <DropdownMenuItem
                    onSelect={() => install(rowKey, row.id, 'global', row.installUrl)}
                    className="cursor-pointer"
                  >
                    <Globe className="text-muted-foreground" aria-hidden />
                    安装到全局
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    disabled={!activeWorkspaceId}
                    onSelect={() =>
                      install(rowKey, row.id, 'workspace', row.installUrl)
                    }
                    className="cursor-pointer"
                  >
                    <FolderPlus className="text-muted-foreground" aria-hidden />
                    <span className="flex flex-col">
                      安装到本工作区
                      {!activeWorkspaceId && (
                        <span className="text-[10px] text-muted-foreground/70">
                          无活动工作区
                        </span>
                      )}
                    </span>
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            )}
          </div>
        )
      })}
    </div>
  )
}
