/**
 * WorkspaceSkillTagsEditor — chip input for per-workspace skill scoping (V19+).
 *
 * Empty tag set = no filter (every enabled skill stays in the manifest).
 * Non-empty enables the intersection rule:
 *   - skills without tags stay global (always included)
 *   - skills with tags need at least one match with the workspace's tags
 *
 * Tag normalization happens server-side (trim + lowercase + dedup), and
 * the `setWorkspaceSkillTags` IPC returns the normalized list so this
 * component echoes back what's actually stored.
 */
import * as React from 'react'
import { Button } from '@/components/ui/button'
import { X } from 'lucide-react'
import { useWorkspaceSkillTags } from '../hooks/useWorkspaceSkillTags'

export function WorkspaceSkillTagsEditor(): React.ReactElement | null {
  // Tag data + load/persist/add/remove actions live in the hook; the component
  // keeps only the draft input state (code-organization ADR 2026-05-31).
  const { activeId, activeWorkspace, tags, loading, saving, addTag, removeTag } =
    useWorkspaceSkillTags()
  const [draft, setDraft] = React.useState('')

  // Always clear the draft after an add attempt (mirrors the pre-migration
  // behavior, including the duplicate-tag case).
  const submitDraft = React.useCallback(() => {
    addTag(draft)
    setDraft('')
  }, [addTag, draft])

  if (!activeId) {
    return (
      <div className="text-xs text-muted-foreground py-2">
        请先选择一个工作区。
      </div>
    )
  }

  return (
    <div className="space-y-3">
      <div className="text-xs text-muted-foreground">
        当前工作区：<span className="font-medium text-foreground/80">
          {activeWorkspace?.name ?? activeId}
        </span>
        {tags.length === 0 && (
          <span className="ml-2 text-muted-foreground/60">
            (未设标签 = 所有 Skill 都可见，默认)
          </span>
        )}
      </div>

      <div className="flex flex-wrap gap-1.5 items-center min-h-[28px]">
        {tags.map((tag) => (
          <span
            key={tag}
            className="inline-flex items-center gap-1 text-xs px-2 py-0.5 rounded-full border bg-primary/10 text-primary border-primary/20"
          >
            {tag}
            <button
              type="button"
              onClick={() => removeTag(tag)}
              disabled={saving}
              className="hover:text-primary/70 disabled:opacity-50"
              aria-label={`移除标签 ${tag}`}
            >
              <X className="size-3" />
            </button>
          </span>
        ))}
        {loading && (
          <span className="text-[10px] text-muted-foreground/60">加载中…</span>
        )}
      </div>

      <div className="flex items-center gap-2">
        <input
          type="text"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              submitDraft()
            } else if (e.key === ',') {
              e.preventDefault()
              submitDraft()
            }
          }}
          placeholder="输入标签，回车或逗号添加"
          disabled={saving}
          className="flex-1 text-xs px-2 py-1 rounded border border-border bg-background focus:outline-none focus:ring-1 focus:ring-primary disabled:opacity-50"
        />
        <Button size="sm" variant="outline" onClick={submitDraft} disabled={saving || !draft.trim()}>
          添加
        </Button>
      </div>
    </div>
  )
}
