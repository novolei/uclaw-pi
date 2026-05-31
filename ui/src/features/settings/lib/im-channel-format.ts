// Pure presentation helpers for the IM-channel accordion row — duration
// formatting + the collapsed-row meta line. Extracted out of the 602-line
// ImChannelAccordionRow during the IM-channel split. No IPC, no React.
import type { ImChannelRow, ImChannelStatus } from '@/atoms/im-channel-atoms'

export function formatDuration(fromMs: number): string {
  const secs = Math.floor((Date.now() - fromMs) / 1000)
  if (secs < 60) return `${secs}s`
  const mins = Math.floor(secs / 60) % 60
  const hours = Math.floor(secs / 3600)
  if (hours > 0) return `${hours}h ${mins}m`
  return `${mins}m`
}

export function getMetaLine(channel: ImChannelRow, status?: ImChannelStatus): string {
  const ct = channel.channelType
  if (ct === 'wecom_bot') {
    const corpId = (channel.config.corp_id as string | undefined) ?? ''
    const prefix = corpId.length > 10 ? corpId.slice(0, 10) + '…' : corpId
    if (status?.state === 'online') {
      const since = status.connectedSinceMs
        ? `在线 ${formatDuration(status.connectedSinceMs)}`
        : '在线'
      const count = status.messageCountToday ? ` · 今日 ${status.messageCountToday} 条` : ''
      return `corp_id: ${prefix} · ${since}${count}`
    }
    if (status?.state === 'error') {
      const snippet = status.lastError?.slice(0, 50) ?? '连接错误'
      return `corp_id: ${prefix} · ${snippet}`
    }
    return `corp_id: ${prefix} · 已停用`
  }
  if (ct === 'wechat_ilink') {
    const accountId = (channel.config.account_id as string | undefined) ?? ''
    if (status?.state === 'needs_rebind') return `账号: ${accountId.slice(0, 16) || '未知'} · 需要重新绑定`
    if (accountId) return `账号: ${accountId.slice(0, 16)}`
    return '未绑定'
  }
  const url =
    (channel.config.url as string | undefined) ??
    (channel.config.webhook_url as string | undefined) ?? ''
  return url ? `url: ${url.slice(0, 50)}${url.length > 50 ? '…' : ''}` : ''
}
