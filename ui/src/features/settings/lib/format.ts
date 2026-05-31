// Settings-feature presentation helpers — pure formatting, no IPC / side effects.
// Moved verbatim out of the legacy `components/settings/SystemTab.tsx` during the
// P1 split (docs/superpowers/plans/2026-05-31-frontend-settings-feature-migration.md).

import type { ServiceStatus } from './diagnostics-types'

export function formatUptime(secs: number): string {
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  return `${h}h ${m}m`
}

export function formatMemory(mb: number): string {
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`
}

export function serviceStatusLabel(s: ServiceStatus): string {
  const map: Record<string, string> = {
    Running: '运行中', Stopped: '未启动',
    Starting: '启动中', Stopping: '停止中',
  }
  if (s.status === 'Failed') return `失败: ${(s as { status: 'Failed'; reason: string }).reason.slice(0, 40)}`
  return map[s.status] ?? s.status
}

export function serviceStatusDot(s: ServiceStatus): string {
  if (s.status === 'Running') return 'bg-green-500'
  if (s.status === 'Stopped' || s.status === 'Stopping') return 'bg-muted-foreground/40'
  if (s.status === 'Failed') return 'bg-red-500'
  return 'bg-yellow-400' // Starting
}

export function formatReason(reason: string): string {
  const map: Record<string, string> = {
    client_not_initialized: '客户端未初始化',
    python_subprocess_not_alive: 'Python 进程未存活',
    health_check_returned_false: '健康检查失败',
    pglite_lock_timeout: 'PGLite 锁超时',
    mcp_connect_timeout: 'MCP 连接超时',
    process_killed: '进程被系统终止',
    page_not_found: '页面不存在',
    pglite_not_ready: 'PGLite 未就绪',
    permission_denied: '权限不足',
    path_mismatch: '路径不匹配',
    launcher_missing_or_unusable: '启动器缺失或不可用',
    not_registered: '未注册',
    disconnected: '已断开',
    connecting: '连接中',
    connected: '已连接',
    error: '错误',
    unknown: '未知错误',
  }
  return map[reason] ?? reason
}
