// 恢复操作 — memu/gbrain restart + AI-engine reset + app restart buttons.
// Moved verbatim out of `legacy settings/SystemTab.tsx` during the P1
// split; side effects live in `useBridgeAction`. Never calls IPC directly.
import * as React from 'react'
import { RotateCcw, RefreshCw, Power } from 'lucide-react'
import { cn } from '@/lib/utils'
import { useBridgeAction } from '../../hooks/useBridgeAction'

export function ServicesCard({ onError }: { onError?: (m: string) => void }) {
  const { busy, run } = useBridgeAction(onError)
  return (
    <div className="flex flex-col gap-2">
      <p className="text-[10px] uppercase tracking-wider text-muted-foreground font-medium">恢复操作</p>
      <div className="flex flex-col gap-2">
        <div className="flex gap-2">
          <ActionButton
            icon={<RotateCcw size={13} />}
            label="重置 AI 引擎"
            busy={busy.reset}
            variant="warm"
            onClick={() => run('reset_ai_engine', 'reset')}
          />
          <ActionButton
            icon={<Power size={13} />}
            label="重启应用"
            busy={busy.restart}
            variant="danger"
            onClick={() => run('restart_app', 'restart')}
          />
        </div>
        <div className="flex gap-2">
          <ActionButton
            icon={<RotateCcw size={13} />}
            label="重启 memU"
            busy={busy.memu}
            variant="bridge"
            onClick={() => run('restart_memu_bridge', 'memu')}
          />
          <ActionButton
            icon={<RotateCcw size={13} />}
            label="重启 gbrain"
            busy={busy.gbrain}
            variant="bridge"
            onClick={() => run('restart_gbrain_mcp', 'gbrain')}
          />
        </div>
      </div>
    </div>
  )
}

function ActionButton({ icon, label, busy, variant, onClick }: {
  icon: React.ReactNode; label: string; busy: boolean
  variant: 'warm' | 'danger' | 'bridge'; onClick: () => void
}) {
  const cls = {
    warm: 'bg-amber-500/10 text-amber-400 hover:bg-amber-500/20 border border-amber-500/20',
    danger: 'bg-red-500/10 text-red-400 hover:bg-red-500/20 border border-red-500/20',
    bridge: 'bg-green-500/10 text-green-400 hover:bg-green-500/20 border border-green-500/20',
  }[variant]

  return (
    <button
      onClick={onClick}
      disabled={busy}
      className={cn(
        'flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-lg transition-colors disabled:opacity-50',
        cls,
      )}
    >
      {busy ? <RefreshCw size={12} className="animate-spin" /> : icon}
      {label}
    </button>
  )
}
