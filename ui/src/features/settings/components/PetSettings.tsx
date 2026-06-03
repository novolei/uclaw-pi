/**
 * Settings panel: enable desktop pet + choose character.
 * Writes to atomWithStorage atoms (petEnabledAtom / petCharacterAtom);
 * PetWidget reacts immediately via Jotai subscription.
 */
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { useEffect, useState } from 'react'
import { cn } from '@/lib/utils'
import {
  petCharacterAtom,
  petEnabledAtom,
  petPersonaIdAtom,
  petPersonasAtom,
  type PetCharacter,
} from '@/atoms/pet-atoms'
import {
  hideDeskPet,
  listPetPersonas,
  setPetPersona,
  showDeskPet,
} from '@/lib/bridge/pet'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { SettingsToggle } from './primitives/SettingsToggle'
import { SettingsSelect } from './primitives/SettingsSelect'
import { LABEL_CLASS, DESCRIPTION_CLASS, ROW_CLASS } from './primitives/SettingsUIConstants'

const CHARACTERS: Array<{ value: PetCharacter; label: string; description: string }> = [
  { value: 'astro', label: '小宇 Astro', description: '3D 磨砂塑料宇航小子' },
  { value: 'clawby', label: '爪宝 Clawby', description: 'Tom & Jerry 风浣熊宝宝' },
]

export function PetSettings() {
  const [enabled, setEnabled] = useAtom(petEnabledAtom)
  const [character, setCharacter] = useAtom(petCharacterAtom)

  // S4 desk-pet companion: persona roster + active selection + window toggle.
  const [personas, setPersonas] = useAtom(petPersonasAtom)
  const [personaId, setPersonaId] = useAtom(petPersonaIdAtom)
  const setCharacterAtom = useSetAtom(petCharacterAtom)
  const personasValue = useAtomValue(petPersonasAtom)
  const [deskPetOn, setDeskPetOn] = useState(false)

  // Load the persona roster once for the dropdown (system-prompt-only personas;
  // no adapter import). Silently no-op if the backend / Tauri isn't available.
  useEffect(() => {
    let alive = true
    listPetPersonas()
      .then((list) => {
        if (alive) setPersonas(list)
      })
      .catch((e) => console.warn('[PetSettings] list_pet_personas failed:', e))
    return () => {
      alive = false
    }
  }, [setPersonas])

  const onPersonaChange = (id: string) => {
    setPersonaId(id)
    // Keep the rendered sprite in sync with the persona's character.
    const persona = personasValue.find((p) => p.id === id)
    if (persona && (persona.character === 'astro' || persona.character === 'clawby')) {
      setCharacterAtom(persona.character)
    }
    setPetPersona(id).catch((e) => console.warn('[PetSettings] set_pet_persona failed:', e))
  }

  const onDeskPetToggle = (next: boolean) => {
    setDeskPetOn(next)
    const call = next ? showDeskPet() : hideDeskPet()
    call.catch((e) => console.warn('[PetSettings] desk-pet toggle failed:', e))
  }

  return (
    <div className="space-y-6">
      <SettingsSection
        title="桌面宠物"
        description="在 Agent 输入框右上角显示一个可爱的 AI 伙伴。"
      >
        <SettingsCard>
          <SettingsToggle
            label="启用桌面宠物"
            description="开启后宠物出现在 Agent 视图右上角"
            checked={enabled}
            onCheckedChange={setEnabled}
          />
        </SettingsCard>
      </SettingsSection>

      {enabled && (
        <SettingsSection title="选择角色">
          <SettingsCard divided={false}>
            <div role="radiogroup" aria-label="选择宠物角色" className="divide-y divide-border/50">
              {CHARACTERS.map((c, index) => (
                <button
                  key={c.value}
                  type="button"
                  role="radio"
                  aria-checked={character === c.value}
                  onClick={() => setCharacter(c.value)}
                  className={cn(
                    ROW_CLASS,
                    'w-full text-left transition-colors',
                    index === 0 ? 'rounded-t-xl' : '',
                    index === CHARACTERS.length - 1 ? 'rounded-b-xl' : '',
                    character === c.value
                      ? 'bg-primary/8'
                      : 'hover:bg-muted/40',
                  )}
                >
                  {/* Text */}
                  <div className="flex-1 min-w-0 mr-4">
                    <div className={LABEL_CLASS}>{c.label}</div>
                    <div className={cn(DESCRIPTION_CLASS, 'mt-0.5')}>{c.description}</div>
                  </div>

                  {/* Character preview */}
                  <img
                    src={`/pet/${c.value}-idle.webp`}
                    alt=""
                    aria-hidden="true"
                    className="size-12 object-contain flex-shrink-0"
                  />

                  {/* Selection indicator */}
                  <div
                    className={cn(
                      'ml-3 size-4 rounded-full border-2 flex-shrink-0 transition-colors',
                      character === c.value
                        ? 'border-primary bg-primary'
                        : 'border-border bg-transparent',
                    )}
                  />
                </button>
              ))}
            </div>
          </SettingsCard>
        </SettingsSection>
      )}

      <SettingsSection
        title="桌面伙伴"
        description="把伙伴放到桌面上：一个常驻置顶的透明小窗口，可与本地 MiniCPM 流式对话。"
      >
        <SettingsCard>
          <SettingsToggle
            label="启用桌面伙伴"
            description="开启后弹出可拖动、可点击穿透的桌面伙伴窗口"
            checked={deskPetOn}
            onCheckedChange={onDeskPetToggle}
          />
        </SettingsCard>

        {personas.length > 0 && (
          <SettingsCard>
            <div className={ROW_CLASS}>
              <div className="flex-1 min-w-0 mr-4">
                <div className={LABEL_CLASS}>性格</div>
                <div className={cn(DESCRIPTION_CLASS, 'mt-0.5')}>
                  选择伙伴的说话风格（仅系统提示词，无需导入适配器）
                </div>
              </div>
              <div className="w-44 flex-shrink-0">
                <SettingsSelect
                  value={personaId}
                  onValueChange={onPersonaChange}
                  options={personas.map((p) => ({ value: p.id, label: p.displayName }))}
                  placeholder="选择性格"
                />
              </div>
            </div>
          </SettingsCard>
        )}
      </SettingsSection>
    </div>
  )
}
