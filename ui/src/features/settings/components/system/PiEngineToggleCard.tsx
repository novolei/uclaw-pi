// Agent 引擎后端 toggle — switch between the legacy in-repo loop and the
// experimental PiEngine backend. Mirrors `HttpApiToggleCard`; side effects live
// in `usePiEngineToggle`. Never calls IPC directly.
import * as React from 'react'
import { cn } from '@/lib/utils'
import { usePiEngineToggle } from '../../hooks/usePiEngineToggle'

export function PiEngineToggleCard({ onError }: { onError?: (m: string) => void }) {
  const { enabled, toggle } = usePiEngineToggle(onError)
  return (
    <div className="rounded-xl border border-border px-4 py-3">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="text-sm font-medium text-foreground">
            Agent 引擎后端：pi <span className="text-amber-400/80">（实验）</span>
          </div>
          <p className="text-xs text-muted-foreground mt-0.5">
            开启后 Agent / 聊天走 pi 引擎；关闭则用稳定的 legacy 回路（默认）。pi 仍在迁移中（R4：MCP / 浏览器 / 技能工具尚未接入），仅供 dogfooding 试用。切换在<span className="text-foreground/70">下一条消息</span>生效，不影响进行中的回复——建议在新会话里验证。
          </p>
        </div>
        <button
          onClick={toggle}
          disabled={enabled === null}
          className={cn(
            'shrink-0 text-xs px-3 py-1.5 rounded-lg transition-colors disabled:opacity-50',
            enabled
              ? 'bg-amber-500/15 text-amber-400 hover:bg-amber-500/25'
              : 'bg-muted text-muted-foreground hover:bg-muted/70',
          )}
        >
          {enabled === null ? '…' : enabled ? 'pi 已开启' : 'legacy（默认）'}
        </button>
      </div>
    </div>
  )
}
