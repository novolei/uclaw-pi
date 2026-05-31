import { SettingsSection } from './primitives/SettingsSection'

// [PLACEHOLDER - Tauri adaptation needed]
// VersionHistory — 版本历史
export function VersionHistory() {
  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">版本历史</h2>

      <SettingsSection>
        <div className="text-sm text-muted-foreground py-4">
          暂无历史版本信息
        </div>
      </SettingsSection>
    </div>
  )
}
