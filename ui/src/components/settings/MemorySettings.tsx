import { useState, useEffect } from 'react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsToggle } from './primitives/SettingsToggle'
import { SettingsCard } from './primitives/SettingsCard'
import { memoryGraphListBoot } from '@/lib/tauri-bridge'

export function MemorySettings() {
  const [autoMemorize, setAutoMemorize] = useState(true)
  const [graphEnabled, setGraphEnabled] = useState(true)
  const [bootNodes, setBootNodes] = useState<unknown[]>([])

  useEffect(() => {
    memoryGraphListBoot({ limit: 20 }).then((data: any) => {
      if (Array.isArray(data)) setBootNodes(data)
    }).catch(() => {})
  }, [])

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">记忆设置</h2>

      <SettingsSection title="自动记忆" description="Agent 自动从对话中提取知识">
        <SettingsToggle
          label="启用自动记忆"
          description="Agent 会自动总结对话中的关键信息"
          checked={autoMemorize}
          onCheckedChange={setAutoMemorize}
        />
        <SettingsToggle
          label="知识图谱"
          description="使用图谱结构存储和检索记忆"
          checked={graphEnabled}
          onCheckedChange={setGraphEnabled}
        />
      </SettingsSection>

      <SettingsSection title="引导节点" description="Agent 每次对话开始时自动加载的记忆">
        {(bootNodes as any[]).length > 0 ? (
          <div className="space-y-2">
            {(bootNodes as any[]).map((node: any, i) => (
              <SettingsCard key={i}>
                <div className="text-sm">{node.title || node.id || `节点 ${i + 1}`}</div>
              </SettingsCard>
            ))}
          </div>
        ) : (
          <div className="text-sm text-muted-foreground py-4 text-center">
            暂无引导节点
          </div>
        )}
      </SettingsSection>
    </div>
  )
}
