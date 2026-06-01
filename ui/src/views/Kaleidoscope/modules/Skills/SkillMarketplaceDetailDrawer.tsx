/**
 * SkillMarketplaceDetailDrawer — 右侧抽屉(Sheet)展示某个 skills.sh 技能的详情。
 *
 * open + id 时并行拉取 getSkillMarketplaceDetail(id) 与 getSkillMarketplaceAudit(id)
 * (各自独立失败)。展示:技能 id/source 标题、由 audit.audits 推导的最高风险徽章
 * (HIGH > MEDIUM > LOW)、SKILL.md 只读预览(滚动 <pre>)、[全局]/[本工作区] 安装
 * 按钮(复用 P3 状态机)。最高风险为 HIGH 时,点击安装先弹内联确认。
 * P4 of the marketplace design
 * (docs/superpowers/specs/2026-06-01-skills-sh-marketplace-design.md)。
 */
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { Loader2 } from 'lucide-react'
import { activeWorkspaceIdAtom } from '@/atoms/workspace'
import {
  getSkillMarketplaceAudit,
  getSkillMarketplaceDetail,
  installSkillFromMarketplace,
} from '@/lib/bridge/skills'
import type { MarketplaceSkillAudit, MarketplaceSkillDetail } from '@/lib/types'
import { Sheet, SheetContent, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { cn } from '@/lib/utils'

type RowState =
  | { kind: 'idle' }
  | { kind: 'installing'; scope: 'global' | 'workspace' }
  | { kind: 'installed'; scope: 'global' | 'workspace' }
  | { kind: 'error'; message: string }

type Risk = 'HIGH' | 'MEDIUM' | 'LOW' | null

/** Highest risk among audit entries; null = no audits / unaudited. */
function highestRisk(audit: MarketplaceSkillAudit | null): Risk {
  if (!audit || audit.audits.length === 0) return null
  const levels = audit.audits.map((a) => a.riskLevel.toUpperCase())
  if (levels.includes('HIGH')) return 'HIGH'
  if (levels.includes('MEDIUM')) return 'MEDIUM'
  if (levels.includes('LOW')) return 'LOW'
  return null
}

// Risk badge color idiom mirrors SkillDetail.tsx's LIFECYCLE/PROVENANCE badges.
const RISK_BADGE: Record<'HIGH' | 'MEDIUM' | 'LOW' | 'NONE', { label: string; className: string }> = {
  HIGH: { label: '⚠ 高风险', className: 'bg-red-500/15 text-red-600 border-red-500/30 dark:text-red-400' },
  MEDIUM: { label: '⚠ 中', className: 'bg-amber-500/15 text-amber-700 border-amber-500/30 dark:text-amber-400' },
  LOW: { label: '✓ 低', className: 'bg-emerald-500/15 text-emerald-700 border-emerald-500/30 dark:text-emerald-400' },
  NONE: { label: '未审计', className: 'bg-muted text-muted-foreground border-border' },
}

export function SkillMarketplaceDetailDrawer({
  id,
  open,
  onOpenChange,
  onError,
}: {
  id: string | null
  open: boolean
  onOpenChange: (open: boolean) => void
  onError?: (m: string) => void
}): React.ReactElement {
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const [detail, setDetail] = React.useState<MarketplaceSkillDetail | null>(null)
  const [audit, setAudit] = React.useState<MarketplaceSkillAudit | null>(null)
  const [loading, setLoading] = React.useState(false)
  const [detailErr, setDetailErr] = React.useState<string | null>(null)
  const [installState, setInstallState] = React.useState<RowState>({ kind: 'idle' })
  const [confirmingHighRisk, setConfirmingHighRisk] = React.useState(false)
  const [pendingScope, setPendingScope] = React.useState<'global' | 'workspace' | null>(null)

  React.useEffect(() => {
    if (!open || !id) return
    let cancelled = false
    setLoading(true)
    setDetail(null)
    setAudit(null)
    setDetailErr(null)
    setInstallState({ kind: 'idle' })
    setConfirmingHighRisk(false)
    setPendingScope(null)

    const run = async () => {
      // Detail + audit fetch in parallel; each settles independently so an audit
      // failure still shows the SKILL.md preview (and vice-versa).
      const [d, a] = await Promise.allSettled([
        getSkillMarketplaceDetail(id),
        getSkillMarketplaceAudit(id),
      ])
      if (cancelled) return
      if (d.status === 'fulfilled') {
        setDetail(d.value)
      } else {
        const message = 'skills.sh 暂不可用，请重试'
        setDetailErr(message)
        onError?.(message)
      }
      if (a.status === 'fulfilled') setAudit(a.value)
      // Audit failure is non-fatal: leave audit null → "未审计" badge.
      setLoading(false)
    }
    void run()
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, id])

  const risk = highestRisk(audit)
  const badge = RISK_BADGE[risk ?? 'NONE']

  // SKILL.md preview: prefer the SKILL.md file, else the first inline file.
  const skillMd = React.useMemo(() => {
    if (!detail) return null
    return detail.files.find((f) => f.path === 'SKILL.md') ?? detail.files[0] ?? null
  }, [detail])

  const doInstall = async (scope: 'global' | 'workspace') => {
    if (!id) return
    setInstallState({ kind: 'installing', scope })
    try {
      await installSkillFromMarketplace(
        id,
        scope,
        scope === 'workspace' ? activeWorkspaceId ?? undefined : undefined,
      )
      setInstallState({ kind: 'installed', scope })
    } catch (e) {
      setInstallState({ kind: 'error', message: String(e) })
    }
  }

  // HIGH-risk guard: clicking install first surfaces an inline confirm; the
  // pending scope is captured so 确认 installs to the right scope.
  const requestInstall = (scope: 'global' | 'workspace') => {
    if (risk === 'HIGH') {
      setPendingScope(scope)
      setConfirmingHighRisk(true)
      return
    }
    void doInstall(scope)
  }

  const busy = installState.kind === 'installing'
  const done = installState.kind === 'installed'

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="w-full sm:max-w-md flex flex-col gap-0 p-0">
        <SheetHeader className="px-6 pt-6 pb-4 border-b border-border text-left">
          <SheetTitle className="truncate pr-8">{detail?.id ?? id ?? '技能详情'}</SheetTitle>
          <div className="flex items-center gap-2 flex-wrap">
            {detail?.source && (
              <span className="text-xs text-muted-foreground truncate">{detail.source}</span>
            )}
            <span
              className={cn(
                'shrink-0 text-[10.5px] px-2 py-0.5 rounded-full border font-medium',
                badge.className,
              )}
            >
              {badge.label}
            </span>
          </div>
        </SheetHeader>

        <div className="flex-1 min-h-0 overflow-y-auto px-6 py-4">
          {loading ? (
            <div className="flex items-center gap-2 text-xs text-muted-foreground py-2">
              <Loader2 className="size-3.5 animate-spin shrink-0" />
              加载中…
            </div>
          ) : detailErr ? (
            <div className="text-xs text-red-400/90 bg-red-400/10 rounded-lg px-3 py-2">
              {detailErr}
            </div>
          ) : skillMd ? (
            <>
              <div className="mb-2 text-[10.5px] font-semibold uppercase tracking-wider text-muted-foreground/80">
                {skillMd.path}
              </div>
              <pre className="text-[11.5px] leading-relaxed text-foreground/85 whitespace-pre-wrap break-words bg-muted/30 rounded-lg p-3 max-h-[60vh] overflow-y-auto font-mono">
                {skillMd.contents.slice(0, 20000)}
              </pre>
            </>
          ) : (
            <div className="text-xs text-muted-foreground py-1">无预览内容</div>
          )}
        </div>

        <div className="shrink-0 border-t border-border px-6 py-4">
          {confirmingHighRisk ? (
            <div className="flex flex-col gap-2">
              <div className="text-xs text-red-400/90">此技能审计为高风险，仍要安装？</div>
              <div className="flex items-center gap-1.5">
                <button
                  type="button"
                  onClick={() => {
                    setConfirmingHighRisk(false)
                    if (pendingScope) void doInstall(pendingScope)
                    setPendingScope(null)
                  }}
                  className="text-xs px-2.5 py-1 rounded-lg bg-red-500/15 text-red-600 dark:text-red-400 hover:bg-red-500/25 transition-colors"
                >
                  确认
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setConfirmingHighRisk(false)
                    setPendingScope(null)
                  }}
                  className="text-xs px-2.5 py-1 rounded-lg bg-muted text-muted-foreground hover:bg-muted/70 transition-colors"
                >
                  取消
                </button>
              </div>
            </div>
          ) : done ? (
            <span className="text-xs px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-400">
              已安装（{installState.scope === 'global' ? '全局' : '本工作区'}）
            </span>
          ) : (
            <div className="flex items-center gap-1.5">
              <button
                type="button"
                onClick={() => requestInstall('global')}
                disabled={busy}
                className="text-xs px-2.5 py-1 rounded-lg bg-muted text-muted-foreground hover:bg-muted/70 transition-colors disabled:opacity-50"
              >
                {busy && installState.scope === 'global' ? '…' : '全局'}
              </button>
              <button
                type="button"
                onClick={() => requestInstall('workspace')}
                disabled={busy || !activeWorkspaceId}
                title={activeWorkspaceId ? '安装到当前工作区' : '无活动工作区'}
                className="text-xs px-2.5 py-1 rounded-lg bg-muted text-muted-foreground hover:bg-muted/70 transition-colors disabled:opacity-50"
              >
                {busy && installState.scope === 'workspace' ? '…' : '本工作区'}
              </button>
              {installState.kind === 'error' && (
                <span className="text-xs text-red-400/90 break-words">{installState.message}</span>
              )}
            </div>
          )}
        </div>
      </SheetContent>
    </Sheet>
  )
}
