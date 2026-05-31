import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { toast } from 'sonner'
import type { ImChannelRow, ImChannelInput, ImChannelStatus } from '@/atoms/im-channel-atoms'
// WechatIlinkBindingPanel migrated into the settings feature; this old-location
// row consumes it from the barrel until the row itself migrates (same batch).
import { WechatIlinkBindingPanel } from '@/features/settings'

// ──────────────── helpers ────────────────

function formatDuration(fromMs: number): string {
  const secs = Math.floor((Date.now() - fromMs) / 1000)
  if (secs < 60) return `${secs}s`
  const mins = Math.floor(secs / 60) % 60
  const hours = Math.floor(secs / 3600)
  if (hours > 0) return `${hours}h ${mins}m`
  return `${mins}m`
}

function getMetaLine(channel: ImChannelRow, status?: ImChannelStatus): string {
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

// ──────────────── props ────────────────

interface Props {
  channel?: ImChannelRow       // undefined = new-instance mode
  newChannelType?: string      // required when channel is undefined
  status?: ImChannelStatus
  spaces: { id: string; name: string }[]
  open: boolean
  onToggleOpen: () => void
  onToggleEnabled: (enabled: boolean) => void
  onSaved: () => void
  onDeleted: () => void
}

// ──────────────── component ────────────────

export function ImChannelAccordionRow({
  channel, newChannelType, status, spaces, open,
  onToggleOpen, onToggleEnabled, onSaved, onDeleted,
}: Props) {
  const isNew = channel === undefined
  const channelType = channel?.channelType ?? newChannelType ?? 'webhook'

  // ── field state (initialized from channel or empty) ──
  const [name, setName] = useState(channel?.name ?? '')
  const [spaceId, setSpaceId] = useState(channel?.spaceId ?? spaces[0]?.id ?? '')
  const [streaming, setStreaming] = useState(channel?.streaming ?? false)
  const [permissionEnabled, setPermissionEnabled] = useState(channel?.permissionEnabled ?? false)
  const [owners, setOwners] = useState(channel?.owners.join(', ') ?? '')
  const [mcpEnabled, setMcpEnabled] = useState(channel?.guestPolicy.mcp_enabled ?? false)

  // channel-type-specific
  const [corpId, setCorpId] = useState((channel?.config.corp_id as string | undefined) ?? '')
  const [agentId, setAgentId] = useState((channel?.config.agent_id as string | undefined) ?? '')
  const [corpSecret, setCorpSecret] = useState('')
  const [wecomWsUrl, setWecomWsUrl] = useState((channel?.config.ws_url as string | undefined) ?? '')
  const [webhookUrl, setWebhookUrl] = useState(
    (channel?.config.url as string | undefined) ??
    (channel?.config.webhook_url as string | undefined) ?? ''
  )
  const [signingSecret, setSigningSecret] = useState('')
  const [smtpHost, setSmtpHost] = useState((channel?.config.smtp_host as string | undefined) ?? '')
  const [smtpPort, setSmtpPort] = useState(String(channel?.config.smtp_port ?? '587'))
  const [smtpUser, setSmtpUser] = useState((channel?.config.username as string | undefined) ?? '')
  const [smtpPass, setSmtpPass] = useState('')
  const [toAddresses, setToAddresses] = useState(
    (channel?.config.to_addresses as string[] | undefined)?.join(', ') ?? ''
  )

  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Keep spaceId in sync if spaces loads after mount
  useEffect(() => {
    if (!channel && spaces.length > 0 && !spaceId) setSpaceId(spaces[0].id)
  }, [spaces, channel, spaceId])

  // Sync non-credential form fields when channel prop changes after save
  useEffect(() => {
    if (!channel) return
    setName(channel.name)
    setSpaceId(channel.spaceId)
    setStreaming(channel.streaming)
    setPermissionEnabled(channel.permissionEnabled)
    setOwners(channel.owners.join(', '))
    setMcpEnabled(channel.guestPolicy.mcp_enabled)
    setCorpId((channel.config.corp_id as string | undefined) ?? '')
    setAgentId((channel.config.agent_id as string | undefined) ?? '')
    setWecomWsUrl((channel.config.ws_url as string | undefined) ?? '')
    setWebhookUrl(
      (channel.config.url as string | undefined) ??
      (channel.config.webhook_url as string | undefined) ?? ''
    )
    setSmtpHost((channel.config.smtp_host as string | undefined) ?? '')
    setSmtpPort(String(channel.config.smtp_port ?? '587'))
    setSmtpUser((channel.config.username as string | undefined) ?? '')
    setToAddresses((channel.config.to_addresses as string[] | undefined)?.join(', ') ?? '')
    setDirty(false)
    setError(null)
  }, [channel])

  function markDirty() { setDirty(true) }

  function handleCancel() {
    setName(channel?.name ?? '')
    setSpaceId(channel?.spaceId ?? spaces[0]?.id ?? '')
    setStreaming(channel?.streaming ?? false)
    setPermissionEnabled(channel?.permissionEnabled ?? false)
    setOwners(channel?.owners.join(', ') ?? '')
    setMcpEnabled(channel?.guestPolicy.mcp_enabled ?? false)
    setCorpId((channel?.config.corp_id as string | undefined) ?? '')
    setAgentId((channel?.config.agent_id as string | undefined) ?? '')
    setCorpSecret('')
    setWecomWsUrl((channel?.config.ws_url as string | undefined) ?? '')
    setWebhookUrl(
      (channel?.config.url as string | undefined) ??
      (channel?.config.webhook_url as string | undefined) ?? ''
    )
    setSigningSecret('')
    setSmtpHost((channel?.config.smtp_host as string | undefined) ?? '')
    setSmtpPort(String(channel?.config.smtp_port ?? '587'))
    setSmtpUser((channel?.config.username as string | undefined) ?? '')
    setSmtpPass('')
    setToAddresses((channel?.config.to_addresses as string[] | undefined)?.join(', ') ?? '')
    setDirty(false)
    setError(null)
    if (isNew) onDeleted()
    else onToggleOpen()
  }

  function buildInput(): ImChannelInput {
    let config: Record<string, unknown> = {}
    let credentials: Record<string, unknown> = {}
    switch (channelType) {
      case 'wecom_bot':
        config = { corp_id: corpId, agent_id: agentId, ...(wecomWsUrl ? { ws_url: wecomWsUrl } : {}) }
        credentials = corpSecret ? { corp_secret: corpSecret } : {}
        break
      case 'wechat_ilink':
        config = {}
        credentials = {}
        break
      case 'dingtalk':
      case 'feishu':
        config = { webhook_url: webhookUrl }
        credentials = signingSecret ? { signing_secret: signingSecret } : {}
        break
      case 'email':
        config = {
          smtp_host: smtpHost,
          smtp_port: Number(smtpPort),
          username: smtpUser,
          to_addresses: toAddresses.split(',').map(s => s.trim()).filter(Boolean),
        }
        credentials = smtpPass ? { password: smtpPass } : {}
        break
      default: // webhook
        config = { url: webhookUrl }
        credentials = {}
    }
    return {
      spaceId,
      channelType,
      name,
      config,
      credentials,
      enabled: channel?.enabled ?? true,
      streaming,
      replyScope: 'all',
      permissionEnabled,
      owners: owners.split(',').map(s => s.trim()).filter(Boolean),
      guestPolicy: { tool_allowlist: [], mcp_enabled: mcpEnabled },
    }
  }

  async function handleSave() {
    if (channelType === 'email') {
      const port = Number(smtpPort)
      if (!Number.isInteger(port) || port < 1 || port > 65535) {
        setError('端口号必须是 1–65535 之间的整数')
        return
      }
    }
    setSaving(true)
    setError(null)
    try {
      const input = buildInput()
      if (isNew) {
        await invoke('create_im_channel', { input })
      } else {
        await invoke('update_im_channel', { id: channel!.id, input })
      }
      setDirty(false)
      onSaved()
    } catch (e) {
      setError(String(e))
    } finally {
      setSaving(false)
    }
  }

  async function handleStatusAction() {
    if (!channel) return
    const state = status?.state
    try {
      if (state === 'online') {
        await invoke('toggle_im_channel', { id: channel.id, enabled: false })
        onSaved()
      } else if (state === 'error') {
        await invoke('update_im_channel', { id: channel.id, input: buildInput() })
        onSaved()
      } else {
        await invoke('toggle_im_channel', { id: channel.id, enabled: true })
        onSaved()
      }
    } catch (e) {
      toast.error(String(e))
    }
  }

  // ── save button label ──
  const saveLabel = dirty && status?.state === 'online' ? '保存并重连' : '保存'

  // ── status block ──
  const stateColor = {
    online:       'bg-success/10 border-success/30',
    error:        'bg-destructive/10 border-destructive/30',
    offline:      'bg-muted border-border',
    needs_rebind: 'bg-amber-500/10 border-amber-500/30',
  }[status?.state ?? 'offline']

  const stateDotCls = {
    online:       'bg-success',
    error:        'bg-destructive',
    offline:      'bg-muted-foreground',
    needs_rebind: 'bg-amber-500',
  }[status?.state ?? 'offline']

  const stateTitle = status?.state === 'online'
    ? `WebSocket 已连接${status.connectedSinceMs ? ` · 在线 ${formatDuration(status.connectedSinceMs)}` : ''}`
    : status?.state === 'error'
    ? `连接错误`
    : '未连接'

  const stateDetail = status?.state === 'online'
    ? status.messageCountToday ? `今日 ${status.messageCountToday} 条消息` : ''
    : status?.state === 'error'
    ? (status.lastError ?? '')
    : ''

  const stateActionLabel = status?.state === 'online' ? '停用' : status?.state === 'error' ? '重连' : '启用'
  const stateActionCls = status?.state === 'error'
    ? 'border-destructive/50 text-destructive'
    : 'border-border text-muted-foreground'

  const credHighlight = status?.state === 'error'

  const inputCls = (highlight = false) =>
    `w-full rounded border bg-background px-2 py-1.5 text-sm ${highlight ? 'border-destructive' : 'border-border'}`

  // ──────────────── closed row ────────────────
  const closedRow = (
    <div
      className="flex items-center justify-between px-3 py-2 cursor-pointer select-none"
      onClick={onToggleOpen}
    >
      <div className="flex items-center gap-2 min-w-0">
        {!isNew && (
          <span
            className={`w-2 h-2 rounded-full flex-shrink-0 ${
              status?.state === 'online'
                ? 'bg-success animate-pulse'
                : status?.state === 'error'
                ? 'bg-destructive'
                : 'bg-muted-foreground'
            }`}
          />
        )}
        <span className="text-sm font-medium truncate">
          {isNew ? `新${channelType === 'wecom_bot' ? '企业微信' : ''}实例` : channel!.name}
        </span>
        {!isNew && status?.state === 'error' && (
          <span className="rounded px-1.5 py-0.5 text-xs bg-destructive/10 border border-destructive/30 text-destructive whitespace-nowrap">
            {status.lastError?.slice(0, 10) ?? '连接错误'}
          </span>
        )}
        {!isNew && channel!.spaceId && (
          <span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground whitespace-nowrap">
            {spaces.find(s => s.id === channel!.spaceId)?.name ?? channel!.spaceId}
          </span>
        )}
      </div>
      <div className="flex items-center gap-2 flex-shrink-0" onClick={e => e.stopPropagation()}>
        {!isNew && (
          <button
            type="button"
            aria-label={channel!.enabled ? '停用' : '启用'}
            onClick={() => onToggleEnabled(!channel!.enabled)}
            className={[
              'relative inline-flex h-4 w-8 cursor-pointer rounded-full border-2 border-transparent transition-colors',
              channel!.enabled ? 'bg-success' : 'bg-muted',
            ].join(' ')}
          >
            <span
              className={[
                'pointer-events-none inline-block h-3 w-3 rounded-full bg-white shadow transform transition-transform',
                channel!.enabled ? 'translate-x-4' : 'translate-x-0',
              ].join(' ')}
            />
          </button>
        )}
        <span
          className={`text-muted-foreground text-sm transition-transform ${open ? 'rotate-90' : ''}`}
        >
          ›
        </span>
      </div>
    </div>
  )

  const metaLine = !isNew && !open && (
    <div className="px-3 pb-2 text-xs text-muted-foreground" onClick={onToggleOpen} style={{cursor:'pointer'}}>
      {getMetaLine(channel!, status)}
    </div>
  )

  // ──────────────── expanded content ────────────────
  const expandedContent = open && (
    <div className="border-t border-border px-3 py-3 space-y-3">

      {!isNew && (
        <div className={`flex items-start justify-between gap-3 rounded border p-2.5 ${stateColor}`}>
          <div className="flex items-start gap-2">
            <span className={`mt-0.5 w-2 h-2 rounded-full flex-shrink-0 ${stateDotCls}`} />
            <div>
              <div className={`text-xs font-medium ${status?.state === 'error' ? 'text-destructive' : status?.state === 'online' ? 'text-success' : 'text-muted-foreground'}`}>
                {stateTitle}
              </div>
              {stateDetail && (
                <div className="text-xs text-muted-foreground mt-0.5">{stateDetail}</div>
              )}
            </div>
          </div>
          <button
            type="button"
            onClick={handleStatusAction}
            className={`flex-shrink-0 rounded border px-2 py-1 text-xs whitespace-nowrap ${stateActionCls}`}
          >
            {stateActionLabel}
          </button>
        </div>
      )}

      <div>
        <label className="block text-xs text-muted-foreground mb-1">名称</label>
        <input
          value={name}
          onChange={e => { setName(e.target.value); markDirty() }}
          className={inputCls()}
          placeholder="我的企微机器人"
        />
      </div>

      <div className="grid grid-cols-2 gap-x-3 gap-y-2">

        {channelType === 'wecom_bot' && <>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">Corp ID</label>
            <input value={corpId} readOnly={!isNew} onChange={isNew ? e => { setCorpId(e.target.value); markDirty() } : undefined} className={`${inputCls()} font-mono ${!isNew ? 'opacity-70' : ''}`} />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">Agent ID</label>
            <input value={agentId} readOnly={!isNew} onChange={isNew ? e => { setAgentId(e.target.value); markDirty() } : undefined} className={`${inputCls()} font-mono ${!isNew ? 'opacity-70' : ''}`} />
          </div>
          <div className="col-span-2">
            <label className={`block text-xs mb-1 ${credHighlight ? 'text-destructive font-medium' : 'text-muted-foreground'}`}>
              Corp Secret{credHighlight && <span className="ml-0.5 text-destructive">*</span>}
            </label>
            <input
              type="password"
              value={corpSecret}
              onChange={e => { setCorpSecret(e.target.value); markDirty() }}
              className={inputCls(credHighlight)}
              placeholder="留空则不修改"
            />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">绑定 Space</label>
            <select
              value={spaceId}
              onChange={e => { setSpaceId(e.target.value); markDirty() }}
              className={inputCls()}
            >
              {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
            </select>
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">WebSocket URL（可选）</label>
            <input
              value={wecomWsUrl}
              onChange={e => { setWecomWsUrl(e.target.value); markDirty() }}
              className={`${inputCls()} font-mono`}
              placeholder="wss://openws.work.weixin.qq.com"
            />
          </div>
        </>}

        {channelType === 'wechat_ilink' && (
          <>
            <div className="col-span-2">
              <label className="block text-xs text-muted-foreground mb-1">绑定 Space</label>
              <select
                value={spaceId}
                onChange={e => { setSpaceId(e.target.value); markDirty() }}
                className={inputCls()}
              >
                {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
              </select>
            </div>
            {!isNew && (
              <div className="col-span-2">
                <WechatIlinkBindingPanel
                  instanceId={channel!.id}
                  accountId={channel!.config.account_id as string | undefined}
                  status={status}
                  onSaved={onSaved}
                  onDisconnect={onSaved}
                />
              </div>
            )}
          </>
        )}

        {(channelType === 'dingtalk' || channelType === 'feishu') && <>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">Webhook URL</label>
            <input
              value={webhookUrl}
              onChange={e => { setWebhookUrl(e.target.value); markDirty() }}
              className={inputCls()}
            />
          </div>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">签名密钥（可选）</label>
            <input
              type="password"
              value={signingSecret}
              onChange={e => { setSigningSecret(e.target.value); markDirty() }}
              className={inputCls(credHighlight)}
            />
          </div>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">绑定 Space</label>
            <select value={spaceId} onChange={e => { setSpaceId(e.target.value); markDirty() }} className={inputCls()}>
              {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
            </select>
          </div>
        </>}

        {channelType === 'email' && <>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">SMTP Host</label>
            <input value={smtpHost} onChange={e => { setSmtpHost(e.target.value); markDirty() }} className={inputCls()} placeholder="smtp.gmail.com" />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">端口</label>
            <input value={smtpPort} onChange={e => { setSmtpPort(e.target.value); markDirty() }} className={inputCls()} placeholder="587" />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">用户名</label>
            <input value={smtpUser} onChange={e => { setSmtpUser(e.target.value); markDirty() }} className={inputCls()} />
          </div>
          <div>
            <label className={`block text-xs mb-1 ${credHighlight ? 'text-destructive font-medium' : 'text-muted-foreground'}`}>
              密码{credHighlight && <span className="ml-0.5 text-destructive">*</span>}
            </label>
            <input type="password" value={smtpPass} onChange={e => { setSmtpPass(e.target.value); markDirty() }} className={inputCls(credHighlight)} placeholder="留空则不修改" />
          </div>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">收件人（逗号分隔）</label>
            <input value={toAddresses} onChange={e => { setToAddresses(e.target.value); markDirty() }} className={inputCls()} placeholder="a@example.com, b@example.com" />
          </div>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">绑定 Space</label>
            <select value={spaceId} onChange={e => { setSpaceId(e.target.value); markDirty() }} className={inputCls()}>
              {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
            </select>
          </div>
        </>}

        {channelType === 'webhook' && <>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">Webhook URL</label>
            <input value={webhookUrl} onChange={e => { setWebhookUrl(e.target.value); markDirty() }} className={inputCls()} placeholder="https://example.com/hook" />
          </div>
          <div className="col-span-2">
            <label className="block text-xs text-muted-foreground mb-1">绑定 Space</label>
            <select value={spaceId} onChange={e => { setSpaceId(e.target.value); markDirty() }} className={inputCls()}>
              {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
            </select>
          </div>
        </>}
      </div>

      <div className="flex gap-4 text-sm">
        <label className="flex items-center gap-1.5">
          <input type="checkbox" checked={streaming} onChange={e => { setStreaming(e.target.checked); markDirty() }} />
          流式回复
        </label>
        <label className="flex items-center gap-1.5">
          <input type="checkbox" checked={permissionEnabled} onChange={e => { setPermissionEnabled(e.target.checked); markDirty() }} />
          开启权限控制
        </label>
      </div>

      {permissionEnabled && (
        <div className="rounded border border-border p-2.5 space-y-2">
          <div>
            <label className="block text-xs text-muted-foreground mb-1">Owners（chat_id，逗号分隔）</label>
            <input value={owners} onChange={e => { setOwners(e.target.value); markDirty() }} className={inputCls()} placeholder="openid_1, openid_2" />
          </div>
          <label className="flex items-center gap-1.5 text-sm">
            <input type="checkbox" checked={mcpEnabled} onChange={e => { setMcpEnabled(e.target.checked); markDirty() }} />
            Guest 允许 MCP 工具
          </label>
        </div>
      )}

      {error && <p className="text-sm text-destructive">{error}</p>}

      <div className="flex items-center justify-between pt-2 border-t border-border">
        {!isNew ? (
          <button
            type="button"
            onClick={onDeleted}
            className="text-xs text-destructive hover:underline"
          >
            删除实例
          </button>
        ) : <span />}
        <div className="flex gap-2">
          <button
            type="button"
            onClick={handleCancel}
            className="rounded border border-border bg-background px-3 py-1.5 text-sm hover:bg-muted"
          >
            取消
          </button>
          <button
            type="button"
            onClick={handleSave}
            disabled={saving || !dirty || !name || !spaceId}
            className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
          >
            {saving ? '保存中…' : saveLabel}
          </button>
        </div>
      </div>
    </div>
  )

  return (
    <div className={`rounded border transition-colors ${open ? 'border-primary' : 'border-border'}`}>
      {closedRow}
      {metaLine}
      <div
        className="overflow-hidden transition-[max-height] duration-200 ease-out"
        style={{ maxHeight: open ? '1000px' : '0px' }}
      >
        {expandedContent}
      </div>
    </div>
  )
}
