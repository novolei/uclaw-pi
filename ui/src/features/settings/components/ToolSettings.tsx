import type { ActiveManifestSkill } from '@/lib/types'
// Primitives still live under components/settings/ (migrated in a later phase);
// imported via the @/ alias so behavior is preserved exactly. The tags editor is
// a migrated sibling — imported relatively.
import { SettingsSection } from '@/components/settings/primitives/SettingsSection'
import { WorkspaceSkillTagsEditor } from './WorkspaceSkillTagsEditor'
import { Button } from '@/components/ui/button'
import { RefreshCw } from 'lucide-react'
import { useToolSettings } from '../hooks/useToolSettings'

type ProvenanceKey = ActiveManifestSkill['provenance']

const PROVENANCE_BADGE: Record<ProvenanceKey, { label: string; className: string }> = {
  bundled: { label: 'Bundled', className: 'bg-primary/10 text-primary border-primary/20' },
  user:    { label: 'User',    className: 'bg-emerald-500/10 text-emerald-600 border-emerald-500/20 dark:text-emerald-400' },
  project: { label: 'Project', className: 'bg-muted text-muted-foreground border-border' },
  learned: { label: 'Learned', className: 'bg-amber-500/10 text-amber-600 border-amber-500/20 dark:text-amber-400' },
}

export function ToolSettings() {
  const { activeManifest, manifestLoading, refreshActiveManifest } = useToolSettings()

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">工具设置</h2>

      <div className="rounded-lg border border-border/60 bg-muted/20 px-3 py-2.5 text-[12px] text-muted-foreground">
        技能与集成（MCP）的完整管理已移至 <span className="text-foreground font-medium">万花筒 → 技能 / 集成</span>。
      </div>

      <SettingsSection
        title="工作区 Skill 标签 (V19+)"
        description="按标签过滤当前工作区可用的 Skill — 留空 = 默认全部可见；填写后只有匹配标签的 Skill 进入 manifest。未打标的 Skill 默认视为全局（始终可见），保护新抽取学得技能的冷启动。"
      >
        <WorkspaceSkillTagsEditor />
      </SettingsSection>

      <SettingsSection
        title="活动技能（调试）"
        description="此刻**会被注入到 Agent system prompt** 的技能清单 — 顺序与 Agent 看到的完全一致。用于排查「为什么这条 skill 没被召回」之类的问题。包含 builtin (Bundled/User/Project) + Learned (已 promoted)。如果配置了上方的工作区标签，此处显示的是过滤后的结果。"
      >
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <div className="text-xs text-muted-foreground">
              {activeManifest == null
                ? '加载中…'
                : activeManifest.length === 0
                ? '当前 manifest 为空 — 没有 enabled 的 builtin 技能且没有 promoted 的 learned 技能'
                : `共 ${activeManifest.length} 条按 E3 排序`}
            </div>
            <Button
              size="sm"
              variant="ghost"
              disabled={manifestLoading}
              onClick={refreshActiveManifest}
            >
              <RefreshCw className={`size-3.5 mr-1 ${manifestLoading ? 'animate-spin' : ''}`} />
              刷新
            </Button>
          </div>
          {activeManifest && activeManifest.length > 0 && (
            <div className="space-y-1">
              {activeManifest.map((row) => {
                const badge = PROVENANCE_BADGE[row.provenance] ?? { label: row.provenance, className: 'bg-muted text-muted-foreground border-border' }
                return (
                  <div
                    key={`${row.rank}-${row.name}`}
                    className="flex items-start gap-2 px-2 py-1.5 rounded border border-border/40 bg-muted/30 hover:bg-muted/50 transition-colors"
                  >
                    <span className="text-[10px] text-muted-foreground/60 tabular-nums w-5 flex-shrink-0 text-right pt-0.5">
                      {row.rank}.
                    </span>
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-1.5 flex-wrap">
                        <span className="text-xs font-medium truncate">{row.name}</span>
                        <span className={`text-[9px] px-1 py-px rounded border ${badge.className}`}>
                          {badge.label}
                        </span>
                        {row.provenance === 'learned' && row.citedCount > 0 && (
                          <span className="text-[9px] text-muted-foreground/60">
                            ✓ {row.citedCount}
                          </span>
                        )}
                      </div>
                      {row.summary && (
                        <div className="text-[11px] text-muted-foreground mt-0.5 line-clamp-1">
                          {row.summary}
                        </div>
                      )}
                    </div>
                  </div>
                )
              })}
            </div>
          )}
        </div>
      </SettingsSection>
    </div>
  )
}
