/**
 * SkillDetail — 技能模块右栏:选中技能的详情。
 *
 * 学得技能:场景 / 原则 / 步骤 / 陷阱 + 可展开的演化历史(SkillEvolutionTab)。
 * 内置技能:描述 / 版本 / 作者 / 分类 / provenance 徽章 + Fork(仅 bundled)。
 * 顶部右侧「Agent 可调用」开关。渲染逻辑迁自原 SkillsSettings 的 SkillCard 展开体。
 *
 * Phase 4 (G8): Edit mode for learned skills — context/principles/steps/
 * pitfalls/category/tags/validationHint become editable when the pencil
 * button is toggled.
 */
import * as React from 'react'
import Markdown from 'react-markdown'
import { History, ArrowUp, Archive, RotateCcw, Pencil, Save, X, BookOpen, Hash } from 'lucide-react'
import { Switch } from '@/components/ui/switch'
import { Button } from '@/components/ui/button'
import { SkillEvolutionTab } from '@/features/settings'
import { cn } from '@/lib/utils'
import { setSkillLifecycle, updateLearnedSkill } from '@/lib/tauri-bridge'
import { toast } from 'sonner'
import type { UnifiedSkill } from './SkillsModule'

const LIFECYCLE_BADGE: Record<string, { label: string; className: string }> = {
  draft:      { label: '草稿', className: 'bg-yellow-500/15 text-yellow-700 border-yellow-500/30 dark:text-yellow-400' },
  promoted:   { label: '已晋升', className: 'bg-emerald-500/15 text-emerald-700 border-emerald-500/30 dark:text-emerald-400' },
  deprecated: { label: '已弃用', className: 'bg-muted text-muted-foreground border-border' },
}

const PROVENANCE_BADGE: Record<'bundled' | 'user' | 'project' | 'marketplace', { label: string; className: string }> = {
  bundled:     { label: 'Bundled',     className: 'bg-primary/10 text-primary border-primary/20' },
  user:        { label: 'User',        className: 'bg-emerald-500/10 text-emerald-600 border-emerald-500/20 dark:text-emerald-400' },
  project:     { label: 'Project',     className: 'bg-muted text-muted-foreground border-border' },
  marketplace: { label: 'Marketplace', className: 'bg-accent/10 text-accent-foreground border-accent/20' },
}

const CATEGORY_OPTIONS = [
  { value: 'repair', label: 'Repair' },
  { value: 'optimize', label: 'Optimize' },
  { value: 'innovate', label: 'Innovate' },
]

function formatDate(s: string): string {
  if (!s) return ''
  const d = new Date(s)
  if (isNaN(d.getTime())) return s
  return `${d.getFullYear()}/${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

function Section({ label, children, id }: { label: string; children: React.ReactNode; id?: string }): React.ReactElement {
  return (
    <div id={id} className="rounded-xl border border-border/30 bg-muted/15 p-5 mb-3 transition-all duration-200 hover:border-border/50">
      <div className="mb-3 text-[10.5px] font-semibold uppercase tracking-wider text-muted-foreground/80">
        {label}
      </div>
      <div className="text-[12.5px] leading-relaxed text-foreground/85">
        {children}
      </div>
    </div>
  )
}

function MarkdownBlock({ text }: { text: string }): React.ReactElement {
  return (
    <div className="prose prose-sm max-w-none text-[12.5px] text-foreground/90
                    prose-p:my-1 prose-ul:my-1 prose-ol:my-1 prose-li:my-0
                    prose-headings:text-foreground prose-strong:text-foreground
                    prose-code:text-foreground prose-code:bg-muted prose-code:px-1 prose-code:rounded">
      <Markdown>{text}</Markdown>
    </div>
  )
}

interface EditableFieldProps {
  label: string
  value: string
  onChange: (v: string) => void
  multiline?: boolean
}

function EditableField({ label, value, onChange, multiline }: EditableFieldProps): React.ReactElement {
  return (
    <div className="rounded-xl border border-border/30 bg-muted/10 p-5 mb-3">
      <div className="mb-3 text-[10.5px] font-semibold uppercase tracking-wider text-muted-foreground/80">
        {label}
      </div>
      {multiline ? (
        <textarea
          value={value}
          onChange={(e) => onChange(e.target.value)}
          rows={4}
          className="w-full rounded-lg border border-border/50 bg-background px-3.5 py-2.5 text-[12.5px] text-foreground/85 resize-y focus:outline-none focus:ring-2 focus:ring-ring/20 focus:border-ring/40 transition-all duration-200"
        />
      ) : (
        <input
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="w-full rounded-lg border border-border/50 bg-background px-3.5 py-2 text-[12.5px] text-foreground/85 focus:outline-none focus:ring-2 focus:ring-ring/20 focus:border-ring/40 transition-all duration-200"
        />
      )}
    </div>
  )
}

export interface SkillDetailProps {
  skill: UnifiedSkill | null
  forking: boolean
  onToggleEnabled: (skill: UnifiedSkill, next: boolean) => void
  onRequestDelete: (skill: UnifiedSkill) => void
  onFork: (name: string) => void
  onLifecycleChanged?: () => void
}

export function SkillDetail({
  skill,
  forking,
  onToggleEnabled,
  onRequestDelete,
  onFork,
  onLifecycleChanged,
}: SkillDetailProps): React.ReactElement {
  const [showTimeline, setShowTimeline] = React.useState(false)
  const [lifecycleUpdating, setLifecycleUpdating] = React.useState(false)
  const [editing, setEditing] = React.useState(false)
  const [saving, setSaving] = React.useState(false)

  // Edit form state
  const [editContext, setEditContext] = React.useState('')
  const [editPrinciples, setEditPrinciples] = React.useState('')
  const [editSteps, setEditSteps] = React.useState('')
  const [editPitfalls, setEditPitfalls] = React.useState('')
  const [editCategory, setEditCategory] = React.useState('')
  const [editTags, setEditTags] = React.useState('')
  const [editValidationHint, setEditValidationHint] = React.useState('')

  // 切换选中技能时收起演化历史 + 退出编辑模式
  React.useEffect(() => {
    setShowTimeline(false)
    setEditing(false)
  }, [skill?.id])

  const enterEditMode = (): void => {
    if (!skill || skill.kind !== 'learned') return
    setEditContext(skill.raw.context || '')
    setEditPrinciples(skill.raw.principles || '')
    setEditSteps(skill.raw.steps || '')
    setEditPitfalls(skill.raw.pitfalls || '')
    setEditCategory(skill.raw.category || '')
    setEditTags((skill.raw.tags ?? []).join(', '))
    setEditValidationHint(skill.raw.validationHint || '')
    setEditing(true)
  }

  const cancelEdit = (): void => {
    setEditing(false)
  }

  const handleSave = async (): Promise<void> => {
    if (!skill || skill.kind !== 'learned') return
    setSaving(true)
    try {
      const tagsArray = editTags
        .split(',')
        .map((t) => t.trim())
        .filter(Boolean)
      await updateLearnedSkill({
        nodeId: skill.raw.id,
        context: editContext,
        principles: editPrinciples,
        steps: editSteps,
        pitfalls: editPitfalls,
        category: editCategory || undefined,
        tags: tagsArray.length > 0 ? tagsArray : undefined,
        validationHint: editValidationHint || undefined,
      })
      toast.success('技能已更新')
      setEditing(false)
      onLifecycleChanged?.() // trigger refetch
    } catch (err) {
      toast.error(`更新失败: ${err}`)
    } finally {
      setSaving(false)
    }
  }

  const handleLifecycleChange = async (nodeId: string, newLifecycle: 'draft' | 'promoted' | 'deprecated') => {
    setLifecycleUpdating(true)
    try {
      await setSkillLifecycle(nodeId, newLifecycle)
      toast.success(`技能状态已更新为 "${LIFECYCLE_BADGE[newLifecycle]?.label}"`)
      onLifecycleChanged?.()
    } catch (err) {
      toast.error(`更新失败: ${err}`)
    } finally {
      setLifecycleUpdating(false)
    }
  }

  if (!skill) {
    return (
      <div className="flex h-full items-center justify-center bg-content-area">
        <div className="flex flex-col items-center gap-4 text-center px-8">
          <div className="relative">
            <div className="absolute inset-0 rounded-2xl bg-primary/5 blur-xl" />
            <div className="relative size-16 rounded-2xl bg-muted/40 border border-border/30 flex items-center justify-center">
              <BookOpen className="size-7 text-muted-foreground/40" />
            </div>
          </div>
          <div className="space-y-1.5">
            <div className="text-[14px] font-medium text-foreground/60">选择左侧一个技能查看详情</div>
            <div className="text-[11.5px] text-muted-foreground/60 max-w-[240px]">
              点击左侧列表中的技能，查看场景、原则、步骤等详细信息
            </div>
          </div>
        </div>
      </div>
    )
  }

  // ── 分区导航锚点 ──
  const isLearned = skill.kind === 'learned'
  const navSections = isLearned
    ? [
        skill.raw.context && { id: 'section-context', label: '场景' },
        skill.raw.principles && { id: 'section-principles', label: '原则' },
        skill.raw.steps && { id: 'section-steps', label: '步骤' },
        skill.raw.pitfalls && { id: 'section-pitfalls', label: '陷阱' },
        skill.raw.category && { id: 'section-category', label: '类别' },
        (skill.raw.tags && skill.raw.tags.length > 0) && { id: 'section-tags', label: '标签' },
        skill.raw.validationHint && { id: 'section-validation', label: '验证' },
      ].filter(Boolean) as { id: string; label: string }[]
    : skill.raw.description
      ? [{ id: 'section-desc', label: '描述' }]
      : []

  const scrollToSection = (id: string) => {
    const el = document.getElementById(id)
    if (el) {
      el.scrollIntoView({ behavior: 'smooth', block: 'start' })
    }
  }

  return (
    <div className="h-full overflow-y-auto bg-content-area">
      {/* Sticky 头部 */}
      <div className="sticky top-0 z-10 bg-background/90 backdrop-blur-xl border-b border-border/30 px-7 pt-7 pb-5">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="text-[18px] font-semibold text-foreground truncate tracking-tight">{skill.name}</span>
              <span className="rounded-full bg-primary/8 border border-primary/20 px-2.5 py-0.5 text-[10px] font-medium text-primary">
                {skill.kind === 'learned' ? '学得' : '内置'}
              </span>
              {skill.kind === 'learned' && (() => {
                const lc = skill.raw.lifecycle || 'promoted'
                const badge = LIFECYCLE_BADGE[lc]
                return badge ? (
                  <span className={`rounded-full border px-2 py-0.5 text-[10px] ${badge.className}`}>
                    {badge.label}
                  </span>
                ) : null
              })()}
              {skill.kind === 'builtin' && skill.raw.provenance && (
                <span
                  className={`rounded-full border px-2 py-0.5 text-[10px] ${PROVENANCE_BADGE[skill.raw.provenance].className}`}
                >
                  {PROVENANCE_BADGE[skill.raw.provenance].label}
                </span>
              )}
            </div>
            {skill.kind === 'learned' ? (
              <div className="mt-1.5 text-[11px] text-muted-foreground tabular-nums">
                使用 {skill.raw.usageCount} 次{skill.raw.citedCount ? ` · 引用 ${skill.raw.citedCount} 次` : ''} · 创建于 {formatDate(skill.raw.createdAt)}
              </div>
            ) : (
              <div className="mt-1.5 text-[11px] text-muted-foreground">
                v{skill.raw.version} · {skill.raw.author} · {skill.raw.category || '未分类'}
              </div>
            )}
          </div>
          <div className="flex items-center gap-2 shrink-0">
            {skill.kind === 'learned' && !editing && (
              <Button
                size="sm"
                variant="outline"
                onClick={enterEditMode}
                className="h-8 px-2.5 text-[12px] gap-1"
              >
                <Pencil className="size-3" />
                编辑
              </Button>
            )}
            {skill.kind === 'builtin' && skill.raw.provenance === 'bundled' && (
              <Button
                size="sm"
                variant="outline"
                disabled={forking}
                onClick={() => onFork(skill.name)}
                className="h-8 px-2.5 text-[12px]"
              >
                {forking ? 'Fork 中…' : 'Fork 到我的'}
              </Button>
            )}
            <span className="text-[11px] text-muted-foreground">Agent 可调用</span>
            <Switch
              checked={skill.enabled}
              onCheckedChange={(next) => onToggleEnabled(skill, next)}
              aria-label="Agent 可调用"
            />
          </div>
        </div>
      </div>

      {/* 分区导航（仅在有多个分区时显示） */}
      {navSections.length > 1 && (
        <div className="sticky top-[73px] z-[9] bg-background/85 backdrop-blur-lg border-b border-border/20 px-7 py-2 flex items-center gap-1 overflow-x-auto scrollbar-none">
          <Hash className="size-3 text-muted-foreground/50 shrink-0" />
          {navSections.map((sec) => (
            <button
              key={sec.id}
              type="button"
              onClick={() => scrollToSection(sec.id)}
              className="shrink-0 px-2.5 py-1 text-[11px] font-medium rounded-full text-muted-foreground hover:text-foreground hover:bg-muted/40 transition-colors duration-150"
            >
              {sec.label}
            </button>
          ))}
        </div>
      )}

      {/* 主体 */}
      <div className="px-7 py-5">
        {skill.kind === 'builtin' ? (
          <div className="space-y-3 text-[12.5px] text-foreground/90">
            {skill.raw.description && (
              <Section label="描述" id="section-desc">
                <p className="leading-relaxed text-muted-foreground">{skill.raw.description}</p>
              </Section>
            )}
          </div>
        ) : (
          <div className="space-y-3 text-[12.5px] text-foreground/90">
            {/* Lifecycle 操作栏 */}
            <div className="flex items-center gap-2 rounded-xl border border-border/30 bg-muted/10 px-4 py-3 mb-5">
              <span className="text-[11px] text-muted-foreground mr-auto font-medium">生命周期</span>
              {(() => {
                const lc = skill.raw.lifecycle || 'promoted'
                if (lc === 'draft') return (
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={lifecycleUpdating}
                    onClick={() => handleLifecycleChange(skill.raw.id, 'promoted')}
                    className="h-7 px-2.5 text-[11px] gap-1"
                  >
                    <ArrowUp className="size-3" />
                    晋升
                  </Button>
                )
                if (lc === 'promoted') return (
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={lifecycleUpdating}
                    onClick={() => handleLifecycleChange(skill.raw.id, 'deprecated')}
                    className="h-7 px-2.5 text-[11px] gap-1"
                  >
                    <Archive className="size-3" />
                    弃用
                  </Button>
                )
                if (lc === 'deprecated') return (
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={lifecycleUpdating}
                    onClick={() => handleLifecycleChange(skill.raw.id, 'promoted')}
                    className="h-7 px-2.5 text-[11px] gap-1"
                  >
                    <RotateCcw className="size-3" />
                    恢复
                  </Button>
                )
                return null
              })()}
            </div>

            <div className="flex items-center justify-between mb-5">
              <button
                type="button"
                onClick={() => setShowTimeline((v) => !v)}
                className={cn(
                  'flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-[11.5px] transition-all duration-200',
                  showTimeline
                    ? 'border-border bg-muted/60 text-foreground shadow-sm'
                    : 'border-border/40 text-muted-foreground hover:border-border hover:text-foreground hover:bg-muted/30',
                )}
              >
                <History className="size-3.5" />
                演化历史
              </button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => onRequestDelete(skill)}
                className="h-8 px-2.5 text-[12px] text-destructive hover:text-destructive"
              >
                删除
              </Button>
            </div>
            {showTimeline ? (
              <SkillEvolutionTab skillId={skill.raw.id} />
            ) : editing ? (
              /* ── Edit mode ─────────────────────────────── */
              <div className="space-y-0">
                <EditableField label="场景" value={editContext} onChange={setEditContext} multiline />
                <EditableField label="原则" value={editPrinciples} onChange={setEditPrinciples} multiline />
                <EditableField label="步骤" value={editSteps} onChange={setEditSteps} multiline />
                <EditableField label="陷阱" value={editPitfalls} onChange={setEditPitfalls} multiline />

                <div className="rounded-xl border border-border/30 bg-muted/10 p-5 mb-3">
                  <div className="mb-3 text-[10.5px] font-semibold uppercase tracking-wider text-muted-foreground/80">
                    类别
                  </div>
                  <select
                    value={editCategory}
                    onChange={(e) => setEditCategory(e.target.value)}
                    className="w-full rounded-lg border border-border/50 bg-background px-3.5 py-2 text-[12.5px] text-foreground/85 focus:outline-none focus:ring-2 focus:ring-ring/20 focus:border-ring/40 transition-all duration-200"
                  >
                    <option value="">未分类</option>
                    {CATEGORY_OPTIONS.map((opt) => (
                      <option key={opt.value} value={opt.value}>{opt.label}</option>
                    ))}
                  </select>
                </div>

                <EditableField label="标签 (逗号分隔)" value={editTags} onChange={setEditTags} />
                <EditableField label="验证方法" value={editValidationHint} onChange={setEditValidationHint} />

                <div className="flex items-center gap-2.5 pt-2">
                  <Button
                    size="sm"
                    onClick={() => void handleSave()}
                    disabled={saving}
                    className="h-8 px-3.5 text-[12px] gap-1"
                  >
                    <Save className="size-3" />
                    {saving ? '保存中…' : '保存'}
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={cancelEdit}
                    disabled={saving}
                    className="h-8 px-3.5 text-[12px] gap-1"
                  >
                    <X className="size-3" />
                    取消
                  </Button>
                </div>
              </div>
            ) : (
              /* ── View mode ─────────────────────────────── */
              <>
                {skill.raw.context && (
                  <Section label="场景" id="section-context">
                    <p className="leading-relaxed text-muted-foreground">{skill.raw.context}</p>
                  </Section>
                )}
                {skill.raw.principles && (
                  <Section label="原则" id="section-principles">
                    <MarkdownBlock text={skill.raw.principles} />
                  </Section>
                )}
                {skill.raw.steps && (
                  <Section label="步骤" id="section-steps">
                    <MarkdownBlock text={skill.raw.steps} />
                  </Section>
                )}
                {skill.raw.pitfalls && (
                  <Section label="陷阱" id="section-pitfalls">
                    <MarkdownBlock text={skill.raw.pitfalls} />
                  </Section>
                )}
                {skill.raw.category && (
                  <Section label="类别" id="section-category">
                    <p className="leading-relaxed text-muted-foreground">{skill.raw.category}</p>
                  </Section>
                )}
                {skill.raw.tags && skill.raw.tags.length > 0 && (
                  <Section label="标签" id="section-tags">
                    <div className="flex flex-wrap gap-1.5">
                      {skill.raw.tags.map((t) => (
                        <span key={t} className="rounded-full bg-accent/15 border border-accent/30 px-2.5 py-0.5 text-[10.5px] text-foreground/80">
                          {t}
                        </span>
                      ))}
                    </div>
                  </Section>
                )}
                {skill.raw.validationHint && (
                  <Section label="验证方法" id="section-validation">
                    <p className="leading-relaxed text-muted-foreground">{skill.raw.validationHint}</p>
                  </Section>
                )}
              </>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
