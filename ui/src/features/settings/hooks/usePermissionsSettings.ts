// Owns all PermissionsSettings side effects — the rules + audit + safety-policy
// load on mount (and on manual refresh), the draft-rule editor state, and the
// create/delete/remove-allow/unblock mutations. Extracted out of the component
// during the P3a split; all IPC goes through `settingsBridge` (no `@tauri-apps/api`
// / catch-all bridge here). Each mutation re-fetches afterwards, exactly as before
// the move — behavior is identical.
import * as React from 'react'
import type {
  CreatePermissionRuleInput,
  PermissionAuditEntry,
  PermissionRule,
  SafetyPolicyResponse,
} from '@/lib/types'
import { settingsBridge } from '../../../lib/bridge/settings'

export function usePermissionsSettings() {
  const [rules, setRules] = React.useState<PermissionRule[]>([])
  const [audit, setAudit] = React.useState<PermissionAuditEntry[]>([])
  const [policy, setPolicy] = React.useState<SafetyPolicyResponse | null>(null)
  const [loading, setLoading] = React.useState(true)
  const [draft, setDraft] = React.useState<CreatePermissionRuleInput>({
    scope: 'pattern', toolName: '', target: '', mode: 'allow',
  })

  const refetch = React.useCallback(async () => {
    setLoading(true)
    try {
      const [r, a, p] = await Promise.all([
        settingsBridge.listPermissionRules(),
        settingsBridge.listPermissionAudit(undefined, 100),
        settingsBridge.getSafetyPolicy(),
      ])
      setRules(r)
      setAudit(a)
      setPolicy(p)
    } finally {
      setLoading(false)
    }
  }, [])
  React.useEffect(() => { void refetch() }, [refetch])

  const onRemoveAllow = React.useCallback(async (toolName: string) => {
    await settingsBridge.removeAutoApprovedTool({ toolName })
    await refetch()
  }, [refetch])

  const onUnblock = React.useCallback(async (toolName: string) => {
    await settingsBridge.unblockTool({ toolName })
    await refetch()
  }, [refetch])

  const onAddRule = React.useCallback(async () => {
    if (!draft.toolName.trim()) return
    await settingsBridge.createPermissionRule({
      scope: draft.scope,
      sessionId: draft.scope === 'session' ? draft.sessionId : undefined,
      toolName: draft.toolName.trim(),
      target: draft.scope === 'pattern' ? (draft.target?.trim() || undefined) : undefined,
      mode: draft.mode,
    })
    setDraft({ scope: 'pattern', toolName: '', target: '', mode: 'allow' })
    await refetch()
  }, [draft, refetch])

  const onDelete = React.useCallback(async (id: string) => {
    await settingsBridge.deletePermissionRule(id)
    await refetch()
  }, [refetch])

  const allowList = policy?.autoApprovedTools ?? []
  const blockList = policy?.blockedTools ?? []

  return {
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
  }
}
