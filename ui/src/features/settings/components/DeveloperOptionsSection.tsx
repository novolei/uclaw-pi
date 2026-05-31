import * as React from 'react'
import { Play, AlertTriangle, ChevronDown, ChevronUp } from 'lucide-react'
import {
  SETUP_SCRIPTS,
  SETUP_SCRIPT_DESCRIPTORS,
  type SetupScriptName,
} from '@/lib/embedding-endpoint'
import { useDeveloperOptions, type ScriptState } from '../hooks/useDeveloperOptions'

export function DeveloperOptionsSection(): React.ReactElement {
  const { expanded, setExpanded, states, forceConfirm, setForceConfirm, handleRun } =
    useDeveloperOptions()

  return (
    <div className="border border-border rounded-lg">
      <button
        onClick={() => setExpanded((v) => !v)}
        className="w-full flex items-center justify-between p-3 hover:bg-accent/30"
      >
        <div className="text-left">
          <h3 className="text-sm font-semibold flex items-center gap-2">
            开发者选项
            <span className="px-1.5 py-0.5 rounded bg-yellow-500/20 text-yellow-600 text-[9px] font-medium">DEV</span>
          </h3>
          <p className="text-[11px] text-muted-foreground mt-0.5">
            手动运行 setup 脚本。仅 dev 模式可用 — release 包不包含 scripts/。
          </p>
        </div>
        {expanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
      </button>

      {expanded && (
        <div className="border-t border-border p-3 space-y-3">
          {SETUP_SCRIPTS.map((name) => (
            <ScriptCard
              key={name}
              name={name}
              state={states[name]}
              onRun={(force) => {
                const desc = SETUP_SCRIPT_DESCRIPTORS[name]
                if (force && desc.supportsForce) {
                  setForceConfirm(name)
                } else {
                  handleRun(name, false)
                }
              }}
              confirmingForce={forceConfirm === name}
              onConfirmForce={() => handleRun(name, true)}
              onCancelForce={() => setForceConfirm(null)}
            />
          ))}
        </div>
      )}
    </div>
  )
}

interface ScriptCardProps {
  name: SetupScriptName
  state: ScriptState
  onRun: (force: boolean) => void
  confirmingForce: boolean
  onConfirmForce: () => void
  onCancelForce: () => void
}

function ScriptCard({ name, state, onRun, confirmingForce, onConfirmForce, onCancelForce }: ScriptCardProps): React.ReactElement {
  const desc = SETUP_SCRIPT_DESCRIPTORS[name]

  return (
    <div className="border border-border rounded-md p-3 space-y-2">
      <div className="flex items-start justify-between gap-3">
        <div className="flex-1 min-w-0">
          <p className="text-[12px] font-medium">{desc.label}</p>
          <p className="text-[10px] text-muted-foreground mt-0.5">{desc.description}</p>
          <code className="text-[10px] text-muted-foreground/70 font-mono">scripts/{name}.sh</code>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={() => onRun(false)}
            disabled={state.running}
            className="flex items-center gap-1 px-2 py-1 rounded text-[11px] bg-primary/10 text-primary hover:bg-primary/20 disabled:opacity-50"
          >
            <Play size={11} />
            {state.running ? '运行中' : '运行'}
          </button>
          {desc.supportsForce && (
            <button
              onClick={() => onRun(true)}
              disabled={state.running}
              className="flex items-center gap-1 px-2 py-1 rounded text-[11px] bg-red-500/10 text-red-500 hover:bg-red-500/20 disabled:opacity-50"
              title="--force"
            >
              <AlertTriangle size={11} />
              重置
            </button>
          )}
        </div>
      </div>

      {confirmingForce && (
        <div className="p-2 rounded bg-red-500/10 border border-red-500/30 space-y-2">
          <p className="text-[11px] text-red-600">
            <AlertTriangle size={11} className="inline mr-1 -mt-0.5" />
            <strong>--force 会清空数据。</strong> 确认执行吗？
          </p>
          <div className="flex gap-2">
            <button
              onClick={onConfirmForce}
              className="px-2 py-1 rounded text-[11px] bg-red-500 text-white hover:bg-red-600"
            >
              确认重置
            </button>
            <button
              onClick={onCancelForce}
              className="px-2 py-1 rounded text-[11px] bg-muted text-muted-foreground hover:bg-accent"
            >
              取消
            </button>
          </div>
        </div>
      )}

      {(state.running || state.progressPct > 0) && (
        <div className="space-y-1">
          <div className="h-1 rounded-full bg-muted overflow-hidden">
            <div
              className={[
                'h-full transition-all duration-300',
                state.error ? 'bg-red-500' : state.exitCode === 0 ? 'bg-green-500' : 'bg-primary',
              ].join(' ')}
              style={{ width: `${state.progressPct}%` }}
            />
          </div>
          <p className="text-[10px] text-muted-foreground">
            {state.running
              ? `运行中 ~${state.progressPct}% (估计耗时 ${desc.expectedDurationSecs}s)`
              : state.exitCode === 0
                ? '✓ 完成'
                : state.error
                  ? `✗ ${state.error}`
                  : ''}
          </p>
        </div>
      )}

      {state.log.length > 0 && (
        <details className="border-t border-border pt-2">
          <summary className="text-[10px] text-muted-foreground cursor-pointer hover:text-foreground">
            输出日志 ({state.log.length} 行)
          </summary>
          <pre className="mt-1 max-h-48 overflow-auto text-[10px] font-mono bg-muted/50 p-2 rounded whitespace-pre-wrap">
            {state.log.join('\n')}
          </pre>
        </details>
      )}
    </div>
  )
}
