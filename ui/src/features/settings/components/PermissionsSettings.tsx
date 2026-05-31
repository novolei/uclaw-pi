/**
 * PermissionsSettings — Settings → 工具权限 tab.
 *
 * Thin shell composing three cards (all state + IPC live in the
 * usePermissionsSettings hook; code-organization ADR 2026-05-31):
 *   1. GlobalAllowBlockLists — legacy whole-tool whitelist + blocklist from
 *      `safety_policy.json`. Most users should NOT add to allow here:
 *      whitelisting `bash` means every `rm -rf` auto-passes. Use pattern
 *      rules (section 2) for granular trust.
 *   2. PermissionRulesSection — V14 session + pattern rules (the granular tier).
 *   3. PermissionAuditLog — most-recent decisions across all sessions.
 * Plus WorkspaceSandboxSettings (now a migrated sibling under features/settings/).
 *
 * Live update: re-fetch on mount; manual refresh button. Split out of the
 * 328-line legacy settings/PermissionsSettings.tsx during P3a.
 */
import * as React from 'react'
import { WorkspaceSandboxSettings } from './WorkspaceSandboxSettings'
import { usePermissionsSettings } from '../hooks/usePermissionsSettings'
import { GlobalAllowBlockLists } from './permissions/GlobalAllowBlockLists'
import { PermissionRulesSection } from './permissions/PermissionRulesSection'
import { PermissionAuditLog } from './permissions/PermissionAuditLog'

export function PermissionsSettings(): React.ReactElement {
  const {
    rules,
    audit,
    loading,
    draft, setDraft,
    allowList,
    blockList,
    refetch,
    onRemoveAllow,
    onUnblock,
    onAddRule,
    onDelete,
  } = usePermissionsSettings()

  return (
    <div className="space-y-6 pb-8">
      <GlobalAllowBlockLists
        allowList={allowList}
        blockList={blockList}
        loading={loading}
        onRefresh={refetch}
        onRemoveAllow={onRemoveAllow}
        onUnblock={onUnblock}
      />

      <PermissionRulesSection
        rules={rules}
        loading={loading}
        draft={draft}
        setDraft={setDraft}
        onAddRule={onAddRule}
        onDelete={onDelete}
      />

      <PermissionAuditLog audit={audit} loading={loading} />

      <div className="mt-8 pt-6 border-t">
        <h2 className="text-base font-semibold mb-3">工作区沙箱</h2>
        <WorkspaceSandboxSettings />
      </div>
    </div>
  )
}
