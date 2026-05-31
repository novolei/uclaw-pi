// MemoryRecall: Token 预算 + 召回数量限制 sections. Extracted verbatim out of the
// 474-line components/settings/MemoryRecallSettings during the features/settings
// split (code-organization ADR 2026-05-31).
import { SettingsSection } from '@/components/settings/primitives/SettingsSection'
import { SettingsCard } from '@/components/settings/primitives/SettingsCard'
import { SettingsRow } from '@/components/settings/primitives/SettingsRow'
import { NumberInput } from './NumberInput'
import { DEFAULTS, RANGES } from '../../lib/memory-recall'
import type { MemoryRecallCardProps } from './types'

export function RecallBudgetCard({ config, updateField }: MemoryRecallCardProps): React.ReactElement {
  return (
    <>
      {/* Token 预算 */}
      <SettingsSection
        title="Token 预算"
        description="控制每轮 Agent 对话中记忆上下文占用的最大 token 数。设为 0 可禁用限制。"
      >
        <SettingsCard>
          <SettingsRow
            label="token_budget"
            description={`范围: ${RANGES.tokenBudget.min} – ${RANGES.tokenBudget.max} tokens · 默认: ${DEFAULTS.tokenBudget}`}
          >
            <NumberInput
              value={config.tokenBudget ?? DEFAULTS.tokenBudget}
              min={RANGES.tokenBudget.min}
              max={RANGES.tokenBudget.max}
              onChange={(v) => updateField('tokenBudget', v)}
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      {/* 召回数量限制 */}
      <SettingsSection
        title="召回数量限制"
        description="控制各记忆层的候选召回数量。减少可降低 token 消耗但可能遗漏关键记忆。"
      >
        <SettingsCard>
          <SettingsRow
            label="boot_limit"
            description={`启动层（始终注入）· ${RANGES.bootLimit.min}–${RANGES.bootLimit.max} · 默认 ${DEFAULTS.bootLimit}`}
          >
            <NumberInput
              value={config.bootLimit ?? DEFAULTS.bootLimit}
              min={RANGES.bootLimit.min}
              max={RANGES.bootLimit.max}
              onChange={(v) => updateField('bootLimit', v)}
            />
          </SettingsRow>
          <SettingsRow
            label="trigger_limit"
            description={`触发层（直接匹配）· ${RANGES.triggerLimit.min}–${RANGES.triggerLimit.max} · 默认 ${DEFAULTS.triggerLimit}`}
          >
            <NumberInput
              value={config.triggerLimit ?? DEFAULTS.triggerLimit}
              min={RANGES.triggerLimit.min}
              max={RANGES.triggerLimit.max}
              onChange={(v) => updateField('triggerLimit', v)}
            />
          </SettingsRow>
          <SettingsRow
            label="seed_limit"
            description={`种子层（触发邻居）· ${RANGES.seedLimit.min}–${RANGES.seedLimit.max} · 默认 ${DEFAULTS.seedLimit}`}
          >
            <NumberInput
              value={config.seedLimit ?? DEFAULTS.seedLimit}
              min={RANGES.seedLimit.min}
              max={RANGES.seedLimit.max}
              onChange={(v) => updateField('seedLimit', v)}
            />
          </SettingsRow>
          <SettingsRow
            label="expansion_limit"
            description={`扩展层（种子邻居）· ${RANGES.expansionLimit.min}–${RANGES.expansionLimit.max} · 默认 ${DEFAULTS.expansionLimit}`}
          >
            <NumberInput
              value={config.expansionLimit ?? DEFAULTS.expansionLimit}
              min={RANGES.expansionLimit.min}
              max={RANGES.expansionLimit.max}
              onChange={(v) => updateField('expansionLimit', v)}
            />
          </SettingsRow>
          <SettingsRow
            label="recent_limit"
            description={`近期层（最近使用）· ${RANGES.recentLimit.min}–${RANGES.recentLimit.max} · 默认 ${DEFAULTS.recentLimit}`}
          >
            <NumberInput
              value={config.recentLimit ?? DEFAULTS.recentLimit}
              min={RANGES.recentLimit.min}
              max={RANGES.recentLimit.max}
              onChange={(v) => updateField('recentLimit', v)}
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>
    </>
  )
}
