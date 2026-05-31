// Owns the PersonaStudio config load + the optimistic voice-profile write + the
// saving flag. Extracted out of the component during the features/settings
// migration (code-organization ADR 2026-05-31). IPC stays in the typed
// `@/lib/persona` domain helpers (precedent: useChannelSettings keeps its typed
// helpers in the hook). The optimistic update + clamp-on-write + toast-on-error
// behavior is preserved verbatim. clampVoice/clampSlider are pure data transforms
// that belong with the write path, so they move here with it.
import * as React from 'react'
import { toast } from 'sonner'
import { getPersonaConfig, updatePersonaVoiceProfile } from '@/lib/persona'
import type { PersonaConfig, VoiceProfile } from '@/lib/persona-types'

export function usePersonaStudio() {
  const [config, setConfig] = React.useState<PersonaConfig | null>(null)
  const [saving, setSaving] = React.useState(false)

  React.useEffect(() => {
    getPersonaConfig()
      .then(setConfig)
      .catch((error) => {
        console.error('[PersonaStudio] load failed', error)
        toast.error('加载人格配置失败')
      })
  }, [])

  const updateVoice = React.useCallback(async (voice: VoiceProfile) => {
    const optimisticVoice = clampVoice(voice)
    setConfig((prev) => (prev ? { ...prev, voice: optimisticVoice } : prev))
    setSaving(true)
    try {
      const next = await updatePersonaVoiceProfile(optimisticVoice)
      setConfig(next)
    } catch (error) {
      console.error('[PersonaStudio] save failed', error)
      toast.error('保存人格配置失败')
    } finally {
      setSaving(false)
    }
  }, [])

  return { config, saving, updateVoice }
}

function clampVoice(voice: VoiceProfile): VoiceProfile {
  return {
    ...voice,
    warmth: clampSlider(voice.warmth),
    directness: clampSlider(voice.directness),
    challenge: clampSlider(voice.challenge),
    playfulness: clampSlider(voice.playfulness),
    detail: clampSlider(voice.detail),
    initiative: clampSlider(voice.initiative),
    structure: clampSlider(voice.structure),
    restraint: clampSlider(voice.restraint),
  }
}

function clampSlider(value: number): number {
  return Math.max(0, Math.min(5, Math.round(value)))
}
