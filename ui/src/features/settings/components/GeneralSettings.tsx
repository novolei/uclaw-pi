import { useState } from 'react'
import { useAtom } from 'jotai'
// Primitives are migrated feature-internals (./primitives), imported relatively.
import {
  SettingsSection,
  SettingsCard,
  SettingsRow,
  SettingsSelect,
  SettingsToggle,
} from './primitives'
import { bottomDockEnabledAtom } from '@/atoms/dock-atoms'
import { useGeneralSettings } from '../hooks/useGeneralSettings'

const LANGUAGE_OPTIONS = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'en', label: 'English' },
  { value: 'ja', label: '日本語' },
]

export function GeneralSettings() {
  // Interface-language load/persist (IPC) lives in the hook; the rest are
  // local-only UI toggles + the bottom-dock atom (no side effects).
  const { language, handleLanguageChange } = useGeneralSettings()
  const [sendOnEnter, setSendOnEnter] = useState(true)
  const [showTimestamp, setShowTimestamp] = useState(true)
  const [bottomDockEnabled, setBottomDockEnabled] = useAtom(bottomDockEnabledAtom)

  return (
    <div className="space-y-6">
      <SettingsSection title="语言与地区">
        <SettingsCard>
          <SettingsRow label="界面语言" description="切换后需要重新加载">
            <SettingsSelect
              value={language}
              onValueChange={handleLanguageChange}
              options={LANGUAGE_OPTIONS}
            />
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>

      <SettingsSection title="消息">
        <SettingsCard>
          <SettingsToggle
            label="按 Enter 发送消息"
            description="关闭后使用 Ctrl+Enter 发送"
            checked={sendOnEnter}
            onCheckedChange={setSendOnEnter}
          />
          <SettingsToggle
            label="显示消息时间戳"
            checked={showTimestamp}
            onCheckedChange={setShowTimestamp}
          />
        </SettingsCard>
      </SettingsSection>

      <SettingsSection title="外观">
        <SettingsCard>
          <SettingsToggle
            label="底部 Dock 导航栏"
            description="触底滑出，macOS Dock 风格快速导航。开启后鼠标移至窗口底边缘时 Dock 自动滑出，移开后自动收回。"
            checked={bottomDockEnabled}
            onCheckedChange={setBottomDockEnabled}
          />
        </SettingsCard>
      </SettingsSection>
    </div>
  )
}
