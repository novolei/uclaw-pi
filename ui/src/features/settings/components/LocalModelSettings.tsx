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
import { Download, Loader2, CheckCircle2, X } from 'lucide-react'
import { SettingsCard, SettingsSection, SettingsRow, SettingsSegmentedControl } from './primitives'
import { Button } from '@/components/ui/button'
import type { LocalModelQuant } from '@/lib/bridge/settings'
import { useLocalModel } from '../hooks/useLocalModel'

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
      </SettingsCard>
    </SettingsSection>
  )
}
