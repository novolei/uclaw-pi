// Owns all PromptSettings side effects — load of the system-prompt config + the
// CRUD handlers (set-default / delete / edit / create) + version-history fetch.
// Extracted out of the component during the P3 split; all IPC goes through
// `settingsBridge` (no `@tauri-apps/api` here). The bridge wrappers preserve the
// legacy `.catch(...)` fallbacks, so error behavior (toasts, console logging,
// optimistic local state updates) is identical to before the move.
import * as React from 'react'
import { toast } from 'sonner'
import type { SystemPrompt, SystemPromptConfig, SystemPromptVersion } from '@/lib/chat-types'
import { BUILTIN_DEFAULT_ID } from '@/lib/chat-types'
import { settingsBridge } from '../../../lib/bridge/settings'

export function usePromptSettings() {
  const [config, setConfig] = React.useState<SystemPromptConfig | null>(null)
  const [loading, setLoading] = React.useState(true)
  const [expandedId, setExpandedId] = React.useState<string | null>(null)
  const [editingId, setEditingId] = React.useState<string | null>(null)
  const [editName, setEditName] = React.useState('')
  const [editContent, setEditContent] = React.useState('')
  const [showNewForm, setShowNewForm] = React.useState(false)
  const [newName, setNewName] = React.useState('')
  const [newContent, setNewContent] = React.useState('')
  const [saving, setSaving] = React.useState(false)
  const [showVersionsId, setShowVersionsId] = React.useState<string | null>(null)
  const [versions, setVersions] = React.useState<SystemPromptVersion[]>([])
  const [loadingVersions, setLoadingVersions] = React.useState(false)

  const loadConfig = React.useCallback(async () => {
    try {
      const cfg = await settingsBridge.getSystemPromptConfig()
      setConfig(cfg as SystemPromptConfig)
    } catch (e) {
      console.error('[PromptSettings] load failed:', e)
      toast.error('加载提示词配置失败')
    } finally {
      setLoading(false)
    }
  }, [])

  React.useEffect(() => { loadConfig() }, [loadConfig])

  const defaultPromptId = config?.defaultPromptId ?? BUILTIN_DEFAULT_ID

  const handleSetDefault = React.useCallback(async (id: string) => {
    try {
      await settingsBridge.setDefaultPrompt(id)
      setConfig((prev) => prev ? { ...prev, defaultPromptId: id } : prev)
      toast.success('已设为默认提示词')
    } catch (e) {
      console.error('[PromptSettings] setDefault failed:', e)
      toast.error('设置默认提示词失败')
    }
  }, [])

  const handleDelete = React.useCallback(async (id: string, name: string) => {
    if (!confirm(`确定要删除提示词「${name}」吗？`)) return
    try {
      await settingsBridge.deleteSystemPrompt(id)
      setConfig((prev) => prev ? {
        ...prev,
        prompts: prev.prompts.filter((p) => p.id !== id),
        defaultPromptId: prev.defaultPromptId === id ? BUILTIN_DEFAULT_ID : prev.defaultPromptId,
      } : prev)
      toast.success(`已删除「${name}」`)
    } catch (e) {
      console.error('[PromptSettings] delete failed:', e)
      toast.error('删除失败')
    }
  }, [])

  const handleStartEdit = React.useCallback((p: SystemPrompt) => {
    setEditingId(p.id)
    setEditName(p.name)
    setEditContent(p.content)
  }, [])

  const handleCancelEdit = React.useCallback(() => {
    setEditingId(null)
    setEditName('')
    setEditContent('')
  }, [])

  const handleSaveEdit = React.useCallback(async () => {
    if (!editingId || !editName.trim()) return
    setSaving(true)
    try {
      const updated = await settingsBridge.updateSystemPrompt(editingId, { name: editName.trim(), content: editContent })
      setConfig((prev) => prev ? {
        ...prev,
        prompts: prev.prompts.map((p) => p.id === editingId ? { ...p, name: updated.name ?? editName, content: updated.content ?? editContent } : p),
      } : prev)
      setEditingId(null)
      toast.success('提示词已更新')
    } catch (e) {
      console.error('[PromptSettings] update failed:', e)
      toast.error('更新失败')
    } finally {
      setSaving(false)
    }
  }, [editingId, editName, editContent])

  const handleCreate = React.useCallback(async () => {
    if (!newName.trim()) return
    setSaving(true)
    try {
      const created = await settingsBridge.createSystemPrompt({ name: newName.trim(), content: newContent })
      setConfig((prev) => prev ? {
        ...prev,
        prompts: [...prev.prompts, created],
      } : prev)
      setShowNewForm(false)
      setNewName('')
      setNewContent('')
      toast.success('提示词已创建')
    } catch (e) {
      console.error('[PromptSettings] create failed:', e)
      toast.error('创建失败')
    } finally {
      setSaving(false)
    }
  }, [newName, newContent])

  const toggleVersions = React.useCallback(async (promptId: string) => {
    if (showVersionsId === promptId) {
      setShowVersionsId(null)
      setVersions([])
      return
    }
    setShowVersionsId(promptId)
    setLoadingVersions(true)
    try {
      const v = await settingsBridge.getSystemPromptVersions(promptId)
      setVersions(v as SystemPromptVersion[])
    } catch (e) {
      console.error('[PromptSettings] load versions failed:', e)
      toast.error('加载版本历史失败')
    } finally {
      setLoadingVersions(false)
    }
  }, [showVersionsId])

  return {
    config,
    loading,
    defaultPromptId,
    expandedId, setExpandedId,
    editingId,
    editName, setEditName,
    editContent, setEditContent,
    showNewForm, setShowNewForm,
    newName, setNewName,
    newContent, setNewContent,
    saving,
    showVersionsId,
    versions,
    loadingVersions,
    handleSetDefault,
    handleDelete,
    handleStartEdit,
    handleCancelEdit,
    handleSaveEdit,
    handleCreate,
    toggleVersions,
  }
}
