import { useEffect, useRef, useState } from 'react'
import { useSetAtom } from 'jotai'
import { Check, ChevronDown, AlertCircle, MessageSquare, Wrench, Zap, FileText, Brain } from 'lucide-react'
import { cn } from '@/lib/utils'
import { settingsOpenAtom, settingsTabAtom } from '@/atoms/settings-tab'
import { useModelSettings } from '../hooks/useModelSettings'

// ── Role metadata ────────────────────────────────────────────────────────────

interface RoleMeta {
  label: string
  desc: string
  Icon: typeof MessageSquare
  color: string
}

const ROLE_META: Record<string, RoleMeta> = {
  chat: {
    label: '主对话模型',
    desc: '主对话 / 复杂交互',
    Icon: MessageSquare,
    color: 'text-emerald-600',
  },
  utility: {
    label: '轻工具模型',
    desc: '摘要 / 翻译 / 轻量调用',
    Icon: Wrench,
    color: 'text-blue-600',
  },
  utility_large: {
    label: '重工具模型',
    desc: '复杂推理 / 多步任务',
    Icon: Zap,
    color: 'text-violet-600',
  },
  summarizer: {
    label: '摘要模型',
    desc: '记忆摘要 / 文本压缩',
    Icon: FileText,
    color: 'text-amber-600',
  },
  compiler: {
    label: '编译模型',
    desc: '记忆编译 / 快速响应',
    Icon: Brain,
    color: 'text-rose-600',
  },
}

// ── Grouped model data ───────────────────────────────────────────────────────

interface ModelGroup {
  providerId: string
  models: string[]
}

// ── Dropdown ─────────────────────────────────────────────────────────────────

interface ModelDropdownProps {
  value: string | null
  groups: ModelGroup[]
  isOpen: boolean
  onOpen: () => void
  onClose: () => void
  onChange: (ref: string | null) => void
  containerRef: (el: HTMLDivElement | null) => void
}

function ModelDropdown({ value, groups, isOpen, onOpen, onClose, onChange, containerRef }: ModelDropdownProps) {
  const [provider, modelId] = value ? value.split('/') : [null, null]
  const hasModels = groups.some((g) => g.models.length > 0)
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)

  return (
    <div ref={containerRef} className="relative w-[260px]">
      <button
        type="button"
        onClick={() => (isOpen ? onClose() : onOpen())}
        className={cn(
          'flex h-8 w-full items-center justify-between gap-2 rounded-lg border px-2.5 text-[12px] font-medium transition-all',
          isOpen
            ? 'border-primary/40 bg-primary/[0.03] ring-2 ring-primary/15'
            : 'border-border bg-muted/30 hover:bg-muted/60',
        )}
      >
        <div className="flex min-w-0 items-center gap-1.5 truncate">
          {provider && (
            <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[9.5px] font-semibold text-muted-foreground">
              {provider}
            </span>
          )}
          <span className={cn('truncate', value ? 'text-foreground' : 'text-muted-foreground/60')}>
            {modelId ?? '未设置（使用默认）'}
          </span>
        </div>
        <ChevronDown className={cn('h-3 w-3 shrink-0 text-muted-foreground/50 transition-transform', isOpen && 'rotate-180')} />
      </button>

      {isOpen && (
        <div className="absolute left-0 z-50 mt-1 max-h-64 w-full min-w-[200px] overflow-y-auto rounded-lg border border-border bg-popover shadow-lg">
          {/* Clear */}
          <button
            type="button"
            onClick={() => { onChange(null); onClose() }}
            className={cn(
              'flex w-full items-center gap-2 px-3 py-2 text-left text-[12px] transition-colors hover:bg-accent/50',
              !value ? 'font-medium text-primary' : 'text-muted-foreground',
            )}
          >
            {!value ? <Check className="h-3 w-3 shrink-0 text-primary" /> : <span className="h-3 w-3 shrink-0" />}
            未设置（使用默认）
          </button>

          {/* Groups */}
          {groups.filter((g) => g.models.length > 0).map((group) => (
            <div key={group.providerId}>
              <div className="border-t border-border/50 bg-muted/30 px-3 py-1 text-[9.5px] font-semibold uppercase tracking-widest text-muted-foreground/60">
                {group.providerId}
              </div>
              {group.models.map((mid) => {
                const ref = `${group.providerId}/${mid}`
                const selected = value === ref
                return (
                  <button
                    key={ref}
                    type="button"
                    onClick={() => { onChange(ref); onClose() }}
                    className={cn(
                      'flex w-full items-center gap-2 px-3 py-1.5 text-left text-[12px] transition-colors hover:bg-accent/50',
                      selected && 'text-primary',
                    )}
                  >
                    {selected ? <Check className="h-3 w-3 shrink-0 text-primary" /> : <span className="h-3 w-3 shrink-0" />}
                    <span className={cn('truncate', selected && 'font-medium')}>{mid}</span>
                  </button>
                )
              })}
            </div>
          ))}

          {!hasModels && (
            <div className="flex flex-col items-center gap-1.5 px-3 py-5 text-center">
              <AlertCircle className="h-4 w-4 text-muted-foreground/40" />
              <p className="text-[11.5px] text-muted-foreground">暂无已配置的模型</p>
              <button
                type="button"
                onClick={() => { setSettingsOpen(true); setSettingsTab('connectivity') }}
                className="mt-0.5 rounded-md bg-primary px-2 py-1 text-[10.5px] font-medium text-primary-foreground hover:bg-primary/90"
              >
                配置服务商 →
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

// ── Main page ─────────────────────────────────────────────────────────────────

export function ModelSettings() {
  const { groups, roleConfigs, allRoles, handleChange } = useModelSettings()
  const [openRole, setOpenRole] = useState<string | null>(null)
  const dropdownRefs = useRef<Map<string, HTMLDivElement>>(new Map())

  const configuredCount = roleConfigs.filter((r) => r.model_ref).length

  // Close dropdown on outside click / ESC
  useEffect(() => {
    if (!openRole) return
    const handleKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpenRole(null) }
    const handleClick = (e: MouseEvent) => {
      const el = dropdownRefs.current.get(openRole)
      if (el && !el.contains(e.target as Node)) setOpenRole(null)
    }
    document.addEventListener('keydown', handleKey)
    document.addEventListener('mousedown', handleClick)
    return () => {
      document.removeEventListener('keydown', handleKey)
      document.removeEventListener('mousedown', handleClick)
    }
  }, [openRole])

  return (
    <div className="space-y-6">
      {/* Summary */}
      <div className="flex items-center justify-between">
        <div>
          <p className="text-[12px] text-muted-foreground">
            为不同场景分配专属模型；未设置的场景将使用当前活跃模型。
          </p>
        </div>
        <span className="text-[11px] text-muted-foreground/60">
          {configuredCount}/{allRoles.length} 已配置
        </span>
      </div>

      {/* Role cards */}
      <div className="space-y-2">
        {roleConfigs.map(({ role, model_ref }) => {
          const meta = ROLE_META[role]
          if (!meta) return null
          const { label, desc, Icon, color } = meta
          return (
            <div
              key={role}
              className="flex items-center gap-4 rounded-xl border border-border/50 bg-card px-4 py-3"
            >
              {/* Icon */}
              <div className={cn('flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-muted/50', color)}>
                <Icon size={15} />
              </div>

              {/* Label */}
              <div className="flex-1 min-w-0">
                <div className="text-[13px] font-medium text-foreground">{label}</div>
                <div className="text-[11px] text-muted-foreground">{desc}</div>
              </div>

              {/* Dropdown */}
              <ModelDropdown
                value={model_ref ?? null}
                groups={groups}
                isOpen={openRole === role}
                onOpen={() => setOpenRole(role)}
                onClose={() => setOpenRole(null)}
                onChange={(ref) => void handleChange(role, ref)}
                containerRef={(el) => {
                  if (el) dropdownRefs.current.set(role, el)
                  else dropdownRefs.current.delete(role)
                }}
              />
            </div>
          )
        })}
      </div>
    </div>
  )
}
