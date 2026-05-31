// BrowserRuntimeSettings presentation helpers — pure label/badge/timestamp
// formatting, no IPC / side effects. Moved verbatim out of the legacy
// `legacy settings/BrowserRuntimeSettings.tsx` during the P3 split
// (docs/superpowers/plans/2026-05-31-frontend-settings-feature-migration.md).

import * as React from 'react'
import type { Badge } from '@/components/ui/badge'
import type { deriveBrowserRuntimeSettingsViewModel } from '@/lib/browser-runtime/browser-runtime-settings'
import type {
  BrowserIdentityActiveTaskSummary,
  BrowserIdentityProfileSummary,
  BrowserIdentityStatusReport,
} from '@/lib/tauri-bridge'

type BadgeVariant = React.ComponentProps<typeof Badge>['variant']

export function identityStatusLabel(
  status: BrowserIdentityStatusReport | undefined,
): string {
  if (!status) return '未检查'
  if (status.authorizedCount > 0) return '已授权'
  if (status.revokedCount > 0) return '已撤销'
  return '未连接'
}

export function identityStatusDetail(
  status: BrowserIdentityStatusReport | undefined,
): string {
  if (!status) return '等待身份状态。'
  if (status.authorizedCount > 0) {
    return `${status.authorizedCount} 个可用身份，${status.revokedCount} 个已撤销。`
  }
  if (status.revokedCount > 0) {
    return `${status.revokedCount} 个已撤销身份。`
  }
  return '未连接浏览器身份。'
}

export function identityBadgeVariant(
  status: BrowserIdentityStatusReport | undefined,
): BadgeVariant {
  if (!status) return 'outline'
  if (status.authorizedCount > 0) return 'default'
  if (status.revokedCount > 0) return 'secondary'
  return 'outline'
}

export function identityCountLabel(
  status: BrowserIdentityStatusReport | undefined,
): string {
  if (!status) return '未检查'
  return `${status.authorizedCount} 可用 / ${status.revokedCount} 已撤销`
}

export function latestIdentityLastUsedLabel(
  status: BrowserIdentityStatusReport | undefined,
): string {
  if (!status) return '未检查'
  const latest = Math.max(
    ...status.profiles
      .map((profile) => profile.lastUsedAtMs ?? 0)
      .filter((timestamp) => timestamp > 0),
  )
  if (!Number.isFinite(latest) || latest <= 0) return '未知'
  return formatIdentityTimestamp(latest)
}

export function identityActiveTaskLabel(
  status: BrowserIdentityStatusReport | undefined,
): string {
  if (!status) return '未检查'
  return `${status.activeTaskCount} 个任务`
}

export function identityProfileStatusLabel(profile: BrowserIdentityProfileSummary): string {
  if (profile.revoked) return '已撤销'
  if (profile.status === 'live') return '可用'
  if (profile.status === 'stale') return '需刷新'
  return '未知'
}

export function identityTaskStatusLabel(
  status: BrowserIdentityActiveTaskSummary['status'],
): string {
  switch (status) {
    case 'running':
      return '运行中'
    case 'completed':
      return '已完成'
    case 'failed':
      return '失败'
    case 'stopped':
      return '已停止'
    case 'needs_user_intervention':
      return '等待用户'
    case 'paused_waiting_for_browser_runtime':
      return '等待运行时'
    case 'paused_checkpointed':
      return '已检查点暂停'
    default:
      return status
  }
}

export function identityProviderLabel(
  provider: BrowserIdentityProfileSummary['provider'],
): string {
  switch (provider) {
    case 'system_chrome':
      return 'System Chrome'
    case 'playwright':
      return 'Playwright'
    case 'browser_use':
      return 'Browser Use'
    case 'manual_import':
      return 'Manual import'
    default:
      return provider
  }
}

export function identityScopeLabel(scope: BrowserIdentityProfileSummary['scope']): string {
  switch (scope) {
    case 'global':
      return 'Global'
    case 'workspace':
      return 'Workspace'
    case 'session':
      return 'Session'
    default:
      return scope
  }
}

export function formatIdentityTimestamp(timestampMs: number | null): string {
  if (!timestampMs) return '未知'
  return new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(timestampMs))
}

export function badgeVariant(
  kind: ReturnType<typeof deriveBrowserRuntimeSettingsViewModel>['statusKind'],
): BadgeVariant {
  if (kind === 'ready') return 'default'
  if (kind === 'blocked') return 'destructive'
  if (kind === 'attention' || kind === 'deferred') return 'secondary'
  return 'outline'
}
