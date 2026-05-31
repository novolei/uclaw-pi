// 本地 HTTP API 服务 toggle — the optional local HTTP API server switch.
// Moved verbatim out of `legacy settings/SystemTab.tsx` during the P1
// split; side effects live in `useHttpApiToggle`. Never calls IPC directly.
import * as React from 'react'
import { cn } from '@/lib/utils'
import { useHttpApiToggle } from '../../hooks/useHttpApiToggle'

export function HttpApiToggleCard({ onError }: { onError?: (m: string) => void }) {
  const { enabled, toggle } = useHttpApiToggle(onError)
  return (
    <div className="rounded-xl border border-border px-4 py-3">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="text-sm font-medium text-foreground">本地 HTTP API 服务</div>
          <p className="text-xs text-muted-foreground mt-0.5">
            可选的本地接口（记忆 / embedding / 外部访问，端口 19528）。聊天与 Agent 不依赖它，默认关闭。修改后需<span className="text-foreground/70">重启应用</span>生效。
          </p>
        </div>
        <button
          onClick={toggle}
          disabled={enabled === null}
          className={cn(
            'shrink-0 text-xs px-3 py-1.5 rounded-lg transition-colors disabled:opacity-50',
            enabled
              ? 'bg-green-500/15 text-green-400 hover:bg-green-500/25'
              : 'bg-muted text-muted-foreground hover:bg-muted/70',
          )}
        >
          {enabled === null ? '…' : enabled ? '已开启' : '已关闭'}
        </button>
      </div>
    </div>
  )
}
