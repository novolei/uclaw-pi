/**
 * SettingsNav — left rail for the settings dialog.
 *
 * 9 tabs grouped into 3 sections (核心 / 偏好 / 系统), with a top
 * search box that fuzz-filters by label (case-insensitive substring).
 * Non-matching tabs dim to 40% opacity rather than disappear, so the
 * structure stays intact during search.
 */
import * as React from 'react'
import {
  Radio, Cpu, Wrench, Settings, Mic, Keyboard, Smile, Globe, Info, Brain,
  Search, MessageSquare, UserCircle2, Monitor, HardDrive,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import type { SettingsTab } from '@/atoms/settings-tab'

interface NavItem {
  id: SettingsTab
  label: string
  icon: React.ReactNode
}

interface NavGroup {
  title: string
  items: NavItem[]
}

const GROUPS: NavGroup[] = [
  {
    title: '核心',
    items: [
      { id: 'connectivity', label: '服务商与用量', icon: <Radio size={16} /> },
      { id: 'intelligence', label: '智能', icon: <Cpu size={16} /> },
      { id: 'tools', label: '工具与能力', icon: <Wrench size={16} /> },
      { id: 'memoryRecall', label: '记忆召回', icon: <Brain size={16} /> },
      { id: 'learnedProfile', label: '学到的偏好', icon: <UserCircle2 size={16} /> },
      { id: 'imChannels', label: 'IM 渠道', icon: <MessageSquare size={16} /> },
    ],
  },
  {
    title: '偏好',
    items: [
      { id: 'general', label: '通用与外观', icon: <Settings size={16} /> },
      { id: 'stt', label: '输入（语音）', icon: <Mic size={16} /> },
      { id: 'shortcuts', label: '快捷键', icon: <Keyboard size={16} /> },
      { id: 'pet', label: '桌面宠物', icon: <Smile size={16} /> },
    ],
  },
  {
    title: '系统',
    items: [
      { id: 'proxy', label: '代理', icon: <Globe size={16} /> },
      { id: 'browserRuntime', label: '浏览器运行时', icon: <HardDrive size={16} /> },
      { id: 'system', label: '系统诊断', icon: <Monitor size={16} /> },
      { id: 'about', label: '关于', icon: <Info size={16} /> },
    ],
  },
]

interface SettingsNavProps {
  active: SettingsTab
  onChange: (id: SettingsTab) => void
  hasUpdate: boolean
  sttNeedsDownload: boolean
}

export function SettingsNav({
  active,
  onChange,
  hasUpdate,
  sttNeedsDownload,
}: SettingsNavProps): React.ReactElement {
  const [query, setQuery] = React.useState('')
  const q = query.trim().toLowerCase()

  const matches = (label: string): boolean =>
    q === '' || label.toLowerCase().includes(q)

  return (
    <div className="w-[200px] border-r border-border/50 pt-3 px-2 flex-shrink-0 overflow-y-auto">
      {/* Search */}
      <div className="relative mb-3 px-1">
        <Search
          size={12}
          className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground/60 pointer-events-none"
        />
        <input
          type="text"
          placeholder="搜索…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className={cn(
            'w-full bg-muted/40 rounded-md pl-7 pr-2 py-1.5 text-xs',
            'text-foreground placeholder:text-muted-foreground/60',
            'border border-transparent focus:border-border/70 focus:bg-muted/60',
            'outline-none transition-colors',
          )}
        />
      </div>

      {/* Groups */}
      <nav className="space-y-3">
        {GROUPS.map((g) => (
          <div key={g.title}>
            <div className="px-3 py-1 text-[10.5px] uppercase tracking-wider text-muted-foreground/70 font-medium">
              {g.title}
            </div>
            <div className="flex flex-col gap-0.5">
              {g.items.map((it) => {
                const dim = !matches(it.label)
                const isActive = active === it.id
                return (
                  <button
                    key={it.id}
                    type="button"
                    onClick={() => onChange(it.id)}
                    className={cn(
                      'relative flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-all',
                      isActive
                        ? 'bg-muted text-foreground font-medium'
                        : 'text-muted-foreground hover:bg-muted/60 hover:text-foreground',
                      dim && 'opacity-40',
                    )}
                  >
                    {isActive && (
                      <span
                        aria-hidden
                        className="absolute left-0 top-1.5 bottom-1.5 w-[2px] rounded-r bg-primary"
                      />
                    )}
                    {it.icon}
                    <span className="flex-1 text-left">{it.label}</span>
                    {it.id === 'about' && hasUpdate && (
                      <span
                        data-update-dot
                        className="w-1.5 h-1.5 rounded-full bg-red-500"
                      />
                    )}
                    {it.id === 'stt' && sttNeedsDownload && (
                      <span
                        data-stt-dot
                        className="w-1.5 h-1.5 rounded-full bg-primary"
                      />
                    )}
                  </button>
                )
              })}
            </div>
          </div>
        ))}
      </nav>
    </div>
  )
}
