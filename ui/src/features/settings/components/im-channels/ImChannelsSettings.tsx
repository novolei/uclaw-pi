// Settings → 机器人 — the IM-channel tabbed list. Thin shell: data + IPC +
// the realtime status subscription live in useImChannelsSettings; only pure-UI
// state (active tab, open row, adding-to-type) stays here. Migrated out of
// components/settings/ during the IM-channel migration. No direct Tauri IPC
// here; behavior (tabs always visible, error badges, add-new row) preserved.
import { useState } from 'react'
import { ImChannelAccordionRow } from './ImChannelAccordionRow'
import { useImChannelsSettings } from '../../hooks/useImChannelsSettings'

const CHANNEL_TYPES_ORDER = [
  'wecom_bot', 'wechat_ilink', 'email', 'dingtalk', 'feishu', 'webhook',
]

const CHANNEL_TYPE_LABELS: Record<string, string> = {
  wecom_bot:    '企业微信',
  wechat_ilink: '微信 iLink',
  email:        '邮件',
  dingtalk:     '钉钉',
  feishu:       '飞书',
  webhook:      'Webhook',
}

const CHANNEL_DESCRIPTIONS: Record<string, string> = {
  wecom_bot:    '企业微信 Bot 通过 WebSocket 长连接收发消息，每个实例对应一个独立的 Corp App。',
  wechat_ilink: '微信 iLink 通过 HTTP 长轮询桥接个人微信账号，需配合 iLink 桥接服务运行。',
  email:        '通过 SMTP 发送邮件通知，适用于低频告警场景。',
  dingtalk:     '钉钉 Webhook 通知，不支持双向对话。',
  feishu:       '飞书 Webhook 通知，不支持双向对话。',
  webhook:      '通用 HTTP Webhook，POST JSON 到目标 URL。',
}

export function ImChannelsSettings() {
  const { channels, statuses, spaces, handleToggle, handleSaved, handleDelete } =
    useImChannelsSettings()
  const [activeTab, setActiveTab] = useState<string | null>(null)
  const [openRowId, setOpenRowId] = useState<string | null>(null)
  const [addingToType, setAddingToType] = useState<string | null>(null)

  // Group channels by type
  const channelsByType: Record<string, typeof channels> = {}
  for (const ch of channels) {
    if (!channelsByType[ch.channelType]) channelsByType[ch.channelType] = []
    channelsByType[ch.channelType].push(ch)
  }

  // All channel types are always visible as tabs regardless of instance count.
  const allTabs = CHANNEL_TYPES_ORDER
  const currentTab = (activeTab && allTabs.includes(activeTab)) ? activeTab : allTabs[0]

  function handleToggleRow(id: string) {
    setOpenRowId(prev => (prev === id ? null : id))
    setAddingToType(null)
  }

  function onSaved() {
    setOpenRowId(null)
    setAddingToType(null)
    handleSaved()
  }

  const tabChannels = currentTab ? (channelsByType[currentTab] ?? []) : []

  return (
    <div className="space-y-0">
      {/* Tab bar — all channel types always visible */}
      <div className="flex items-end gap-0 border-b border-border overflow-x-auto">
        {allTabs.map(type => {
          const count = channelsByType[type]?.length ?? 0
          const hasError = (channelsByType[type] ?? []).some(
            ch => statuses[ch.id]?.state === 'error'
          )
          return (
            <button
              key={type}
              onClick={() => { setActiveTab(type); setOpenRowId(null); setAddingToType(null) }}
              className={[
                'flex items-center gap-1.5 whitespace-nowrap px-3 py-2 text-sm border-b-2 transition-colors',
                currentTab === type
                  ? 'border-primary font-medium text-foreground'
                  : 'border-transparent text-muted-foreground hover:text-foreground',
              ].join(' ')}
            >
              {CHANNEL_TYPE_LABELS[type] ?? type}
              {count > 0 && (
                <span className={[
                  'rounded-full px-1.5 py-0.5 text-xs font-medium leading-none',
                  hasError
                    ? 'bg-destructive text-destructive-foreground'
                    : 'bg-muted text-muted-foreground',
                ].join(' ')}>
                  {count}
                </span>
              )}
            </button>
          )
        })}
      </div>

      <div className="pt-3 space-y-1.5">
        {CHANNEL_DESCRIPTIONS[currentTab] && (
          <p className="text-xs text-muted-foreground px-1 pb-2">
            {CHANNEL_DESCRIPTIONS[currentTab]}
          </p>
        )}

        {tabChannels.map(ch => (
          <ImChannelAccordionRow
            key={ch.id}
            channel={ch}
            status={statuses[ch.id]}
            spaces={spaces}
            open={openRowId === ch.id}
            onToggleOpen={() => handleToggleRow(ch.id)}
            onToggleEnabled={(enabled) => handleToggle(ch.id, enabled)}
            onSaved={onSaved}
            onDeleted={() => handleDelete(ch.id)}
          />
        ))}

        {/* New instance row */}
        {addingToType === currentTab ? (
          <ImChannelAccordionRow
            key="__new__"
            channel={undefined}
            newChannelType={currentTab}
            status={undefined}
            spaces={spaces}
            open={true}
            onToggleOpen={() => setAddingToType(null)}
            onToggleEnabled={(_enabled: boolean) => {}}
            onSaved={onSaved}
            onDeleted={() => setAddingToType(null)}
          />
        ) : (
          <button
            onClick={() => { setAddingToType(currentTab); setOpenRowId(null) }}
            className="flex w-full items-center gap-2 rounded border border-dashed border-border px-3 py-2 text-sm text-primary opacity-70 hover:opacity-100 transition-opacity"
          >
            <span className="text-base leading-none">+</span>
            新增{CHANNEL_TYPE_LABELS[currentTab] ?? currentTab}实例
          </button>
        )}
      </div>
    </div>
  )
}
