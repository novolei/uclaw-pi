// Owns the ImChannelAccordionRow form: per-channel-type field state, the two
// prop-sync effects, dirty tracking, buildInput, save (create/update), and the
// status action (enable/disable/reconnect). Extracted out of the 602-line row
// during the IM-channel split. All IPC goes through `settingsBridge` (no direct
// Tauri IPC here). Behavior — credential-blank-keeps-existing, the email port
// validation, the "保存并重连" relabel — is preserved verbatim.
import { useEffect, useState } from 'react'
import { toast } from 'sonner'
import type { ImChannelRow, ImChannelInput, ImChannelStatus } from '@/atoms/im-channel-atoms'
import { settingsBridge } from '../../../lib/bridge/settings'

interface Args {
  channel?: ImChannelRow
  newChannelType?: string
  status?: ImChannelStatus
  spaces: { id: string; name: string }[]
  isNew: boolean
  channelType: string
  onToggleOpen: () => void
  onSaved: () => void
  onDeleted: () => void
}

export function useImChannelAccordionForm({
  channel, status, spaces, isNew, channelType,
  onToggleOpen, onSaved, onDeleted,
}: Args) {
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
    (channel?.config.webhook_url as string | undefined) ?? '',
  )
  const [signingSecret, setSigningSecret] = useState('')
  const [smtpHost, setSmtpHost] = useState((channel?.config.smtp_host as string | undefined) ?? '')
  const [smtpPort, setSmtpPort] = useState(String(channel?.config.smtp_port ?? '587'))
  const [smtpUser, setSmtpUser] = useState((channel?.config.username as string | undefined) ?? '')
  const [smtpPass, setSmtpPass] = useState('')
  const [toAddresses, setToAddresses] = useState(
    (channel?.config.to_addresses as string[] | undefined)?.join(', ') ?? '',
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
      (channel.config.webhook_url as string | undefined) ?? '',
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
      (channel?.config.webhook_url as string | undefined) ?? '',
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
        await settingsBridge.createImChannel(input)
      } else {
        await settingsBridge.updateImChannel(channel!.id, input)
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
        await settingsBridge.toggleImChannel(channel.id, false)
        onSaved()
      } else if (state === 'error') {
        await settingsBridge.updateImChannel(channel.id, buildInput())
        onSaved()
      } else {
        await settingsBridge.toggleImChannel(channel.id, true)
        onSaved()
      }
    } catch (e) {
      toast.error(String(e))
    }
  }

  // ── save button label ──
  const saveLabel = dirty && status?.state === 'online' ? '保存并重连' : '保存'

  return {
    fields: {
      name, setName,
      spaceId, setSpaceId,
      streaming, setStreaming,
      permissionEnabled, setPermissionEnabled,
      owners, setOwners,
      mcpEnabled, setMcpEnabled,
      corpId, setCorpId,
      agentId, setAgentId,
      corpSecret, setCorpSecret,
      wecomWsUrl, setWecomWsUrl,
      webhookUrl, setWebhookUrl,
      signingSecret, setSigningSecret,
      smtpHost, setSmtpHost,
      smtpPort, setSmtpPort,
      smtpUser, setSmtpUser,
      smtpPass, setSmtpPass,
      toAddresses, setToAddresses,
    },
    dirty,
    saving,
    error,
    saveLabel,
    markDirty,
    handleCancel,
    handleSave,
    handleStatusAction,
  }
}

export type ImChannelAccordionFields = ReturnType<typeof useImChannelAccordionForm>['fields']
