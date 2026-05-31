/**
 * PromptSettings — 系统提示词管理组件 (thin shell).
 *
 * 功能：
 *  - 列出所有系统提示词
 *  - 选择当前使用的提示词（设为默认）
 *  - 创建 / 编辑 / 删除自定义提示词
 *  - 内置提示词（"默认"）不可删除或编辑
 *
 * Migrated + split out of components/settings/PromptSettings.tsx (378 lines)
 * during P3: state + CRUD side effects live in `usePromptSettings` (IPC via
 * `settingsBridge`); each list row is `prompts/PromptRow`.
 */

import * as React from 'react'
import { Plus, Loader2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { PromptRow } from './prompts/PromptRow'
import { usePromptSettings } from '../hooks/usePromptSettings'

export function PromptSettings(): React.ReactElement {
  const {
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
  } = usePromptSettings()

  if (loading) {
    return (
      <div className="flex items-center justify-center py-8">
        <Loader2 className="size-4 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const prompts = config?.prompts ?? []

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium text-foreground">系统提示词</h3>
        <Button
          size="sm"
          variant="outline"
          onClick={() => setShowNewForm((v) => !v)}
          disabled={showNewForm}
        >
          <Plus className="size-3.5 mr-1" />
          新建
        </Button>
      </div>

      {/* Create new form */}
      {showNewForm && (
        <div className="space-y-2 rounded border border-border/50 bg-muted/30 p-3">
          <input
            type="text"
            placeholder="提示词名称"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            className="w-full rounded border border-border/50 bg-background px-2 py-1 text-xs outline-none focus:border-border"
          />
          <textarea
            placeholder="提示词内容…"
            value={newContent}
            onChange={(e) => setNewContent(e.target.value)}
            rows={4}
            className="w-full rounded border border-border/50 bg-background px-2 py-1 text-xs font-mono outline-none focus:border-border resize-y"
          />
          <div className="flex items-center gap-2">
            <Button size="sm" onClick={handleCreate} disabled={saving || !newName.trim()}>
              {saving ? <Loader2 className="size-3 animate-spin mr-1" /> : null}
              创建
            </Button>
            <Button size="sm" variant="ghost" onClick={() => { setShowNewForm(false); setNewName(''); setNewContent('') }}>
              取消
            </Button>
          </div>
        </div>
      )}

      {/* Prompt list */}
      <div className="space-y-1.5">
        {prompts.map((p) => (
          <PromptRow
            key={p.id}
            prompt={p}
            isDefault={p.id === defaultPromptId}
            isExpanded={expandedId === p.id}
            isEditing={editingId === p.id}
            saving={saving}
            editName={editName}
            editContent={editContent}
            showVersionsId={showVersionsId}
            versions={versions}
            loadingVersions={loadingVersions}
            onToggleExpand={() => setExpandedId(expandedId === p.id ? null : p.id)}
            onSetEditName={setEditName}
            onSetEditContent={setEditContent}
            onSaveEdit={handleSaveEdit}
            onCancelEdit={handleCancelEdit}
            onSetDefault={handleSetDefault}
            onStartEdit={handleStartEdit}
            onDelete={handleDelete}
            onToggleVersions={toggleVersions}
          />
        ))}
      </div>

      {prompts.length === 0 && (
        <p className="text-xs text-muted-foreground py-4 text-center">
          暂无提示词
        </p>
      )}
    </div>
  )
}
