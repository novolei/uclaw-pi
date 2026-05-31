import * as React from 'react'
import { Loader2, Sparkles } from 'lucide-react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { SettingsRow } from './primitives/SettingsRow'
import { SettingsToggle } from './primitives/SettingsToggle'
import type { PersonaPreset, VoiceProfile } from '@/lib/persona-types'
import { usePersonaStudio } from '../hooks/usePersonaStudio'

const SLIDERS: Array<{ key: keyof Omit<VoiceProfile, 'presetId' | 'neutralMode'>; label: string }> = [
  { key: 'warmth', label: '温度' },
  { key: 'directness', label: '直接度' },
  { key: 'challenge', label: '挑战度' },
  { key: 'playfulness', label: '趣味感' },
  { key: 'detail', label: '展开深度' },
  { key: 'initiative', label: '主动性' },
  { key: 'structure', label: '结构化' },
  { key: 'restraint', label: '克制感' },
]

export function PersonaStudio(): React.ReactElement {
  // Config load + optimistic voice write + saving flag live in the hook
  // (code-organization ADR 2026-05-31); the component stays presentation-only.
  const { config, saving, updateVoice } = usePersonaStudio()

  if (!config) {
    return (
      <SettingsSection title="Persona Studio">
        <SettingsCard>
          <div className="flex items-center gap-2 px-3 py-2 text-xs text-muted-foreground">
            <Loader2 className="size-3 animate-spin" />
            加载中…
          </div>
        </SettingsCard>
      </SettingsSection>
    )
  }

  return (
    <SettingsSection title="Persona Studio">
      <SettingsCard>
        <SettingsRow
          label="人格预设"
          description="选择起点后继续微调声音。"
          icon={<Sparkles size={15} className="text-muted-foreground" />}
        >
          <select
            value={config.voice.presetId}
            onChange={(event) => {
              const preset = config.presets.find((p) => p.id === event.target.value)
              if (preset) void updateVoice(preset.profile)
            }}
            className="h-8 min-w-28 rounded-md border border-border bg-background px-2 text-xs text-foreground"
          >
            {config.presets.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.label}
              </option>
            ))}
          </select>
        </SettingsRow>

        <SettingsToggle
          label="中性专业声音"
          description="临时关闭关系化表达。"
          checked={config.voice.neutralMode}
          onCheckedChange={(neutralMode) => void updateVoice({ ...config.voice, neutralMode })}
        />

        <div className="space-y-3 px-3 py-3">
          {SLIDERS.map((slider) => (
            <label
              key={slider.key}
              className="grid grid-cols-[80px_minmax(120px,1fr)_24px] items-center gap-3 text-xs"
            >
              <span className="text-muted-foreground">{slider.label}</span>
              <input
                aria-label={slider.label}
                type="range"
                min={0}
                max={5}
                value={config.voice[slider.key]}
                onChange={(event) => (
                  void updateVoice({ ...config.voice, [slider.key]: Number(event.target.value) })
                )}
                className="h-2 w-full accent-primary"
              />
              <span className="text-right text-muted-foreground">{config.voice[slider.key]}</span>
            </label>
          ))}
        </div>

        <PersonaPreview
          presets={config.presets}
          voice={config.voice}
          renderedPrompt={config.renderedPrompt}
          saving={saving}
        />
      </SettingsCard>
    </SettingsSection>
  )
}

function PersonaPreview({
  presets,
  voice,
  renderedPrompt,
  saving,
}: {
  presets: PersonaPreset[]
  voice: VoiceProfile
  renderedPrompt: string
  saving: boolean
}) {
  const preset = presets.find((p) => p.id === voice.presetId) ?? presets[0]
  return (
    <div className="space-y-3 border-t border-border/40 px-3 py-3">
      <div className="rounded-md bg-muted/40 p-3">
        <div className="text-xs font-medium text-foreground">{preset?.role}</div>
        <div className="mt-1 text-xs text-muted-foreground">{preset?.voice}</div>
        <div className="mt-3 text-xs text-muted-foreground">用户：{preset?.exampleUserPrompt}</div>
        <div className="mt-1 text-sm text-foreground">{preset?.exampleReply}</div>
      </div>
      <details open>
        <summary className="cursor-pointer text-xs text-muted-foreground">Persona Voice prompt</summary>
        <pre className="mt-2 max-h-48 overflow-auto rounded-md bg-muted/40 p-3 text-[11px] whitespace-pre-wrap">
          {renderedPrompt}
        </pre>
      </details>
      {saving && <div className="text-[11px] text-muted-foreground">保存中…</div>}
    </div>
  )
}
