// SettingsSection primitive still lives under components/settings/.
import { SettingsSection } from '@/components/settings/primitives/SettingsSection'

// [PLACEHOLDER - Tauri adaptation needed]
// BotHubSettings — Bot Hub marketplace
export function BotHubSettings() {
  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">Bot Hub</h2>

      <SettingsSection title="Bot 市场" description="浏览和安装社区 Bot">
        <div className="text-sm text-muted-foreground py-8 text-center">
          Bot Hub 即将上线，敬请期待
        </div>
      </SettingsSection>
    </div>
  )
}
