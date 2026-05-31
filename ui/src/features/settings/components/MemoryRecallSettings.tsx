/**
 * MemoryRecallSettings — 记忆召回参数配置表单
 *
 * Thin shell: state + IPC live in useMemoryRecallSettings; the form is split into
 * memory-recall/ section cards (RecallBudget / FusionStrategy / Advanced). Split
 * out of the 474-line components/settings/MemoryRecallSettings during the
 * features/settings migration (code-organization ADR 2026-05-31). Behavior
 * preserved verbatim: load → merge defaults, dirty-tracking, save, reset.
 */
import { RotateCcw, Save } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { useMemoryRecallSettings } from '../hooks/useMemoryRecallSettings'
import { RecallBudgetCard } from './memory-recall/RecallBudgetCard'
import { FusionStrategyCard } from './memory-recall/FusionStrategyCard'
import { AdvancedRecallCard } from './memory-recall/AdvancedRecallCard'

export function MemoryRecallSettings(): React.ReactElement {
  const { config, loading, saving, dirty, updateField, handleSave, handleReset } =
    useMemoryRecallSettings()

  if (loading) {
    return (
      <div className="space-y-6 animate-pulse">
        <div className="h-6 bg-muted rounded w-32" />
        <div className="h-40 bg-muted rounded" />
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* 操作栏 */}
      <div className="flex items-center justify-between">
        <p className="text-xs text-muted-foreground">
          修改后点击「保存」生效，每次 Agent 对话自动热加载最新配置
        </p>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleReset}
            disabled={saving}
            className="h-7 text-xs gap-1"
          >
            <RotateCcw size={12} />
            恢复默认
          </Button>
          <Button
            size="sm"
            onClick={handleSave}
            disabled={!dirty || saving}
            className="h-7 text-xs gap-1"
          >
            <Save size={12} />
            {saving ? '保存中…' : '保存'}
          </Button>
        </div>
      </div>

      <RecallBudgetCard config={config} updateField={updateField} />
      <FusionStrategyCard config={config} updateField={updateField} />
      <AdvancedRecallCard config={config} updateField={updateField} />
    </div>
  )
}
