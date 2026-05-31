import { SettingsSection } from './primitives/SettingsSection'

// [PLACEHOLDER - Tauri adaptation needed]
// ReleaseNotesViewer — 显示发版说明
export function ReleaseNotesViewer() {
  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">发版说明</h2>

      <SettingsSection>
        <div className="text-sm text-muted-foreground py-4">
          暂无发版说明
        </div>
      </SettingsSection>
    </div>
  )
}
