/**
 * Settings panel: enable desktop pet + choose character.
 * Writes to atomWithStorage atoms (petEnabledAtom / petCharacterAtom);
 * PetWidget reacts immediately via Jotai subscription.
 */
import { useAtom } from 'jotai'
import { cn } from '@/lib/utils'
import { petCharacterAtom, petEnabledAtom, type PetCharacter } from '@/atoms/pet-atoms'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { SettingsToggle } from './primitives/SettingsToggle'
import { LABEL_CLASS, DESCRIPTION_CLASS, ROW_CLASS } from './primitives/SettingsUIConstants'

const CHARACTERS: Array<{ value: PetCharacter; label: string; description: string }> = [
  { value: 'astro', label: '小宇 Astro', description: '3D 磨砂塑料宇航小子' },
  { value: 'clawby', label: '爪宝 Clawby', description: 'Tom & Jerry 风浣熊宝宝' },
]

export function PetSettings() {
  const [enabled, setEnabled] = useAtom(petEnabledAtom)
  const [character, setCharacter] = useAtom(petCharacterAtom)

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
    </div>
  )
}
