// MemoryRecall: the 高级设置 collapsible (图扩展参数 / 时间衰减 / FTS 降级 / 用户档案).
// Extracted verbatim out of the 474-line components/settings/MemoryRecallSettings
// during the features/settings split (code-organization ADR 2026-05-31).
import { ChevronDown } from 'lucide-react'
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from '@/components/ui/collapsible'
import { SettingsSection } from '@/components/settings/primitives/SettingsSection'
import { SettingsCard } from '@/components/settings/primitives/SettingsCard'
import { SettingsRow } from '@/components/settings/primitives/SettingsRow'
import { SettingsSelect } from '@/components/settings/primitives/SettingsSelect'
import { NumberInput } from './NumberInput'
import { DEFAULTS, RANGES } from '../../lib/memory-recall'
import type { MemoryRecallCardProps } from './types'

export function AdvancedRecallCard({ config, updateField }: MemoryRecallCardProps): React.ReactElement {
  return (
    <Collapsible>
      <CollapsibleTrigger className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground hover:text-foreground cursor-pointer transition-colors duration-150 py-1">
        <ChevronDown className="size-3.5 transition-transform duration-200 [[data-state=open]>&]:rotate-180" />
        高级设置
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="mt-3 space-y-6">
          <SettingsSection title="图扩展参数" description="控制 L4 BFS 图扩展阶段的种子数和搜索深度。">
            <SettingsCard>
              <SettingsRow
                label="layer_expanded_seed_take"
                description={`图扩展种子数 · ${RANGES.layerExpandedSeedTake.min}–${RANGES.layerExpandedSeedTake.max} · 默认 ${DEFAULTS.layerExpandedSeedTake}`}
              >
                <NumberInput
                  value={config.layerExpandedSeedTake ?? DEFAULTS.layerExpandedSeedTake}
                  min={RANGES.layerExpandedSeedTake.min}
                  max={RANGES.layerExpandedSeedTake.max}
                  onChange={(v) => updateField('layerExpandedSeedTake', v)}
                />
              </SettingsRow>
              <SettingsRow
                label="layer_expanded_max_depth"
                description={`BFS 最大搜索深度 · ${RANGES.layerExpandedMaxDepth.min}–${RANGES.layerExpandedMaxDepth.max} · 默认 ${DEFAULTS.layerExpandedMaxDepth}`}
              >
                <SettingsSelect
                  value={String(config.layerExpandedMaxDepth ?? DEFAULTS.layerExpandedMaxDepth)}
                  onValueChange={(v) => updateField('layerExpandedMaxDepth', Number(v))}
                  options={[1, 2, 3, 4, 5].map((n) => ({ value: String(n), label: String(n) }))}
                />
              </SettingsRow>
            </SettingsCard>
          </SettingsSection>

          <SettingsSection title="时间衰减" description="记忆相关性随时间衰减的半衰期。较短的半衰期会更偏向近期记忆。">
            <SettingsCard>
              <SettingsRow
                label="time_decay_half_life_days"
                description={`半衰期天数 · ${RANGES.timeDecayHalfLifeDays.min}–${RANGES.timeDecayHalfLifeDays.max} · 默认 ${DEFAULTS.timeDecayHalfLifeDays}`}
              >
                <NumberInput
                  value={config.timeDecayHalfLifeDays ?? DEFAULTS.timeDecayHalfLifeDays}
                  min={RANGES.timeDecayHalfLifeDays.min}
                  max={RANGES.timeDecayHalfLifeDays.max}
                  step={0.5}
                  onChange={(v) => updateField('timeDecayHalfLifeDays', v)}
                />
              </SettingsRow>
            </SettingsCard>
          </SettingsSection>

          <SettingsSection title="FTS 降级" description="当 memU 向量引擎不可用时，全文搜索候选数量的倍增系数。">
            <SettingsCard>
              <SettingsRow
                label="fts_fallback_limit_multiplier"
                description={`倍增系数 · ${RANGES.ftsFallbackLimitMultiplier.min}–${RANGES.ftsFallbackLimitMultiplier.max} · 默认 ${DEFAULTS.ftsFallbackLimitMultiplier}`}
              >
                <NumberInput
                  value={config.ftsFallbackLimitMultiplier ?? DEFAULTS.ftsFallbackLimitMultiplier}
                  min={RANGES.ftsFallbackLimitMultiplier.min}
                  max={RANGES.ftsFallbackLimitMultiplier.max}
                  step={0.1}
                  onChange={(v) => updateField('ftsFallbackLimitMultiplier', v)}
                />
              </SettingsRow>
            </SettingsCard>
          </SettingsSection>

          <SettingsSection title="用户档案" description="控制自动挂载的 UserProfile 节点数量。0 为禁用。">
            <SettingsCard>
              <SettingsRow
                label="boot_user_profile_limit"
                description={`挂载数 · ${RANGES.bootUserProfileLimit.min}–${RANGES.bootUserProfileLimit.max} · 默认 ${DEFAULTS.bootUserProfileLimit}`}
              >
                <NumberInput
                  value={config.bootUserProfileLimit ?? DEFAULTS.bootUserProfileLimit}
                  min={RANGES.bootUserProfileLimit.min}
                  max={RANGES.bootUserProfileLimit.max}
                  onChange={(v) => updateField('bootUserProfileLimit', v)}
                />
              </SettingsRow>
            </SettingsCard>
          </SettingsSection>
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}
