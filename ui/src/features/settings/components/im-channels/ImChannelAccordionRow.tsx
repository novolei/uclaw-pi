// One IM-channel instance accordion row — thin shell. All field state + IPC
// (create/update/toggle) live in useImChannelAccordionForm; the per-channel-type
// field grid is ChannelTypeFields; formatDuration/getMetaLine are in
// lib/im-channel-format. Split out of the 602-line components/settings version
// during the IM-channel migration. No direct Tauri IPC here; behavior preserved.
import type { ImChannelRow, ImChannelStatus } from '@/atoms/im-channel-atoms'
import { useImChannelAccordionForm } from '../../hooks/useImChannelAccordionForm'
import { formatDuration, getMetaLine } from '../../lib/im-channel-format'
import { ChannelTypeFields } from './accordion/ChannelTypeFields'

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

export function ImChannelAccordionRow({
  channel, newChannelType, status, spaces, open,
  onToggleOpen, onToggleEnabled, onSaved, onDeleted,
}: Props) {
  const isNew = channel === undefined
  const channelType = channel?.channelType ?? newChannelType ?? 'webhook'

  const {
    fields, dirty, saving, error, saveLabel,
    markDirty, handleCancel, handleSave, handleStatusAction,
  } = useImChannelAccordionForm({
    channel, newChannelType, status, spaces, isNew, channelType,
    onToggleOpen, onSaved, onDeleted,
  })

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
          value={fields.name}
          onChange={e => { fields.setName(e.target.value); markDirty() }}
          className={inputCls()}
          placeholder="我的企微机器人"
        />
      </div>

      <ChannelTypeFields
        channel={channel}
        channelType={channelType}
        status={status}
        spaces={spaces}
        isNew={isNew}
        credHighlight={credHighlight}
        fields={fields}
        markDirty={markDirty}
        inputCls={inputCls}
        onSaved={onSaved}
      />

      <div className="flex gap-4 text-sm">
        <label className="flex items-center gap-1.5">
          <input type="checkbox" checked={fields.streaming} onChange={e => { fields.setStreaming(e.target.checked); markDirty() }} />
          流式回复
        </label>
        <label className="flex items-center gap-1.5">
          <input type="checkbox" checked={fields.permissionEnabled} onChange={e => { fields.setPermissionEnabled(e.target.checked); markDirty() }} />
          开启权限控制
        </label>
      </div>

      {fields.permissionEnabled && (
        <div className="rounded border border-border p-2.5 space-y-2">
          <div>
            <label className="block text-xs text-muted-foreground mb-1">Owners（chat_id，逗号分隔）</label>
            <input value={fields.owners} onChange={e => { fields.setOwners(e.target.value); markDirty() }} className={inputCls()} placeholder="openid_1, openid_2" />
          </div>
          <label className="flex items-center gap-1.5 text-sm">
            <input type="checkbox" checked={fields.mcpEnabled} onChange={e => { fields.setMcpEnabled(e.target.checked); markDirty() }} />
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
            disabled={saving || !dirty || !fields.name || !fields.spaceId}
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
