/**
 * LocalModelSettings — Intelligence-tab section for the local MiniCPM model (S2).
 *
 * Renders an advanced quant selector (Q4_K_M default / Q8_0 / F16) that persists
 * the choice, and a source-aware download progress bar (mirror label + phase +
 * cancel button) wired to the smart-download commands + the
 * `local-model:download-progress` event. Follows the SttSettings download-UI
 * pattern + the shared settings primitives.
 */
import * as React from 'react'
import { Download, Loader2, CheckCircle2, X, ListChecks, AlertTriangle, XCircle } from 'lucide-react'
import { SettingsCard, SettingsSection, SettingsRow, SettingsSegmentedControl } from './primitives'
import { Button } from '@/components/ui/button'
import type { EnvReport, LocalModelQuant } from '@/lib/bridge/settings'
import { useLocalModel } from '../hooks/useLocalModel'
import { useLocalModelSetup } from '../hooks/useLocalModelSetup'

/** Quant options with approximate on-disk sizes (from repo LFS metadata). */
const QUANT_OPTIONS: Array<{ value: LocalModelQuant; label: string; size: string }> = [
  { value: 'q4_k_m', label: 'Q4_K_M', size: '~688 MB' },
  { value: 'q8_0', label: 'Q8_0', size: '~1.15 GB' },
  { value: 'f16', label: 'F16', size: '~2.17 GB' },
]

const SOURCE_LABEL: Record<string, string> = {
  modelscope: 'ModelScope',
  huggingface: 'HuggingFace',
}

const PHASE_LABEL: Record<string, string> = {
  probing: '测速中…',
  downloading: '下载中',
  verifying: '校验中…',
}

function fmtMB(bytes: number): string {
  return `${(bytes / 1_048_576).toFixed(0)} MB`
}

function fmtGB(bytes: number): string {
  return `${(bytes / 1_073_741_824).toFixed(1)} GB`
}

/** One env-report line for the Settings re-entry checklist. */
function CheckRow({ ok, warn, label, detail }: { ok: boolean; warn?: boolean; label: string; detail: string }): React.ReactElement {
  return (
    <div className="flex items-center gap-2 text-xs">
      {ok ? (
        <CheckCircle2 className="size-3.5 shrink-0 text-emerald-500" />
      ) : warn ? (
        <AlertTriangle className="size-3.5 shrink-0 text-amber-500" />
      ) : (
        <XCircle className="size-3.5 shrink-0 text-destructive" />
      )}
      <span className="text-foreground">{label}</span>
      <span className="truncate text-muted-foreground" title={detail}>
        {detail}
      </span>
    </div>
  )
}

function EnvChecklist({ report }: { report: EnvReport }): React.ReactElement {
  return (
    <div className="flex w-full max-w-[360px] flex-col gap-1.5">
      <CheckRow
        ok={report.diskOk}
        label="磁盘"
        detail={report.diskOk ? `可用 ${fmtGB(report.diskFreeBytes)}` : `需要约 ${fmtGB(report.diskRequiredBytes)}`}
      />
      <CheckRow ok={report.ramOk} warn label="内存" detail={`可用 ${fmtGB(report.ramAvailableBytes)}`} />
      <CheckRow
        ok={report.metalAvailable}
        warn
        label="GPU"
        detail={report.metalAvailable ? 'Metal 加速' : '未检测到 Metal，将用 CPU（较慢）'}
      />
      <CheckRow
        ok={report.network.anyReachable}
        warn
        label="网络"
        detail={report.network.anyReachable ? '镜像可达' : '暂未连通镜像'}
      />
    </div>
  )
}

/**
 * Settings re-entry for the first-launch local-model setup (S3). Reuses the
 * shared `useLocalModelSetup` orchestration so Settings and onboarding run the
 * exact same check → download → warmup → assign flow.
 */
function LocalModelSetupReentry(): React.ReactElement {
  const setup = useLocalModelSetup()
  const { phase, report, progress, error, runChecks, downloadAndEnable } = setup
  const busy = phase === 'checking' || phase === 'downloading' || phase === 'warming'

  return (
    <SettingsRow label="首启检查" description="检测运行环境并一键下载 / 启用本地模型，自动用于摘要等后台角色。">
      <div className="flex w-full max-w-[360px] flex-col gap-2">
        {report && phase !== 'checking' && <EnvChecklist report={report} />}

        {phase === 'warming' && (
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin text-primary" />
            正在加载并预热模型…
          </div>
        )}
        {phase === 'downloading' && progress && (
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin text-primary" />
            <span>{PHASE_LABEL[progress.phase] ?? progress.phase}</span>
            {progress.phase === 'downloading' && progress.total > 0 && (
              <span className="tabular-nums">{progress.percent}%</span>
            )}
          </div>
        )}
        {phase === 'done' && (
          <div className="flex items-center gap-2 text-xs text-emerald-500">
            <CheckCircle2 className="size-3.5" />
            已下载并启用
          </div>
        )}
        {phase === 'blocked' && (
          <span className="text-xs text-destructive">磁盘空间不足，无法下载。</span>
        )}
        {error && phase === 'report' && (
          <span className="truncate text-xs text-destructive" title={error}>
            {error}
          </span>
        )}

        <div className="flex items-center gap-2">
          {phase === 'intro' || phase === 'skipped' ? (
            <Button size="sm" variant="outline" onClick={() => void runChecks()} disabled={busy}>
              <ListChecks className="mr-1 size-3" />
              运行首启检查
            </Button>
          ) : (
            <Button
              size="sm"
              onClick={() => void downloadAndEnable()}
              disabled={busy || phase === 'blocked' || phase === 'done'}
            >
              <Download className="mr-1 size-3" />
              {error ? '重试下载' : phase === 'done' ? '已启用' : '下载并启用'}
            </Button>
          )}
        </div>
      </div>
    </SettingsRow>
  )
}

export function LocalModelSettings(): React.ReactElement {
  const { quant, status, handleDownload, handleCancel, handleQuantChange } = useLocalModel()
  const selected = QUANT_OPTIONS.find((o) => o.value === quant) ?? QUANT_OPTIONS[0]
  const isDownloading = status.kind === 'downloading'

  return (
    <SettingsSection
      title="本地模型（MiniCPM）"
      description="在本机离线运行的 MiniCPM5-1B。首次使用需下载 GGUF 权重；将自动选择更快的镜像（ModelScope / HuggingFace）。"
    >
      <SettingsCard>
        {/* Advanced quant selector */}
        <SettingsRow label="量化精度" description={`权重大小约 ${selected.size}`}>
          <SettingsSegmentedControl
            value={quant}
            onValueChange={(v) => handleQuantChange(v as LocalModelQuant)}
            options={QUANT_OPTIONS.map((o) => ({ value: o.value, label: o.label }))}
          />
        </SettingsRow>

        {/* Status / download */}
        <SettingsRow label="状态">
          <div className="flex w-full max-w-[360px] items-center gap-2">
            {status.kind === 'ready' && (
              <>
                <CheckCircle2 className="size-4 text-primary" />
                <span className="text-sm text-foreground">已就绪</span>
              </>
            )}

            {status.kind === 'not-downloaded' && (
              <Button size="sm" onClick={handleDownload}>
                <Download className="mr-1 size-3" />
                下载（{selected.size}）
              </Button>
            )}

            {status.kind === 'unknown' && (
              <span className="text-sm text-muted-foreground">检测中…</span>
            )}

            {status.kind === 'error' && (
              <>
                <span className="truncate text-sm text-destructive" title={status.message}>
                  {status.message}
                </span>
                <Button size="sm" variant="outline" onClick={handleDownload}>
                  重试
                </Button>
              </>
            )}

            {isDownloading && (
              <div className="flex w-full flex-col gap-1.5">
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <Loader2 className="size-3.5 animate-spin text-primary" />
                  <span>{PHASE_LABEL[status.phase] ?? status.phase}</span>
                  {status.source && (
                    <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                      {SOURCE_LABEL[status.source] ?? status.source}
                    </span>
                  )}
                  {status.phase === 'downloading' && status.total > 0 && (
                    <span className="tabular-nums">
                      {fmtMB(status.downloaded)} / {fmtMB(status.total)} · {status.percent}%
                    </span>
                  )}
                  <button
                    type="button"
                    onClick={handleCancel}
                    className="ml-auto flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[11px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                    aria-label="取消下载"
                  >
                    <X className="size-3" />
                    取消
                  </button>
                </div>
                {/* Progress bar */}
                <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                  <div
                    className="h-full rounded-full bg-primary transition-[width] duration-200"
                    style={{
                      width:
                        status.phase === 'downloading' && status.total > 0
                          ? `${status.percent}%`
                          : '100%',
                    }}
                    data-indeterminate={status.phase !== 'downloading' || status.total === 0}
                  />
                </div>
              </div>
            )}
          </div>
        </SettingsRow>

        {/* S3 first-launch re-entry: shares useLocalModelSetup with onboarding. */}
        <LocalModelSetupReentry />
      </SettingsCard>
    </SettingsSection>
  )
}
