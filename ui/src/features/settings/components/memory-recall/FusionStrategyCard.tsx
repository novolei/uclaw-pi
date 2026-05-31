// MemoryRecall: 融合策略 + 技能挂载 sections. Extracted verbatim out of the 474-line
// legacy settings/MemoryRecallSettings during the features/settings split
// (code-organization ADR 2026-05-31).
import { SettingsSection } from '../primitives/SettingsSection'
import { SettingsCard } from '../primitives/SettingsCard'
import { SettingsRow } from '../primitives/SettingsRow'
import { SettingsSelect } from '../primitives/SettingsSelect'
import { NumberInput } from './NumberInput'
import { DEFAULTS, FUSION_OPTIONS, RANGES } from '../../lib/memory-recall'
import type { MemoryRecallCardProps } from './types'

export function FusionStrategyCard({ config, updateField }: MemoryRecallCardProps): React.ReactElement {
  return (
    <>
      {/* 融合策略 */}
      <SettingsSection
        title="融合策略"
        description="控制全文搜索和向量搜索结果的融合方式。RRF 使用倒数排名融合，Weighted 使用加权分数。"
      >
        <SettingsCard>
          <SettingsRow label="fusion_strategy" description="融合算法选择">
            <SettingsSelect
              value={config.fusionStrategy ?? DEFAULTS.fusionStrategy}
              onValueChange={(v) => updateField('fusionStrategy', v as 'rrf' | 'weighted')}
              options={FUSION_OPTIONS}
            />
          </SettingsRow>
          <SettingsRow
            label="rrf_k"
            description={`RRF 平滑参数 · ${RANGES.rrfK.min}–${RANGES.rrfK.max} · 默认 ${DEFAULTS.rrfK}（仅 RRF 模式生效）`}
          >
            <NumberInput
              value={config.rrfK ?? DEFAULTS.rrfK}
              min={RANGES.rrfK.min}
              max={RANGES.rrfK.max}
              onChange={(v) => updateField('rrfK', v)}
            />
          </SettingsRow>
          <SettingsRow
            label="fts_weight"
            description={`全文搜索权重 · ${RANGES.ftsWeight.min}–${RANGES.ftsWeight.max} · 默认 ${DEFAULTS.ftsWeight}（仅 Weighted 模式生效）`}
          >
            <NumberInput
              value={config.ftsWeight ?? DEFAULTS.ftsWeight}
              min={RANGES.ftsWeight.min}
              max={RANGES.ftsWeight.max}
              step={0.1}
              onChange={(v) => updateField('ftsWeight', v)}
            />
          </SettingsRow>
          <SettingsRow
            label="vector_weight"
            description={`向量搜索权重 · ${RANGES.vectorWeight.min}–${RANGES.vectorWeight.max} · 默认 ${DEFAULTS.vectorWeight}（仅 Weighted 模式生效）`}
          >
            <NumberInput
              value={config.vectorWeight ?? DEFAULTS.vectorWeight}
              min={RANGES.vectorWeight.min}
              max={RANGES.vectorWeight.max}
              step={0.1}
              onChange={(v) => updateField('vectorWeight', v)}
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      {/* 技能挂载 */}
      <SettingsSection
        title="技能挂载"
        description="控制每轮 Agent 对话中自动注入的已学技能数量。技能按使用次数排名。"
      >
        <SettingsCard>
          <SettingsRow
            label="boot_learned_skills_limit"
            description={`自动挂载技能数 · ${RANGES.bootLearnedSkillsLimit.min}–${RANGES.bootLearnedSkillsLimit.max} · 默认 ${DEFAULTS.bootLearnedSkillsLimit}（0=禁用）`}
          >
            <NumberInput
              value={config.bootLearnedSkillsLimit ?? DEFAULTS.bootLearnedSkillsLimit}
              min={RANGES.bootLearnedSkillsLimit.min}
              max={RANGES.bootLearnedSkillsLimit.max}
              onChange={(v) => updateField('bootLearnedSkillsLimit', v)}
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>
    </>
  )
}
