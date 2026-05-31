// Per-channel-type field blocks for the standalone ImChannelForm — the
// `channelType ===` branches (webhook / dingtalk+feishu / email / wecom_bot /
// wechat_ilink). Split out of the 315-line ImChannelForm during the migration.
// Pure presentation: every field reads/writes the `fields` bag from
// useImChannelForm. No IPC.
import type { ImChannelFormFields as Fields } from '../../../hooks/useImChannelForm'

const INPUT_CLS = 'w-full rounded border border-border bg-background px-2 py-1.5 text-sm'

export function ImChannelFormFields({ fields }: { fields: Fields }) {
  const { channelType } = fields
  return (
    <>
      {channelType === 'webhook' && (
        <div className="space-y-1">
          <label className="text-xs text-muted-foreground">Webhook URL</label>
          <input value={fields.webhookUrl} onChange={e => fields.setWebhookUrl(e.target.value)}
            className={INPUT_CLS}
            placeholder="https://example.com/hook" />
        </div>
      )}

      {(channelType === 'dingtalk' || channelType === 'feishu') && (
        <>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">Webhook URL</label>
            <input value={fields.webhookUrl} onChange={e => fields.setWebhookUrl(e.target.value)}
              className={INPUT_CLS} />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">签名密钥（可选）</label>
            <input value={fields.signingSecret} onChange={e => fields.setSigningSecret(e.target.value)}
              type="password"
              className={INPUT_CLS} />
          </div>
        </>
      )}

      {channelType === 'email' && (
        <>
          <div className="grid grid-cols-2 gap-2">
            <div className="space-y-1">
              <label className="text-xs text-muted-foreground">SMTP Host</label>
              <input value={fields.smtpHost} onChange={e => fields.setSmtpHost(e.target.value)}
                className={INPUT_CLS}
                placeholder="smtp.gmail.com" />
            </div>
            <div className="space-y-1">
              <label className="text-xs text-muted-foreground">端口</label>
              <input value={fields.smtpPort} onChange={e => fields.setSmtpPort(e.target.value)}
                className={INPUT_CLS}
                placeholder="587" />
            </div>
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">用户名</label>
            <input value={fields.smtpUser} onChange={e => fields.setSmtpUser(e.target.value)}
              className={INPUT_CLS} />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">密码</label>
            <input value={fields.smtpPass} onChange={e => fields.setSmtpPass(e.target.value)}
              type="password"
              className={INPUT_CLS}
              placeholder="留空则不修改" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">收件人（逗号分隔）</label>
            <input value={fields.toAddresses} onChange={e => fields.setToAddresses(e.target.value)}
              className={INPUT_CLS}
              placeholder="a@example.com, b@example.com" />
          </div>
        </>
      )}

      {channelType === 'wecom_bot' && (
        <>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">Corp ID</label>
            <input value={fields.corpId} onChange={e => fields.setCorpId(e.target.value)}
              className={INPUT_CLS} />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">Agent ID</label>
            <input value={fields.agentId} onChange={e => fields.setAgentId(e.target.value)}
              className={INPUT_CLS} />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">Corp Secret</label>
            <input value={fields.corpSecret} onChange={e => fields.setCorpSecret(e.target.value)}
              type="password"
              className={INPUT_CLS}
              placeholder="留空则不修改" />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">
              WebSocket 服务器（可选，私有化部署时填写）
            </label>
            <input value={fields.wecomWsUrl} onChange={e => fields.setWecomWsUrl(e.target.value)}
              className={`${INPUT_CLS} font-mono`}
              placeholder="wss://openws.work.weixin.qq.com" />
          </div>
        </>
      )}

      {channelType === 'wechat_ilink' && (
        <>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">App ID</label>
            <input value={fields.appId} onChange={e => fields.setAppId(e.target.value)}
              className={INPUT_CLS} />
          </div>
          <div className="space-y-1">
            <label className="text-xs text-muted-foreground">API Key</label>
            <input value={fields.apiKey} onChange={e => fields.setApiKey(e.target.value)}
              type="password"
              className={INPUT_CLS}
              placeholder="留空则不修改" />
          </div>
        </>
      )}
    </>
  )
}
