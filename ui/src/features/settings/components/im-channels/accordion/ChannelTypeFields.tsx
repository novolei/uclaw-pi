// The per-channel-type credential/config field grid for the accordion row — the
// `channelType ===` branches (wecom_bot / wechat_ilink / dingtalk+feishu / email
// / webhook). Split out of the 602-line ImChannelAccordionRow during the
// IM-channel migration. Pure presentation: every field reads/writes the `fields`
// bag from useImChannelAccordionForm and calls markDirty on change. The
// wechat_ilink branch renders the (feature-local) WechatIlinkBindingPanel.
import type { ImChannelRow, ImChannelStatus } from '@/atoms/im-channel-atoms'
import type { ImChannelAccordionFields } from '../../../hooks/useImChannelAccordionForm'
import { WechatIlinkBindingPanel } from '../WechatIlinkBindingPanel'

interface Props {
  channel?: ImChannelRow
  channelType: string
  status?: ImChannelStatus
  spaces: { id: string; name: string }[]
  isNew: boolean
  credHighlight: boolean
  fields: ImChannelAccordionFields
  markDirty: () => void
  inputCls: (highlight?: boolean) => string
  onSaved: () => void
}

export function ChannelTypeFields({
  channel, channelType, status, spaces, isNew, credHighlight,
  fields, markDirty, inputCls, onSaved,
}: Props) {
  const spaceSelect = (
    <div className={channelType === 'wecom_bot' ? '' : 'col-span-2'}>
      <label className="block text-xs text-muted-foreground mb-1">绑定 Space</label>
      <select
        value={fields.spaceId}
        onChange={e => { fields.setSpaceId(e.target.value); markDirty() }}
        className={inputCls()}
      >
        {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
      </select>
    </div>
  )

  return (
    <div className="grid grid-cols-2 gap-x-3 gap-y-2">
      {channelType === 'wecom_bot' && <>
        <div>
          <label className="block text-xs text-muted-foreground mb-1">Corp ID</label>
          <input value={fields.corpId} readOnly={!isNew} onChange={isNew ? e => { fields.setCorpId(e.target.value); markDirty() } : undefined} className={`${inputCls()} font-mono ${!isNew ? 'opacity-70' : ''}`} />
        </div>
        <div>
          <label className="block text-xs text-muted-foreground mb-1">Agent ID</label>
          <input value={fields.agentId} readOnly={!isNew} onChange={isNew ? e => { fields.setAgentId(e.target.value); markDirty() } : undefined} className={`${inputCls()} font-mono ${!isNew ? 'opacity-70' : ''}`} />
        </div>
        <div className="col-span-2">
          <label className={`block text-xs mb-1 ${credHighlight ? 'text-destructive font-medium' : 'text-muted-foreground'}`}>
            Corp Secret{credHighlight && <span className="ml-0.5 text-destructive">*</span>}
          </label>
          <input
            type="password"
            value={fields.corpSecret}
            onChange={e => { fields.setCorpSecret(e.target.value); markDirty() }}
            className={inputCls(credHighlight)}
            placeholder="留空则不修改"
          />
        </div>
        {spaceSelect}
        <div>
          <label className="block text-xs text-muted-foreground mb-1">WebSocket URL（可选）</label>
          <input
            value={fields.wecomWsUrl}
            onChange={e => { fields.setWecomWsUrl(e.target.value); markDirty() }}
            className={`${inputCls()} font-mono`}
            placeholder="wss://openws.work.weixin.qq.com"
          />
        </div>
      </>}

      {channelType === 'wechat_ilink' && (
        <>
          {spaceSelect}
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
            value={fields.webhookUrl}
            onChange={e => { fields.setWebhookUrl(e.target.value); markDirty() }}
            className={inputCls()}
          />
        </div>
        <div className="col-span-2">
          <label className="block text-xs text-muted-foreground mb-1">签名密钥（可选）</label>
          <input
            type="password"
            value={fields.signingSecret}
            onChange={e => { fields.setSigningSecret(e.target.value); markDirty() }}
            className={inputCls(credHighlight)}
          />
        </div>
        {spaceSelect}
      </>}

      {channelType === 'email' && <>
        <div>
          <label className="block text-xs text-muted-foreground mb-1">SMTP Host</label>
          <input value={fields.smtpHost} onChange={e => { fields.setSmtpHost(e.target.value); markDirty() }} className={inputCls()} placeholder="smtp.gmail.com" />
        </div>
        <div>
          <label className="block text-xs text-muted-foreground mb-1">端口</label>
          <input value={fields.smtpPort} onChange={e => { fields.setSmtpPort(e.target.value); markDirty() }} className={inputCls()} placeholder="587" />
        </div>
        <div>
          <label className="block text-xs text-muted-foreground mb-1">用户名</label>
          <input value={fields.smtpUser} onChange={e => { fields.setSmtpUser(e.target.value); markDirty() }} className={inputCls()} />
        </div>
        <div>
          <label className={`block text-xs mb-1 ${credHighlight ? 'text-destructive font-medium' : 'text-muted-foreground'}`}>
            密码{credHighlight && <span className="ml-0.5 text-destructive">*</span>}
          </label>
          <input type="password" value={fields.smtpPass} onChange={e => { fields.setSmtpPass(e.target.value); markDirty() }} className={inputCls(credHighlight)} placeholder="留空则不修改" />
        </div>
        <div className="col-span-2">
          <label className="block text-xs text-muted-foreground mb-1">收件人（逗号分隔）</label>
          <input value={fields.toAddresses} onChange={e => { fields.setToAddresses(e.target.value); markDirty() }} className={inputCls()} placeholder="a@example.com, b@example.com" />
        </div>
        {spaceSelect}
      </>}

      {channelType === 'webhook' && <>
        <div className="col-span-2">
          <label className="block text-xs text-muted-foreground mb-1">Webhook URL</label>
          <input value={fields.webhookUrl} onChange={e => { fields.setWebhookUrl(e.target.value); markDirty() }} className={inputCls()} placeholder="https://example.com/hook" />
        </div>
        {spaceSelect}
      </>}
    </div>
  )
}
