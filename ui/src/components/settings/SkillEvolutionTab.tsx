/**
 * SkillEvolutionTab — side-by-side plain-text diff of a skill's version history.
 *
 * v1 uses plain side-by-side text; react-diff-view (added as a dep) is deferred
 * for a future polish pass once the tokenizer/refractor setup is in place.
 *
 * Shows:
 *   - Left aside: version list (newest first) with status badge + date
 *   - Right body: active version content on the left column, most-recent
 *     superseded version content on the right column
 */

import * as React from 'react'
import { getSkillVersions, type SkillVersionInfo } from '@/lib/tauri-bridge'
import { cn } from '@/lib/utils'

interface Props {
  skillId: string
}

function formatDate(s: string): string {
  if (!s) return ''
  const d = new Date(s)
  if (isNaN(d.getTime())) return s
  return `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

function StatusBadge({ status }: { status: string }): React.ReactElement {
  const active = status === 'active'
  return (
    <span
      className={cn(
        'inline-block rounded px-1.5 py-0.5 text-[10px] font-medium tabular-nums',
        active
          ? 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400'
          : 'bg-muted text-muted-foreground',
      )}
    >
      {active ? '当前' : '历史'}
    </span>
  )
}

export function SkillEvolutionTab({ skillId }: Props): React.ReactElement {
  const [versions, setVersions] = React.useState<SkillVersionInfo[]>([])
  const [loading, setLoading] = React.useState(true)
  const [selectedId, setSelectedId] = React.useState<string | null>(null)

  React.useEffect(() => {
    let cancelled = false
    setLoading(true)
    getSkillVersions(skillId).then((v) => {
      if (cancelled) return
      setVersions(v)
      // Default: show active vs previous
      const first = v[0]
      if (first) setSelectedId(first.id)
      setLoading(false)
    })
    return () => { cancelled = true }
  }, [skillId])

  if (loading) {
    return (
      <div className="flex items-center justify-center py-8 text-[12px] text-muted-foreground/70">
        加载中…
      </div>
    )
  }

  if (versions.length === 0) {
    return (
      <div className="rounded-lg border border-dashed border-border/50 bg-muted/10 p-6 text-center text-[12px] text-muted-foreground/70">
        尚无版本记录
      </div>
    )
  }

  const active = versions.find((v) => v.status === 'active') ?? versions[0]
  // The "previous" to compare against is the first non-active version,
  // unless the user has explicitly selected one.
  const compareTarget = selectedId && selectedId !== active.id
    ? versions.find((v) => v.id === selectedId) ?? null
    : versions.find((v) => v.id !== active.id) ?? null

  return (
    <div className="flex gap-3 text-[12px]">
      {/* Version list aside */}
      <aside className="w-36 flex-shrink-0 space-y-1">
        <div className="mb-1.5 text-[10.5px] font-semibold uppercase tracking-wider text-muted-foreground/70">
          版本列表
        </div>
        {versions.map((v, i) => (
          <button
            key={v.id}
            type="button"
            onClick={() => setSelectedId(v.id)}
            className={cn(
              'w-full rounded-md border px-2.5 py-2 text-left transition-colors',
              selectedId === v.id
                ? 'border-border bg-muted/60'
                : 'border-transparent hover:bg-muted/30',
            )}
          >
            <div className="flex items-center justify-between gap-1 mb-0.5">
              <StatusBadge status={v.status} />
              <span className="text-[10px] text-muted-foreground/60 tabular-nums">v{i + 1}</span>
            </div>
            <div className="text-[10.5px] text-muted-foreground/80 leading-tight">
              {formatDate(v.createdAt)}
            </div>
          </button>
        ))}
      </aside>

      {/* Side-by-side diff body */}
      <div className="flex-1 min-w-0 grid grid-cols-2 gap-2">
        <DiffPane label="当前版本" content={active.content} />
        <DiffPane
          label={compareTarget ? `对比版本 (${formatDate(compareTarget.createdAt)})` : '无历史版本可对比'}
          content={compareTarget?.content ?? null}
        />
      </div>
    </div>
  )
}

function DiffPane({ label, content }: { label: string; content: string | null }): React.ReactElement {
  return (
    <div className="min-w-0 rounded-md border border-border/40 bg-muted/10">
      <div className="border-b border-border/40 px-3 py-1.5 text-[10.5px] font-medium text-muted-foreground/80">
        {label}
      </div>
      {content != null ? (
        <pre className="overflow-auto p-3 text-[11.5px] leading-relaxed text-foreground/90 whitespace-pre-wrap break-words">
          {content}
        </pre>
      ) : (
        <div className="p-3 text-[11.5px] text-muted-foreground/60 italic">无内容</div>
      )}
    </div>
  )
}
