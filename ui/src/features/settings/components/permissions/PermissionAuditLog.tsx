// Audit log — most-recent permission decisions across all sessions.
// Presentational: receives the audit entries + loading flag from the
// PermissionsSettings shell's hook. Split out of
// legacy settings/PermissionsSettings.tsx during P3a; markup is byte-identical
// so behavior is preserved exactly.
import * as React from 'react'
import type { PermissionAuditEntry } from '@/lib/types'
import { cn } from '@/lib/utils'

const DECISION_BADGE: Record<string, { label: string; className: string }> = {
  auto_approve:  { label: '自动允许', className: 'bg-green-500/10 text-green-600 dark:text-green-400' },
  user_approve:  { label: '用户允许', className: 'bg-blue-500/10 text-blue-600 dark:text-blue-400' },
  user_deny:     { label: '用户拒绝', className: 'bg-orange-500/10 text-orange-600 dark:text-orange-400' },
  blocked:       { label: '已阻止',   className: 'bg-red-500/10 text-red-600 dark:text-red-400' },
}

function formatTime(epochMs: number): string {
  const d = new Date(epochMs)
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

interface PermissionAuditLogProps {
  audit: PermissionAuditEntry[]
  loading: boolean
}

export function PermissionAuditLog({ audit, loading }: PermissionAuditLogProps): React.ReactElement {
  return (
    <section>
      <h3 className="mb-2.5 text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
        审计日志（最近 100 条）
      </h3>
      <div className="rounded-lg border border-border/50 bg-muted/20 max-h-72 overflow-y-auto">
        {audit.length === 0 ? (
          <div className="p-6 text-center text-[12px] text-muted-foreground/60">
            {loading ? '加载中…' : '暂无审计记录'}
          </div>
        ) : (
          <table className="w-full text-[12px]">
            <thead className="sticky top-0 bg-muted/60 backdrop-blur-sm">
              <tr className="text-left text-muted-foreground/70">
                <th className="px-3 py-2 font-normal">时间</th>
                <th className="px-3 py-2 font-normal">工具</th>
                <th className="px-3 py-2 font-normal">会话</th>
                <th className="px-3 py-2 font-normal">参数 hash</th>
                <th className="px-3 py-2 font-normal">决定</th>
              </tr>
            </thead>
            <tbody>
              {audit.map((a) => {
                const db = DECISION_BADGE[a.decision] ?? { label: a.decision, className: 'bg-muted text-muted-foreground' }
                return (
                  <tr key={a.id} className="border-t border-border/30 hover:bg-muted/30">
                    <td className="px-3 py-1.5 text-muted-foreground/70 tabular-nums">{formatTime(a.createdAt)}</td>
                    <td className="px-3 py-1.5 font-mono">{a.toolName}</td>
                    <td className="px-3 py-1.5 font-mono text-muted-foreground/70 truncate max-w-[100px]">
                      {a.sessionId.slice(0, 8)}
                    </td>
                    <td className="px-3 py-1.5 font-mono text-muted-foreground/70">{a.argsHash}</td>
                    <td className="px-3 py-1.5">
                      <span className={cn('inline-flex items-center rounded px-1.5 py-0.5 text-[10.5px]', db.className)}>
                        {db.label}
                      </span>
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
