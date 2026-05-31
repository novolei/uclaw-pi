// Owns the standalone ImChannelForm: field state, buildInput, and save
// (create/update) via settingsBridge (no direct Tauri IPC here). Extracted out
// of the 315-line ImChannelForm during the split. This is the flat add/edit
// form (distinct from the accordion row); its buildInput sets credentials
// unconditionally and supports the wechat_ilink app_id/api_key shape — preserved
// verbatim, including the email port validation.
import { useState } from 'react'
import type { ImChannelInput, ImChannelRow } from '@/atoms/im-channel-atoms'
import { settingsBridge } from '../../../lib/bridge/settings'

export function useImChannelForm(
  spaces: { id: string; name: string }[],
  editing: ImChannelRow | undefined,
  onDone: () => void,
) {
  const [channelType, setChannelType] = useState(editing?.channelType ?? 'webhook')
  const [name, setName] = useState(editing?.name ?? '')
  const [spaceId, setSpaceId] = useState(editing?.spaceId ?? spaces[0]?.id ?? '')
  const [enabled, setEnabled] = useState(editing?.enabled ?? true)
  const [streaming, setStreaming] = useState(editing?.streaming ?? false)
  const [permissionEnabled, setPermissionEnabled] = useState(editing?.permissionEnabled ?? false)
  const [owners, setOwners] = useState(editing?.owners.join(', ') ?? '')
  const [mcpEnabled, setMcpEnabled] = useState(editing?.guestPolicy.mcp_enabled ?? false)
  // Channel-specific fields
  const [webhookUrl, setWebhookUrl] = useState((editing?.config.url as string) ?? '')
  const [smtpHost, setSmtpHost] = useState((editing?.config.smtp_host as string) ?? '')
  const [smtpPort, setSmtpPort] = useState(String(editing?.config.smtp_port ?? '587'))
  const [smtpUser, setSmtpUser] = useState((editing?.config.username as string) ?? '')
  const [smtpPass, setSmtpPass] = useState('')
  const [toAddresses, setToAddresses] = useState((editing?.config.to_addresses as string[])?.join(', ') ?? '')
  const [corpId, setCorpId] = useState((editing?.config.corp_id as string) ?? '')
  const [agentId, setAgentId] = useState((editing?.config.agent_id as string) ?? '')
  const [corpSecret, setCorpSecret] = useState('')
  const [wecomWsUrl, setWecomWsUrl] = useState((editing?.config.ws_url as string) ?? '')
  const [appId, setAppId] = useState((editing?.config.app_id as string) ?? '')
  const [apiKey, setApiKey] = useState('')
  const [signingSecret, setSigningSecret] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  function buildInput(): ImChannelInput {
    let config: Record<string, unknown> = {}
    let credentials: Record<string, unknown> = {}

    switch (channelType) {
      case 'webhook':
        config = { url: webhookUrl }
        break
      case 'email':
        config = {
          smtp_host: smtpHost,
          smtp_port: Number(smtpPort),
          username: smtpUser,
          to_addresses: toAddresses.split(',').map(s => s.trim()).filter(Boolean),
        }
        credentials = { password: smtpPass }
        break
      case 'dingtalk':
      case 'feishu':
        config = { webhook_url: webhookUrl }
        credentials = { signing_secret: signingSecret }
        break
      case 'wecom_bot':
        config = { corp_id: corpId, agent_id: agentId, ...(wecomWsUrl ? { ws_url: wecomWsUrl } : {}) }
        credentials = { corp_secret: corpSecret }
        break
      case 'wechat_ilink':
        config = { app_id: appId }
        credentials = { api_key: apiKey }
        break
    }

    return {
      spaceId,
      channelType,
      name,
      config,
      credentials,
      enabled,
      streaming,
      replyScope: 'all',
      permissionEnabled,
      owners: owners.split(',').map(s => s.trim()).filter(Boolean),
      guestPolicy: { tool_allowlist: [], mcp_enabled: mcpEnabled },
    }
  }

  async function handleSave() {
    setSaving(true)
    setError(null)
    try {
      if (channelType === 'email') {
        const port = Number(smtpPort)
        if (!Number.isInteger(port) || port < 1 || port > 65535) {
          setError('端口号必须是 1–65535 之间的整数')
          setSaving(false)
          return
        }
      }
      const input = buildInput()
      if (editing) {
        await settingsBridge.updateImChannel(editing.id, input)
      } else {
        await settingsBridge.createImChannel(input)
      }
      onDone()
    } catch (e) {
      setError(String(e))
    } finally {
      setSaving(false)
    }
  }

  return {
    fields: {
      channelType, setChannelType,
      name, setName,
      spaceId, setSpaceId,
      enabled, setEnabled,
      streaming, setStreaming,
      permissionEnabled, setPermissionEnabled,
      owners, setOwners,
      mcpEnabled, setMcpEnabled,
      webhookUrl, setWebhookUrl,
      smtpHost, setSmtpHost,
      smtpPort, setSmtpPort,
      smtpUser, setSmtpUser,
      smtpPass, setSmtpPass,
      toAddresses, setToAddresses,
      corpId, setCorpId,
      agentId, setAgentId,
      corpSecret, setCorpSecret,
      wecomWsUrl, setWecomWsUrl,
      appId, setAppId,
      apiKey, setApiKey,
      signingSecret, setSigningSecret,
    },
    saving,
    error,
    handleSave,
  }
}

export type ImChannelFormFields = ReturnType<typeof useImChannelForm>['fields']
