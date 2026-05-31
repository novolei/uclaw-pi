// Owns the WorkspaceSkillTagsEditor data: the active workspace (read from the
// workspace atoms), the loaded tag list, the loading/saving flags, and the
// load/persist/add/remove actions. Extracted out of the component during the
// features/settings migration (code-organization ADR 2026-05-31). The
// `@/atoms/workspace` atom reads + the typed `@/lib/tauri-bridge`
// get/setWorkspaceSkillTags helpers stay in the hook (precedent:
// useChannelSettings keeps its typed provider IPC in the hook too). Server-side
// tag normalization is echoed back via the setWorkspaceSkillTags return.
// Behavior preserved verbatim; the component keeps only the draft input state.
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { toast } from 'sonner'
import { activeWorkspaceIdAtom, workspacesAtom } from '@/atoms/workspace'
import { getWorkspaceSkillTags, setWorkspaceSkillTags } from '@/lib/tauri-bridge'

export function useWorkspaceSkillTags() {
  const activeId = useAtomValue(activeWorkspaceIdAtom)
  const workspaces = useAtomValue(workspacesAtom)
  const activeWorkspace = React.useMemo(
    () => workspaces.find((w) => w.id === activeId),
    [workspaces, activeId],
  )

  const [tags, setTags] = React.useState<string[]>([])
  const [loading, setLoading] = React.useState(false)
  const [saving, setSaving] = React.useState(false)

  React.useEffect(() => {
    if (!activeId) {
      setTags([])
      return
    }
    setLoading(true)
    getWorkspaceSkillTags(activeId)
      .then(setTags)
      .catch((e) => toast.error('读取标签失败', { description: String(e) }))
      .finally(() => setLoading(false))
  }, [activeId])

  const persist = React.useCallback(
    async (next: string[]) => {
      if (!activeId) return
      setSaving(true)
      try {
        const normalized = await setWorkspaceSkillTags(activeId, next)
        setTags(normalized)
      } catch (e) {
        toast.error('保存失败', { description: String(e) })
      } finally {
        setSaving(false)
      }
    },
    [activeId],
  )

  // Add a tag from the raw draft string. Trim + case-insensitive dedup, then
  // persist (behavior preserved from the pre-migration addTag). The component
  // always clears its draft after calling this (mirrors the original).
  const addTag = React.useCallback(
    (raw: string) => {
      const trimmed = raw.trim()
      if (!trimmed) return
      if (tags.includes(trimmed.toLowerCase())) return
      void persist([...tags, trimmed])
    },
    [tags, persist],
  )

  const removeTag = React.useCallback(
    (tag: string) => {
      void persist(tags.filter((t) => t !== tag))
    },
    [tags, persist],
  )

  return { activeId, activeWorkspace, tags, loading, saving, addTag, removeTag }
}
