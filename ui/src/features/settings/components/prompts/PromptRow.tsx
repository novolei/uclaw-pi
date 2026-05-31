// One row in the PromptSettings list — collapsed header + expanded view/edit
// form + version-history panel. Split out of `PromptSettings` during the P3
// move; presentation only (all side effects come in as props from
// `usePromptSettings`). Never calls IPC directly.
import * as React from 'react'
import { Trash2, Check, Pencil, Star, StarOff, Loader2, History, ChevronDown, ChevronUp } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { SystemPrompt, SystemPromptVersion } from '@/lib/chat-types'
import { BUILTIN_DEFAULT_ID } from '@/lib/chat-types'

interface PromptRowProps {
  prompt: SystemPrompt
  isDefault: boolean
  isExpanded: boolean
  isEditing: boolean
  saving: boolean
  editName: string
  editContent: string
  showVersionsId: string | null
  versions: SystemPromptVersion[]
  loadingVersions: boolean
  onToggleExpand: () => void
  onSetEditName: (v: string) => void
  onSetEditContent: (v: string) => void
  onSaveEdit: () => void
  onCancelEdit: () => void
  onSetDefault: (id: string) => void
  onStartEdit: (p: SystemPrompt) => void
  onDelete: (id: string, name: string) => void
  onToggleVersions: (id: string) => void
}

export function PromptRow({
  prompt: p,
  isDefault,
  isExpanded,
  isEditing,
  saving,
  editName,
  editContent,
  showVersionsId,
  versions,
  loadingVersions,
  onToggleExpand,
  onSetEditName,
  onSetEditContent,
  onSaveEdit,
  onCancelEdit,
  onSetDefault,
  onStartEdit,
  onDelete,
  onToggleVersions,
}: PromptRowProps): React.ReactElement {
  const isBuiltin = p.id === BUILTIN_DEFAULT_ID

  return (
    <div
      className={cn(
        'rounded border transition-colors',
        isDefault ? 'border-primary/30 bg-primary/5' : 'border-border/50 bg-background',
      )}
    >
      {/* Header row */}
      <div
        className="flex items-center gap-2 px-3 py-2 cursor-pointer"
        onClick={onToggleExpand}
      >
        <span className={cn(
          'flex-1 text-xs font-medium truncate',
          isDefault ? 'text-primary' : 'text-foreground',
        )}>
          {p.name}
          {isBuiltin && <span className="ml-1.5 text-[10px] text-muted-foreground">(内置)</span>}
        </span>
        <span className="text-[10px] text-muted-foreground whitespace-nowrap">
          {p.content.length} 字符
        </span>
        {isDefault && (
          <span className="text-[10px] text-primary font-medium whitespace-nowrap">
            当前使用
          </span>
        )}
      </div>

      {/* Expanded content / edit form */}
      {isExpanded && (
        <div className="px-3 pb-3 pt-0 border-t border-border/30">
          {isEditing ? (
            <div className="space-y-2 mt-2">
              <input
                type="text"
                value={editName}
                onChange={(e) => onSetEditName(e.target.value)}
                className="w-full rounded border border-border/50 bg-background px-2 py-1 text-xs outline-none focus:border-border"
              />
              <textarea
                value={editContent}
                onChange={(e) => onSetEditContent(e.target.value)}
                rows={6}
                className="w-full rounded border border-border/50 bg-background px-2 py-1 text-xs font-mono outline-none focus:border-border resize-y"
              />
              <div className="flex items-center gap-2">
                <Button size="sm" onClick={onSaveEdit} disabled={saving || !editName.trim()}>
                  {saving ? <Loader2 className="size-3 animate-spin mr-1" /> : <Check className="size-3 mr-1" />}
                  保存
                </Button>
                <Button size="sm" variant="ghost" onClick={onCancelEdit}>取消</Button>
              </div>
            </div>
          ) : (
            <>
              <pre className="mt-2 text-[11px] font-mono text-muted-foreground whitespace-pre-wrap max-h-40 overflow-y-auto">
                {p.content}
              </pre>
              <div className="mt-2 flex items-center gap-1.5 flex-wrap">
                {!isDefault && (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={(e) => { e.stopPropagation(); onSetDefault(p.id) }}
                    title="设为默认"
                  >
                    <StarOff className="size-3" />
                  </Button>
                )}
                {isDefault && (
                  <span className="px-2 py-1 text-[11px] text-primary inline-flex items-center gap-1">
                    <Star className="size-3" />
                    当前默认
                  </span>
                )}
                {!isBuiltin && (
                  <>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={(e) => { e.stopPropagation(); onStartEdit(p) }}
                    >
                      <Pencil className="size-3" />
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={(e) => { e.stopPropagation(); onDelete(p.id, p.name) }}
                      className="text-destructive hover:text-destructive"
                    >
                      <Trash2 className="size-3" />
                    </Button>
                  </>
                )}
                <div className="flex-1" />
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={(e) => { e.stopPropagation(); onToggleVersions(p.id) }}
                  className="text-[10px] text-muted-foreground hover:text-foreground"
                >
                  <History className="size-3 mr-1" />
                  版本历史
                  {showVersionsId === p.id ? <ChevronUp className="size-3 ml-0.5" /> : <ChevronDown className="size-3 ml-0.5" />}
                </Button>
              </div>

              {/* Version history list */}
              {showVersionsId === p.id && (
                <div className="mt-2 pt-2 border-t border-border/30">
                  {loadingVersions ? (
                    <div className="flex items-center justify-center py-2">
                      <Loader2 className="size-3 animate-spin text-muted-foreground" />
                    </div>
                  ) : versions.length === 0 ? (
                    <p className="text-[11px] text-muted-foreground text-center py-2">暂无版本记录</p>
                  ) : (
                    <div className="space-y-1.5 max-h-48 overflow-y-auto">
                      {versions.map((v, idx) => (
                        <div key={v.id} className="rounded border border-border/30 bg-muted/20 p-2">
                          <div className="flex items-center justify-between mb-1">
                            <span className="text-[10px] font-medium text-foreground">
                              {idx === 0 ? '当前版本' : `版本 ${versions.length - idx}`}
                            </span>
                            <span className="text-[9px] text-muted-foreground">
                              {new Date(v.createdAt).toLocaleString('zh-CN', {
                                month: '2-digit',
                                day: '2-digit',
                                hour: '2-digit',
                                minute: '2-digit',
                              })}
                            </span>
                          </div>
                          <pre className="text-[10px] font-mono text-muted-foreground whitespace-pre-wrap max-h-20 overflow-y-auto">
                            {v.content}
                          </pre>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}
            </>
          )}
        </div>
      )}
    </div>
  )
}
