/**
 * IntelligenceTab — composes ModelSettings + AgentSettings + PromptsSettings
 * as vertically-stacked sub-sections within a single tab.
 */
import * as React from 'react'
import { Play, Square, Loader2 } from 'lucide-react'
import { ModelSettings } from './ModelSettings'
import { AgentSettings } from '@/features/settings'
import { PromptsSettings } from './PromptsSettings'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { Button } from '@/components/ui/button'
import { proactiveStatus, proactiveStart, proactiveStop } from '@/lib/tauri-bridge'

function GeneEvolutionSection(): React.ReactElement {
  const [status, setStatus] = React.useState<string>('unknown')
  const [loading, setLoading] = React.useState(true)
  const [toggling, setToggling] = React.useState(false)

  const refresh = React.useCallback(() => {
    proactiveStatus()
      .then((s: unknown) => {
        // ServiceHealth.status 是 ServiceStatus 枚举（#[serde(tag = "status")]），
        // 序列化为 { status: { status: "Running" | "Stopped" } } 的嵌套结构
        const st = ((s as any)?.status?.status === 'Running') ? 'running' : 'stopped'
        setStatus(st)
      })
      .catch(() => setStatus('error'))
      .finally(() => setLoading(false))
  }, [])

  React.useEffect(() => { refresh() }, [refresh])

  const handleToggle = async () => {
    setToggling(true)
    try {
      if (status === 'running') {
        await proactiveStop()
        setStatus('stopped')
      } else {
        await proactiveStart()
        setStatus('running')
      }
    } catch {
      // keep current status on error
    } finally {
      setToggling(false)
    }
  }

  const isRunning = status === 'running'

  return (
    <SettingsSection
      title="Gene 自进化"
      description="GEP（Gene Evolution Protocol）引擎持续监听 Agent 行为，自动生成优化策略。"
      action={
        <Button
          size="sm"
          variant={isRunning ? 'destructive' : 'default'}
          onClick={handleToggle}
          disabled={loading || toggling}
          className="h-7 text-xs gap-1"
        >
          {toggling ? (
            <Loader2 className="h-3 w-3 animate-spin" />
          ) : isRunning ? (
            <Square className="h-3 w-3" />
          ) : (
            <Play className="h-3 w-3" />
          )}
          {isRunning ? '停止' : '启动'}
        </Button>
      }
    >
      <SettingsCard>
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span
            className={`inline-block w-2 h-2 rounded-full ${
              loading
                ? 'bg-muted-foreground/30'
                : isRunning
                  ? 'bg-emerald-500'
                  : 'bg-muted-foreground/40'
            }`}
          />
          {loading ? '检测中…' : isRunning ? '运行中 — Gene 引擎正在监控 Agent 行为并生成 Capsule' : '已停止'}
        </div>
      </SettingsCard>
    </SettingsSection>
  )
}

export function IntelligenceTab(): React.ReactElement {
  return (
    <div className="space-y-8">
      <section data-settings-section="模型分配">
        <ModelSettings />
      </section>
      <section data-settings-section="Agent 行为">
        <AgentSettings />
      </section>
      <section data-settings-section="提示词">
        <PromptsSettings />
      </section>
      <section data-settings-section="Gene 自进化">
        <GeneEvolutionSection />
      </section>
    </div>
  )
}
