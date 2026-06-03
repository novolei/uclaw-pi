/**
 * LocalModelStep — 首启引导中的「本地模型」可选步骤 (S3)
 *
 * 渲染环境自检清单（磁盘 / 内存 / Metal / 网络，每项 ✅/⚠️/❌ + 中文指引），
 * 一个下载进度条（复用 S2 的进度条 UI 形态），以及「下载并启用 / 跳过」按钮。
 * 编排逻辑全部来自 `useLocalModelSetup`（与 设置 → 本地模型 共享同一套状态机）。
 *
 * 规则：磁盘不足（diskOk===false）→ blocked，禁用下载；Metal/网络仅警告不阻断。
 */
import * as React from 'react'
import { Cpu, Loader2, CheckCircle2, AlertTriangle, XCircle, HardDrive, MemoryStick, Zap, Wifi } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { EnvReport } from '@/lib/bridge/settings'
import { useLocalModelSetup, type UseLocalModelSetup } from '@/features/settings/hooks/useLocalModelSetup'

const PHASE_LABEL: Record<string, string> = {
  probing: '测速中…',
  downloading: '下载中',
  verifying: '校验中…',
}

const SOURCE_LABEL: Record<string, string> = {
  modelscope: 'ModelScope',
  huggingface: 'HuggingFace',
}

function fmtGB(bytes: number): string {
  return `${(bytes / 1_073_741_824).toFixed(1)} GB`
}

type ItemStatus = 'ok' | 'warn' | 'fail'

interface ChecklistItem {
  icon: React.ReactNode
  label: string
  status: ItemStatus
  detail: string
}

/** Derive the four checklist rows from the env report. */
function buildChecklist(report: EnvReport): ChecklistItem[] {
  return [
    {
      icon: <HardDrive className="size-4" />,
      label: '磁盘空间',
      status: report.diskOk ? 'ok' : 'fail',
      detail: report.diskOk
        ? `可用 ${fmtGB(report.diskFreeBytes)}`
        : `空间不足：需要约 ${fmtGB(report.diskRequiredBytes)}，仅可用 ${fmtGB(report.diskFreeBytes)}`,
    },
    {
      icon: <MemoryStick className="size-4" />,
      label: '内存',
      status: report.ramOk ? 'ok' : 'warn',
      detail: report.ramOk
        ? `可用 ${fmtGB(report.ramAvailableBytes)}`
        : `内存偏低（可用 ${fmtGB(report.ramAvailableBytes)}），运行可能较慢`,
    },
    {
      icon: <Zap className="size-4" />,
      label: 'GPU 加速',
      status: report.metalAvailable ? 'ok' : 'warn',
      detail: report.metalAvailable ? '已启用 Metal 加速' : '未检测到 Metal，将用 CPU（较慢）',
    },
    {
      icon: <Wifi className="size-4" />,
      label: '网络',
      status: report.network.anyReachable ? 'ok' : 'warn',
      detail: report.network.anyReachable
        ? `可下载（${SOURCE_LABEL[report.network.fastest ?? ''] ?? report.network.fastest ?? '镜像可达'}）`
        : '暂未连通下载镜像，可稍后在设置中重试',
    },
  ]
}

function StatusIcon({ status }: { status: ItemStatus }): React.ReactElement {
  if (status === 'ok') return <CheckCircle2 className="size-4 text-emerald-500" />
  if (status === 'warn') return <AlertTriangle className="size-4 text-amber-500" />
  return <XCircle className="size-4 text-destructive" />
}

function Checklist({ report }: { report: EnvReport }): React.ReactElement {
  const items = buildChecklist(report)
  return (
    <div className="w-full max-w-sm space-y-2">
      {items.map((item) => (
        <div
          key={item.label}
          className="flex items-center gap-3 rounded-lg border border-border/60 bg-muted/30 px-3 py-2 text-left"
        >
          <span className="text-muted-foreground">{item.icon}</span>
          <div className="min-w-0 flex-1">
            <div className="text-sm font-medium text-foreground">{item.label}</div>
            <div className="truncate text-xs text-muted-foreground" title={item.detail}>
              {item.detail}
            </div>
          </div>
          <StatusIcon status={item.status} />
        </div>
      ))}
    </div>
  )
}

/** The S2-shaped download progress bar (phase label + mirror + percent). */
function ProgressBar({ progress }: { progress: NonNullable<UseLocalModelSetup['progress']> }): React.ReactElement {
  const determinate = progress.phase === 'downloading' && progress.total > 0
  return (
    <div className="flex w-full max-w-sm flex-col gap-1.5">
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin text-primary" />
        <span>{PHASE_LABEL[progress.phase] ?? progress.phase}</span>
        {progress.source && (
          <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
            {SOURCE_LABEL[progress.source] ?? progress.source}
          </span>
        )}
        {determinate && <span className="tabular-nums">{progress.percent}%</span>}
      </div>
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
        <div
          className="h-full rounded-full bg-primary transition-[width] duration-200"
          style={{ width: determinate ? `${progress.percent}%` : '100%' }}
          data-indeterminate={!determinate}
        />
      </div>
    </div>
  )
}

export interface LocalModelStepProps {
  /** Injected for testing; defaults to the real shared hook. */
  setup?: UseLocalModelSetup
  /** Called once the step reaches a terminal state (done or skipped). */
  onSettled?: (outcome: 'done' | 'skipped') => void
}

export function LocalModelStep({ setup: injected, onSettled }: LocalModelStepProps): React.ReactElement {
  const ownSetup = useLocalModelSetup()
  const setup = injected ?? ownSetup
  const { phase, report, progress, error, runChecks, downloadAndEnable, skip } = setup

  // Auto-run the preflight on first mount of the step.
  const started = React.useRef(false)
  React.useEffect(() => {
    if (!started.current && phase === 'intro') {
      started.current = true
      void runChecks()
    }
  }, [phase, runChecks])

  // Notify the host when we settle (done / skipped).
  React.useEffect(() => {
    if (phase === 'done') onSettled?.('done')
    else if (phase === 'skipped') onSettled?.('skipped')
  }, [phase, onSettled])

  const isBusy = phase === 'downloading' || phase === 'warming'

  return (
    <div className="flex flex-col items-center text-center gap-6 py-8">
      <div className="size-16 rounded-2xl bg-sky-500/10 flex items-center justify-center">
        <Cpu className="size-8 text-sky-500" />
      </div>
      <div>
        <h2 className="text-xl font-bold text-foreground mb-2">本地模型（可选）</h2>
        <p className="text-muted-foreground max-w-sm text-sm">
          在本机离线运行 MiniCPM5-1B，自动用于摘要等轻量后台任务。下载约几百 MB，可随时跳过。
        </p>
      </div>

      {(phase === 'checking' || phase === 'intro') && (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin text-primary" />
          正在检测运行环境…
        </div>
      )}

      {report && phase !== 'checking' && phase !== 'intro' && <Checklist report={report} />}

      {isBusy && progress && <ProgressBar progress={progress} />}
      {phase === 'warming' && (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="size-3.5 animate-spin text-primary" />
          正在加载并预热模型…
        </div>
      )}

      {phase === 'done' && (
        <div className="flex items-center gap-2 text-sm text-emerald-500">
          <CheckCircle2 className="size-4" />
          已下载并启用本地模型
        </div>
      )}

      {phase === 'blocked' && (
        <p className="max-w-sm text-sm text-destructive">磁盘空间不足，暂时无法下载。你可以清理空间后在「设置 → 本地模型」重试。</p>
      )}

      {error && phase === 'report' && (
        <p className="max-w-sm text-xs text-destructive" title={error}>
          {error}
        </p>
      )}

      {/* Actions: download (disabled when blocked) + skip. Hidden once terminal. */}
      {phase !== 'done' && phase !== 'skipped' && (
        <div className="flex items-center gap-3">
          <Button
            size="sm"
            onClick={() => void downloadAndEnable()}
            disabled={phase === 'blocked' || isBusy || phase === 'checking' || phase === 'intro'}
            className={cn(phase === 'blocked' && 'opacity-50')}
          >
            {error ? '重试下载' : '下载并启用'}
          </Button>
          <Button variant="ghost" size="sm" onClick={skip} disabled={isBusy}>
            跳过
          </Button>
        </div>
      )}
    </div>
  )
}
