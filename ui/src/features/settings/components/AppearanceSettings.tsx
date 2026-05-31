import { useAtom, useAtomValue } from 'jotai'
import { Check } from 'lucide-react'
import { cn } from '@/lib/utils'
import { themeModeAtom, themeStyleAtom, applyThemeToDOM, updateThemeMode, updateThemeStyle, systemIsDarkAtom } from '@/atoms/theme'
import { stickyUserMessageEnabledAtom, updateStickyUserMessageEnabled, agentStatusBarEnabledAtom } from '@/atoms/ui-preferences'
// Primitives are migrated feature-internals (./primitives), imported relatively.
import { SettingsSection, SettingsToggle } from './primitives'
import type { ThemeStyle } from '@/lib/chat-types'

// ─────────────────────────────────────────────────────────
// Theme card definitions
// ─────────────────────────────────────────────────────────

type ThemeEntry =
  | { kind: 'basic'; id: 'light' | 'dark'; title: string; subtitle: string; className: string; preview: [string, string, string]; specialBg?: false }
  | { kind: 'special'; id: ThemeStyle; title: string; subtitle: string; className: string; preview: [string, string, string]; specialBg?: boolean }

const THEME_ENTRIES: ThemeEntry[] = [
  {
    kind: 'basic',
    id: 'light',
    title: '浅色',
    subtitle: '明亮干净',
    className: 'bg-[#ffffff] text-[#111111]',
    preview: ['#f0f0f0', '#e0e0e0', '#111111'],
  },
  {
    kind: 'basic',
    id: 'dark',
    title: '深色',
    subtitle: '经典暗黑',
    className: 'bg-[#121212] text-[#f0f0f0]',
    preview: ['#2a2a2a', '#1e1e1e', '#f0f0f0'],
  },
  {
    kind: 'special',
    id: 'ocean-light',
    title: '晴空碧海',
    subtitle: '蓝调日光',
    className: 'bg-[#ecf2f7] text-[#1b2632]',
    preview: ['#c3d5e5', '#d7e4ef', '#408abf'],
  },
  {
    kind: 'special',
    id: 'ocean-dark',
    title: '苍穹暮色',
    subtitle: '深海夜境',
    className: 'bg-[#182434] text-[#e7ebef]',
    preview: ['#202e3c', '#19242e', '#084672'],
  },
  {
    kind: 'special',
    id: 'forest-light',
    title: '森息晨光',
    subtitle: '自然清绿',
    className: 'bg-[#eff5f1] text-[#1d3026]',
    preview: ['#cad8cf', '#dae7df', '#3f8361'],
  },
  {
    kind: 'special',
    id: 'forest-dark',
    title: '森息夜语',
    subtitle: '幽林深绿',
    className: 'bg-[#212c26] text-[#e3e8e5]',
    preview: ['#24332b', '#1b2721', '#185337'],
  },
  {
    kind: 'special',
    id: 'slate-light',
    title: '云朵舞者',
    subtitle: '温暖灰调',
    className: 'bg-[#f0efec] text-[#312f2a]',
    preview: ['#d0cdc6', '#dddad4', '#bda59b'],
  },
  {
    kind: 'special',
    id: 'slate-dark',
    title: '莫兰迪夜',
    subtitle: '淡紫雾境',
    className: 'bg-[#1d1b20] text-[#e9e6e3]',
    preview: ['#322e36', '#272429', '#c9a89e'],
  },
  {
    kind: 'special',
    id: 'warm-paper',
    title: '新暖纸',
    subtitle: '纸感白天',
    className: 'bg-[#f8f5ed] text-[#3b3d3f]',
    preview: ['#e2ddd2', '#f4f2ea', '#537d96'],
  },
  {
    kind: 'special',
    id: 'qingye',
    title: '青夜',
    subtitle: '柔和夜间',
    className: 'bg-[#3b4a54] text-[#c8d1d8]',
    preview: ['#34424b', '#445560', '#aa798d'],
  },
  {
    kind: 'special',
    id: 'black',
    title: '黑色',
    subtitle: '深色专注',
    className: 'bg-[#18191d] text-[#dedee3]',
    preview: ['#24252b', '#2f2e35', '#c19a3b'],
  },
  {
    kind: 'special',
    id: 'the-finals',
    title: 'THE FINALS',
    subtitle: '竞技赛场',
    className: 'text-[#fff4df]',
    preview: ['#fff4df', '#ffd23f', '#d91f3c'],
    specialBg: true,
  },
]

// ─────────────────────────────────────────────────────────
// ThemeCard component
// ─────────────────────────────────────────────────────────

function ThemeCard({
  selected,
  onSelect,
  title,
  subtitle,
  className,
  preview,
  specialBg,
}: {
  selected: boolean
  onSelect: () => void
  title: string
  subtitle: string
  className: string
  preview: [string, string, string]
  specialBg?: boolean
}) {
  const finalsStyle = specialBg ? {
    backgroundImage: `linear-gradient(90deg, rgba(10,10,12,0.72) 0%, rgba(217,31,60,0.55) 100%), url('/themes/the-finals/s10-keyart-bkg.png')`,
    backgroundSize: 'cover',
    backgroundPosition: 'center',
  } : undefined

  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={selected}
      style={finalsStyle}
      className={cn(
        'group relative flex h-[96px] flex-col justify-between overflow-hidden rounded-[10px] border px-3.5 py-2.5 text-left transition-all duration-200',
        'hover:-translate-y-0.5 hover:shadow-[0_12px_28px_rgba(0,0,0,0.10)]',
        selected
          ? 'border-foreground/80 ring-2 ring-foreground/60 shadow-[0_8px_20px_rgba(0,0,0,0.08)]'
          : 'border-white/20 shadow-[0_4px_12px_rgba(0,0,0,0.06)]',
        className,
      )}
    >
      {/* Color swatches — top right */}
      <div className="flex justify-end gap-1">
        {preview.map((color, i) => (
          <span
            key={i}
            className="h-2 w-6 rounded-full opacity-90"
            style={{ backgroundColor: color }}
          />
        ))}
      </div>

      {/* Title & subtitle — bottom left */}
      <div>
        <div className="text-[15px] font-semibold leading-tight tracking-tight">{title}</div>
        <div className="mt-0.5 text-[11.5px] opacity-65">{subtitle}</div>
      </div>

      {/* Selected checkmark */}
      {selected && (
        <span className="absolute right-2 top-2 flex size-[18px] items-center justify-center rounded-full bg-white/90 shadow-sm">
          <Check className="size-3 text-zinc-900" strokeWidth={2.5} />
        </span>
      )}
    </button>
  )
}

// ─────────────────────────────────────────────────────────
// AppearanceSettings
// ─────────────────────────────────────────────────────────

export function AppearanceSettings() {
  const [themeMode, setThemeModeAtom] = useAtom(themeModeAtom)
  const [themeStyle, setThemeStyleAtom] = useAtom(themeStyleAtom)
  const systemIsDark = useAtomValue(systemIsDarkAtom)
  const [stickyUserMessageEnabled, setStickyUserMessageEnabled] = useAtom(stickyUserMessageEnabledAtom)
  const [agentStatusBarEnabled, setAgentStatusBarEnabled] = useAtom(agentStatusBarEnabledAtom)

  async function handleStickyToggle(enabled: boolean): Promise<void> {
    setStickyUserMessageEnabled(enabled)
    await updateStickyUserMessageEnabled(enabled)
  }

  function isSelected(entry: ThemeEntry): boolean {
    if (entry.kind === 'basic') {
      return themeMode === entry.id
    }
    return themeMode === 'special' && themeStyle === entry.id
  }

  async function selectEntry(entry: ThemeEntry) {
    if (entry.kind === 'basic') {
      setThemeModeAtom(entry.id)
      setThemeStyleAtom('default')
      applyThemeToDOM(entry.id, 'default', systemIsDark)
      await updateThemeMode(entry.id)
      await updateThemeStyle('default')
    } else {
      setThemeModeAtom('special')
      setThemeStyleAtom(entry.id)
      applyThemeToDOM('special', entry.id, systemIsDark)
      await updateThemeMode('special')
      await updateThemeStyle(entry.id)
    }
  }

  async function selectSystem() {
    setThemeModeAtom('system')
    setThemeStyleAtom('default')
    applyThemeToDOM('system', 'default', systemIsDark)
    await updateThemeMode('system')
    await updateThemeStyle('default')
  }

  const isSystem = themeMode === 'system'

  return (
    <div className="space-y-6">
      <h2 className="text-lg font-semibold">外观设置</h2>

      <div className="space-y-3">
        {/* Section header with "Follow system" button */}
        <div className="flex items-end justify-between gap-2">
          <div>
            <div className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground/70 mb-0.5">
              主题
            </div>
            <div className="text-[12px] text-muted-foreground">
              选择后立即应用，并会保存到下次启动。
            </div>
          </div>
          <button
            type="button"
            onClick={selectSystem}
            className={cn(
              'shrink-0 h-7 rounded-xl border px-3 text-[11px] font-medium transition-colors',
              isSystem
                ? 'border-primary/40 bg-primary/10 text-primary'
                : 'border-border bg-muted/30 text-muted-foreground hover:bg-muted/60',
            )}
          >
            跟随系统
          </button>
        </div>

        {/* Theme cards grid */}
        <div className="grid grid-cols-3 gap-2.5">
          {THEME_ENTRIES.map((entry) => (
            <ThemeCard
              key={entry.id}
              selected={isSelected(entry)}
              onSelect={() => selectEntry(entry)}
              title={entry.title}
              subtitle={entry.subtitle}
              className={entry.className}
              preview={entry.preview}
              specialBg={entry.specialBg}
            />
          ))}
        </div>
      </div>

      <SettingsSection title="界面行为">
        <SettingsToggle
          label="用户消息置顶条"
          description="Agent 运行时把最近一条用户消息精简后钉在输入框上方，作为上下文提示。"
          checked={stickyUserMessageEnabled}
          onCheckedChange={handleStickyToggle}
        />
        <SettingsToggle
          label="Agent 状态条"
          description="在输入框上方常驻显示一条 Agent 执行状态（阶段 / 工具 / 耗时 / token / 费用）。默认关闭——流式气泡内已有运行指示器。"
          checked={agentStatusBarEnabled}
          onCheckedChange={setAgentStatusBarEnabled}
        />
      </SettingsSection>
    </div>
  )
}
