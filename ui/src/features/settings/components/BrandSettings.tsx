import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'

/**
 * BrandSettings — uClaw 品牌设置
 * (原 PromaLogoSettings，品牌已替换为 uClaw)
 */
export function BrandSettings() {
  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">品牌设置</h2>

      <SettingsSection title="uClaw 品牌" description="自定义应用外观标识">
        <SettingsCard>
          <div className="flex flex-col items-center py-6 space-y-3">
            <div className="w-20 h-20 rounded-2xl bg-gradient-to-br from-primary to-primary/60 flex items-center justify-center">
              <span className="text-3xl font-bold text-primary-foreground">u</span>
            </div>
            <div className="text-center">
              <h3 className="text-lg font-semibold">uClaw</h3>
              <p className="text-xs text-muted-foreground">Your AI Assistant</p>
            </div>
          </div>
        </SettingsCard>
      </SettingsSection>
    </div>
  )
}
