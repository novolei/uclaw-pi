// Permission rules — the V14 session + pattern rules (granular tier): the draft
// editor + the rules table. Presentational: receives the rules + draft state +
// add/delete callbacks from the PermissionsSettings shell's hook. Split out of
// legacy settings/PermissionsSettings.tsx during P3a; markup is byte-identical
// so behavior is preserved exactly.
import * as React from 'react'
import { Trash2, Plus } from 'lucide-react'
import type { CreatePermissionRuleInput, PermissionRule } from '@/lib/types'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

const MODE_BADGE: Record<string, { label: string; className: string }> = {
  allow: { label: '允许', className: 'bg-green-500/15 text-green-600 dark:text-green-400 border-green-500/30' },
  block: { label: '阻止', className: 'bg-red-500/15 text-red-600 dark:text-red-400 border-red-500/30' },
  ask:   { label: '询问', className: 'bg-yellow-500/15 text-yellow-600 dark:text-yellow-400 border-yellow-500/30' },
}

interface PermissionRulesSectionProps {
  rules: PermissionRule[]
  loading: boolean
  draft: CreatePermissionRuleInput
  setDraft: React.Dispatch<React.SetStateAction<CreatePermissionRuleInput>>
  onAddRule: () => void
  onDelete: (id: string) => void
}

export function PermissionRulesSection({
  rules,
  loading,
  draft,
  setDraft,
  onAddRule,
  onDelete,
}: PermissionRulesSectionProps): React.ReactElement {
  return (
    <section>
      <div className="mb-2.5 flex items-center justify-between">
        <h3 className="text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
          权限规则
        </h3>
      </div>

      {/* Rule editor */}
      <div className="mb-3 rounded-lg border border-border/50 bg-muted/20 p-3 space-y-2">
        <div className="grid grid-cols-12 gap-2 text-[12px]">
          <select
            value={draft.scope}
            onChange={(e) => setDraft((d) => ({ ...d, scope: e.target.value as 'session' | 'pattern' }))}
            className="col-span-2 bg-background border border-border/50 rounded px-2 py-1.5 outline-none"
          >
            <option value="pattern">模式</option>
            <option value="session">会话</option>
          </select>
          <input
            placeholder="tool_name (例如 bash)"
            value={draft.toolName}
            onChange={(e) => setDraft((d) => ({ ...d, toolName: e.target.value }))}
            className="col-span-3 bg-background border border-border/50 rounded px-2 py-1.5 outline-none font-mono"
          />
          <input
            placeholder={draft.scope === 'pattern' ? '目标前缀 (例如 git status)' : 'session_id'}
            value={draft.scope === 'pattern' ? (draft.target ?? '') : (draft.sessionId ?? '')}
            onChange={(e) => setDraft((d) => draft.scope === 'pattern'
              ? { ...d, target: e.target.value }
              : { ...d, sessionId: e.target.value })}
            className="col-span-4 bg-background border border-border/50 rounded px-2 py-1.5 outline-none font-mono"
          />
          <select
            value={draft.mode}
            onChange={(e) => setDraft((d) => ({ ...d, mode: e.target.value as 'allow' | 'block' | 'ask' }))}
            className="col-span-2 bg-background border border-border/50 rounded px-2 py-1.5 outline-none"
          >
            <option value="allow">允许</option>
            <option value="block">阻止</option>
            <option value="ask">询问</option>
          </select>
          <Button size="sm" onClick={onAddRule} className="col-span-1" disabled={!draft.toolName.trim()}>
            <Plus className="size-3.5" />
          </Button>
        </div>
      </div>

      {/* Rules table */}
      <div className="rounded-lg border border-border/50 bg-muted/20 max-h-64 overflow-y-auto">
        {rules.length === 0 ? (
          <div className="p-6 text-center text-[12px] text-muted-foreground/60">
            {loading ? '加载中…' : '暂无规则'}
          </div>
        ) : (
          <table className="w-full text-[12px]">
            <thead className="sticky top-0 bg-muted/60 backdrop-blur-sm">
              <tr className="text-left text-muted-foreground/70">
                <th className="px-3 py-2 font-normal">范围</th>
                <th className="px-3 py-2 font-normal">工具</th>
                <th className="px-3 py-2 font-normal">目标 / 会话</th>
                <th className="px-3 py-2 font-normal">模式</th>
                <th className="px-3 py-2 font-normal w-8" />
              </tr>
            </thead>
            <tbody>
              {rules.map((r) => {
                const mb = MODE_BADGE[r.mode] ?? MODE_BADGE.ask
                return (
                  <tr key={r.id} className="border-t border-border/30 hover:bg-muted/30">
                    <td className="px-3 py-1.5">{r.scope === 'session' ? '会话' : '模式'}</td>
                    <td className="px-3 py-1.5 font-mono">{r.toolName}</td>
                    <td className="px-3 py-1.5 font-mono text-muted-foreground/85 truncate max-w-[200px]">
                      {r.target ?? r.sessionId ?? ''}
                    </td>
                    <td className="px-3 py-1.5">
                      <span className={cn('inline-flex items-center rounded border px-1.5 py-0.5 text-[10.5px]', mb.className)}>
                        {mb.label}
                      </span>
                    </td>
                    <td className="px-3 py-1.5">
                      <Button size="sm" variant="ghost" onClick={() => void onDelete(r.id)} className="h-6 w-6 p-0">
                        <Trash2 className="size-3 text-muted-foreground/70" />
                      </Button>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        )}
      </div>
    </section>
  )
}
