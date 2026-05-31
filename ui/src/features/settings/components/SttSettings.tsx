/**
 * SttSettings — settings page for the STT feature.
 *
 * Reuses settings primitives. Model-status section mirrors FirstRunDialog's
 * download UI so visuals stay consistent.
 */
import * as React from 'react'
import { useAtom } from 'jotai'
import { Download, Loader2, CheckCircle2 } from 'lucide-react'
import {
  SettingsCard,
  SettingsSection,
  SettingsRow,
  SettingsSelect,
} from '@/components/settings/primitives'
import { LABEL_CLASS } from '@/components/settings/primitives/SettingsUIConstants'
import { Button } from '@/components/ui/button'
import { sttSettingsAtom, type Language } from '@/atoms/stt-atoms'
import { getShortcutForPlatform } from '@/lib/shortcut-defaults'
import { useSttModel } from '../hooks/useSttModel'

const LANGUAGE_OPTIONS: Array<{ value: Language; label: string }> = [
  { value: 'auto', label: '自动' },
  { value: 'zh', label: '中文' },
  { value: 'en', label: '英文' },
  { value: 'yue', label: '粤语' },
  { value: 'ja', label: '日文' },
  { value: 'ko', label: '韩文' },
]

const SILENCE_OPTIONS: Array<{ value: string; label: string }> = [
  { value: '1200', label: '1.2 秒（灵敏）' },
  { value: '1800', label: '1.8 秒（默认）' },
  { value: '2400', label: '2.4 秒（宽松）' },
  { value: '3000', label: '3.0 秒（很宽松）' },
]

export function SttSettings(): React.ReactElement {
  const { modelStatus, devices, handleDownload } = useSttModel()
  const [settings, setSettings] = useAtom(sttSettingsAtom)

  // 兜底：旧 localStorage 值可能缺 silenceThresholdMs。
  const silenceThresholdMs = settings.silenceThresholdMs ?? 1800

  const shortcut = getShortcutForPlatform('toggle-stt-recording') ?? 'Cmd+Shift+M'

  return (
    <div className="space-y-4">
      <SettingsSection title="模型">
        <SettingsCard>
          <SettingsRow label="状态">
            <div className="flex items-center gap-2">
              {modelStatus.kind === 'ready' && (
                <>
                  <CheckCircle2 className="size-4 text-primary" />
                  <span className="text-sm text-foreground">已就绪</span>
                  <span className="text-xs text-muted-foreground">
                    {modelStatus.modelDir}
                  </span>
                </>
              )}
              {modelStatus.kind === 'not-downloaded' && (
                <>
                  <span className="text-sm text-muted-foreground">未下载</span>
                  <Button size="sm" onClick={handleDownload}>
                    <Download className="size-3 mr-1" />
                    下载（~230MB）
                  </Button>
                </>
              )}
              {modelStatus.kind === 'downloading' && (
                <>
                  <Loader2 className="size-4 animate-spin text-primary" />
                  <span className="text-sm text-muted-foreground">
                    {modelStatus.percent}% · {modelStatus.file}
                  </span>
                </>
              )}
              {modelStatus.kind === 'error' && (
                <>
                  <span className="text-sm text-destructive">{modelStatus.message}</span>
                  <Button size="sm" variant="outline" onClick={handleDownload}>
                    重试
                  </Button>
                </>
              )}
              {modelStatus.kind === 'unknown' && (
                <span className="text-sm text-muted-foreground">检测中…</span>
              )}
            </div>
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      <SettingsSection title="转写">
        <SettingsCard>
          <SettingsRow label="默认语言">
            <SettingsSelect
              value={settings.language}
              onValueChange={(v: string) =>
                setSettings({ ...settings, language: v as Language })
              }
              options={LANGUAGE_OPTIONS}
            />
          </SettingsRow>
          <SettingsRow label="麦克风设备">
            <SettingsSelect
              value={settings.microphoneDeviceId ?? '__default__'}
              onValueChange={(v: string) =>
                setSettings({
                  ...settings,
                  microphoneDeviceId: v === '__default__' ? null : v,
                })
              }
              options={[
                { value: '__default__', label: '系统默认' },
                ...devices
                  .filter((d) => d.deviceId !== '')
                  .map((d) => ({
                    value: d.deviceId,
                    label: d.label || `Mic ${d.deviceId.slice(0, 8)}`,
                  })),
              ]}
            />
          </SettingsRow>
          <SettingsRow label="静音多久后自动录入">
            <SettingsSelect
              value={String(silenceThresholdMs)}
              onValueChange={(v: string) =>
                setSettings({ ...settings, silenceThresholdMs: Number(v) })
              }
              options={SILENCE_OPTIONS}
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      <SettingsSection title="快捷键">
        <SettingsCard>
          <SettingsRow label="语音输入开/关">
            <span className={LABEL_CLASS + ' text-muted-foreground font-mono'}>
              {shortcut}
            </span>
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>
    </div>
  )
}
