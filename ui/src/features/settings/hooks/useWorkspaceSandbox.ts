// Owns the WorkspaceSandboxSettings data: the global ("always allowed") path list,
// the per-session temporary path list, and the add/remove/promote actions.
// Extracted out of the component during the features/settings migration
// (code-organization ADR 2026-05-31). The current-session atom read +
// the typed `@/lib/tauri-bridge` sandbox-path helpers stay in the hook (precedent:
// useWorkspaceSkillTags). Behavior preserved verbatim — same refresh order, the
// same console.error on refresh failure, and the same toast on action failure.
import * as React from 'react'
import { useAtomValue } from 'jotai'
import { toast } from 'sonner'
import {
  listAlwaysAllowedPaths,
  addAlwaysAllowedPath,
  removeAlwaysAllowedPath,
  listSessionAllowedPaths,
  promoteSessionPathToGlobal,
  openFolderDialog,
} from '@/lib/tauri-bridge'
import { currentAgentSessionIdAtom } from '@/atoms/agent-atoms'

export function useWorkspaceSandbox() {
  const sessionId = useAtomValue(currentAgentSessionIdAtom)
  const [global, setGlobal] = React.useState<string[]>([])
  const [session, setSession] = React.useState<string[]>([])

  const refreshGlobal = React.useCallback(async () => {
    try { setGlobal(await listAlwaysAllowedPaths()) } catch (err) { console.error('[sandbox]', err) }
  }, [])

  const refreshSession = React.useCallback(async () => {
    if (!sessionId) { setSession([]); return }
    try { setSession(await listSessionAllowedPaths(sessionId)) } catch (err) { console.error('[sandbox]', err) }
  }, [sessionId])

  React.useEffect(() => { void refreshGlobal() }, [refreshGlobal])
  React.useEffect(() => { void refreshSession() }, [refreshSession])

  const handleAdd = React.useCallback(async () => {
    try {
      const picked = await openFolderDialog()
      if (!picked) return
      await addAlwaysAllowedPath(picked.path)
      await refreshGlobal()
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`添加失败: ${msg}`)
    }
  }, [refreshGlobal])

  const handleRemove = React.useCallback(async (p: string) => {
    try {
      await removeAlwaysAllowedPath(p)
      await refreshGlobal()
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`删除失败: ${msg}`)
    }
  }, [refreshGlobal])

  const handlePromote = React.useCallback(async (p: string) => {
    if (!sessionId) return
    try {
      await promoteSessionPathToGlobal(sessionId, p)
      await refreshGlobal()
      await refreshSession()
      toast.success('已升级为永久允许')
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`升级失败: ${msg}`)
    }
  }, [sessionId, refreshGlobal, refreshSession])

  return { sessionId, global, session, handleAdd, handleRemove, handlePromote }
}
