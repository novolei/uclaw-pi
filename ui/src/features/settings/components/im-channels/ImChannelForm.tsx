// Standalone IM-channel add/edit form — thin shell. Field state + buildInput +
// save (create/update) IPC live in useImChannelForm; the per-channel-type field
// blocks are ImChannelFormFields. Split out of the 315-line components/settings
// version during the migration. No direct Tauri IPC here; behavior preserved.
import type { ImChannelRow } from '@/atoms/im-channel-atoms'
import { useImChannelForm } from '../../hooks/useImChannelForm'
import { ImChannelFormFields } from './form/ImChannelFormFields'

const CHANNEL_TYPES = [
  { value: 'wecom_bot',    label: '企业微信 Bot (WebSocket)' },
  { value: 'wechat_ilink', label: '微信个人 (iLink)' },
  { value: 'email',        label: '电子邮件 (SMTP)' },
  { value: 'dingtalk',     label: '钉钉 Webhook' },
  { value: 'feishu',       label: '飞书 Webhook' },
  { value: 'webhook',      label: '通用 Webhook' },
]

interface Props {
  spaces: { id: string; name: string }[]
  editing?: ImChannelRow
  onDone: () => void
}

export function ImChannelForm({ spaces, editing, onDone }: Props) {
  const { fields, saving, error, handleSave } = useImChannelForm(spaces, editing, onDone)

  return (
    <div className="space-y-4 p-4">
      <div className="space-y-1">
        <label className="text-xs text-muted-foreground">渠道类型</label>
        <select
          value={fields.channelType}
          onChange={e => fields.setChannelType(e.target.value)}
          className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
          disabled={!!editing}
        >
          {CHANNEL_TYPES.map(t => (
            <option key={t.value} value={t.value}>{t.label}</option>
          ))}
        </select>
      </div>

      <div className="space-y-1">
        <label className="text-xs text-muted-foreground">名称</label>
        <input
          value={fields.name}
          onChange={e => fields.setName(e.target.value)}
          className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
          placeholder="我的企微机器人"
        />
      </div>

      <div className="space-y-1">
        <label className="text-xs text-muted-foreground">绑定 Space</label>
        <select
          value={fields.spaceId}
          onChange={e => fields.setSpaceId(e.target.value)}
          className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
        >
          {spaces.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
        </select>
      </div>

      <ImChannelFormFields fields={fields} />

      <div className="flex items-center gap-3">
        <label className="flex items-center gap-1.5 text-sm">
          <input type="checkbox" checked={fields.enabled} onChange={e => fields.setEnabled(e.target.checked)} />
          启用
        </label>
        <label className="flex items-center gap-1.5 text-sm">
          <input type="checkbox" checked={fields.streaming} onChange={e => fields.setStreaming(e.target.checked)} />
          流式回复
        </label>
      </div>

      <div className="space-y-2 rounded border border-border p-3">
        <label className="flex items-center gap-1.5 text-sm font-medium">
          <input type="checkbox" checked={fields.permissionEnabled}
            onChange={e => fields.setPermissionEnabled(e.target.checked)} />
          启用权限控制
        </label>
        {fields.permissionEnabled && (
          <>
            <div className="space-y-1">
              <label className="text-xs text-muted-foreground">
                Owners（chat_id 白名单，逗号分隔）
              </label>
              <input value={fields.owners} onChange={e => fields.setOwners(e.target.value)}
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
                placeholder="openid_1, openid_2" />
            </div>
            <label className="flex items-center gap-1.5 text-sm">
              <input type="checkbox" checked={fields.mcpEnabled}
                onChange={e => fields.setMcpEnabled(e.target.checked)} />
              Guest 允许 MCP 工具
            </label>
          </>
        )}
      </div>

      {error && <p className="text-sm text-destructive">{error}</p>}

      <div className="flex justify-end gap-2">
        <button onClick={onDone}
          className="rounded px-3 py-1.5 text-sm hover:bg-muted">
          取消
        </button>
        <button onClick={handleSave} disabled={saving || !fields.name || !fields.spaceId}
          className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50">
          {saving ? '保存中…' : '保存'}
        </button>
      </div>
    </div>
  )
}
