import * as React from 'react'
import { useAtom, useAtomValue } from 'jotai'
import { settingsTabAtom, settingsOpenAtom, type SettingsTab } from '@/atoms/settings-tab'
import { hasUpdateAtom } from '@/atoms/updater'
import { modelStatusAtom } from '@/atoms/stt-atoms'
import { ScrollArea } from '@/components/ui/scroll-area'
import { ToolsTab } from './ToolsTab'
import { ShortcutSettings } from './ShortcutSettings'
import { ConnectivityTab } from './ConnectivityTab'
import { IntelligenceTab } from './IntelligenceTab'
import { ProxySetting } from './ProxySetting'
import { PetSettings } from './PetSettings'
import { SettingsNav } from './SettingsNav'
import { SttSettings } from './SttSettings'
import { MemoryRecallTab } from '@/features/settings'
import { LearnedProfileTab } from './LearnedProfileTab'
import {
  AboutSettings,
  BrowserRuntimeSettings,
  GeneralTab,
  ImChannelsSettings,
  SystemTab,
} from '@/features/settings'
import { SettingsBreadcrumb } from './SettingsBreadcrumb'


function SettingsContent({ tab }: { tab: SettingsTab }) {
  switch (tab) {
    case 'connectivity':
      return <ConnectivityTab />
    case 'intelligence':
      return <IntelligenceTab />
    case 'tools':
      return <ToolsTab />
    case 'memoryRecall':
      return <MemoryRecallTab />
    case 'learnedProfile':
      return <LearnedProfileTab />
    case 'imChannels':
      return <ImChannelsSettings />
    case 'general':
      return <GeneralTab />
    case 'stt':
      return <SttSettings />
    case 'shortcuts':
      return <ShortcutSettings />
    case 'pet':
      return <PetSettings />
    case 'proxy':
      return <ProxySetting />
    case 'browserRuntime':
      return <BrowserRuntimeSettings />
    case 'system':
      return <SystemTab />
    case 'about':
      return <AboutSettings />
    default:
      return <ConnectivityTab />
  }
}

export default function SettingsPanel() {
  const [activeTab, setActiveTab] = useAtom(settingsTabAtom)
  const [, setOpen] = useAtom(settingsOpenAtom)
  const [hasUpdate] = useAtom(hasUpdateAtom)
  const modelStatus = useAtomValue(modelStatusAtom)
  const sttNeedsDownload = modelStatus.kind === 'not-downloaded'

  const TAB_LABEL: Record<SettingsTab, string> = {
    connectivity: '服务商与用量',
    intelligence: '智能',
    tools: '工具与能力',
    memoryRecall: '记忆召回',
    learnedProfile: '学到的偏好',
    imChannels: 'IM 渠道',
    general: '通用与外观',
    stt: '输入（语音）',
    shortcuts: '快捷键',
    pet: '桌面宠物',
    proxy: '代理',
    browserRuntime: '浏览器运行时',
    system: '系统诊断',
    about: '关于',
  }
  const activeLabel = TAB_LABEL[activeTab] ?? '设置'
  const scrollRef = React.useRef<HTMLDivElement | null>(null)

  return (
    <div className="flex flex-col h-full">
      <SettingsBreadcrumb
        tabLabel={activeLabel}
        scrollContainerRef={scrollRef as React.MutableRefObject<HTMLElement | null>}
        onClose={() => setOpen(false)}
      />

      {/* Body: left nav + right content */}
      <div className="flex flex-1 min-h-0">
        <SettingsNav
          active={activeTab}
          onChange={setActiveTab}
          hasUpdate={hasUpdate}
          sttNeedsDownload={sttNeedsDownload}
        />

        {/* Right content */}
        <ScrollArea className="flex-1">
          <div ref={scrollRef} className="max-w-[800px] mx-auto px-6 py-5">
            <SettingsContent tab={activeTab} />
          </div>
        </ScrollArea>
      </div>
    </div>
  )
}
