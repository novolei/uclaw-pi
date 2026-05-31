import { SettingsSection } from '@/components/settings/primitives/SettingsSection'
import { SettingsCard } from '@/components/settings/primitives/SettingsCard'
import { useAboutInfo } from '../hooks/useAboutInfo'

export function AboutSettings() {
  const { version, platform } = useAboutInfo()

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">关于 uClaw</h2>

      <SettingsSection>
        <SettingsCard>
          <div className="flex flex-col items-center py-6 space-y-3">
            <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-primary to-primary/60 flex items-center justify-center">
              <span className="text-2xl font-bold text-primary-foreground">u</span>
            </div>
            <div className="text-center">
              <h3 className="text-lg font-semibold">uClaw</h3>
              <p className="text-sm text-muted-foreground">
                {version ? `v${version.appVersion}` : '加载中...'}
              </p>
            </div>
          </div>
        </SettingsCard>
      </SettingsSection>

      <SettingsSection title="系统信息">
        <SettingsCard>
          <div className="space-y-2 text-sm">
            <div className="flex justify-between">
              <span className="text-muted-foreground">应用版本</span>
              <span>{version?.appVersion || '-'}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Tauri 版本</span>
              <span>{version?.tauriVersion || '-'}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Rust 版本</span>
              <span>{version?.rustVersion || '-'}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">操作系统</span>
              <span>{platform ? `${platform.os} (${platform.arch})` : '-'}</span>
            </div>
          </div>
        </SettingsCard>
      </SettingsSection>

      <SettingsSection title="链接">
        <div className="flex gap-2">
          <a
            href="#"
            onClick={(e) => {
              e.preventDefault()
              // [PLACEHOLDER - Tauri adaptation needed] open external link
            }}
            className="text-sm text-primary hover:underline"
          >
            GitHub
          </a>
          <span className="text-muted-foreground">·</span>
          <a
            href="#"
            onClick={(e) => {
              e.preventDefault()
              // [PLACEHOLDER - Tauri adaptation needed] open external link
            }}
            className="text-sm text-primary hover:underline"
          >
            文档
          </a>
        </div>
      </SettingsSection>
    </div>
  )
}
