import { useState } from 'react'
import { useAtomValue } from 'jotai'
// Sub-components + primitives still live under components/settings/ (migrated in a
// later phase); imported via the @/ alias so behavior is preserved exactly.
import { SettingsSection } from '@/components/settings/primitives/SettingsSection'
import { SettingsToggle } from '@/components/settings/primitives/SettingsToggle'
import { SettingsCard } from '@/components/settings/primitives/SettingsCard'
import { SettingsRow } from '@/components/settings/primitives/SettingsRow'
import { PersonaStudio } from '@/components/settings/PersonaStudio'
import { PersonaBondTimeline } from '@/components/settings/PersonaBondTimeline'
import { activeProviderModelAtom } from '@/atoms/active-model'
import { Cpu } from 'lucide-react'
import { usePlanModeSuggest } from '../hooks/usePlanModeSuggest'

export function AgentSettings() {
  const activeModel = useAtomValue(activeProviderModelAtom)
  const [streamResponse, setStreamResponse] = useState(true)
  const [autoTitle, setAutoTitle] = useState(true)
  const { enabled: planSuggestEnabled, setPlanSuggestEnabled } = usePlanModeSuggest()

  return (
    <div className="space-y-6">
      <PersonaStudio />
      <PersonaBondTimeline />

      <SettingsSection title="当前模型">
        <SettingsCard>
          <SettingsRow
            label="活跃模型"
            description="在会话输入框底部工具栏的模型选择器中切换"
            icon={<Cpu size={15} className="text-muted-foreground" />}
          >
            <span className="text-sm text-muted-foreground">
              {activeModel ? `${activeModel.providerId} / ${activeModel.modelId}` : '未选择'}
            </span>
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      <SettingsSection title="行为设置">
        <SettingsCard>
          <SettingsToggle
            label="流式响应"
            description="实时显示 AI 生成内容"
            checked={streamResponse}
            onCheckedChange={setStreamResponse}
          />
          <SettingsToggle
            label="自动生成标题"
            description="根据对话内容自动命名会话"
            checked={autoTitle}
            onCheckedChange={setAutoTitle}
          />
          <SettingsToggle
            label="为复杂任务建议 Plan 模式"
            description="检测到多步骤构建/重构/设计请求时弹出建议横幅；可被 agent 主动调用，也按关键词触发。"
            checked={planSuggestEnabled}
            onCheckedChange={setPlanSuggestEnabled}
          />
        </SettingsCard>
      </SettingsSection>
    </div>
  )
}
