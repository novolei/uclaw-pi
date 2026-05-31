import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import type { ImChannelRow, ImChannelStatus } from '@/atoms/im-channel-atoms'
import { ImChannelAccordionRow } from './ImChannelAccordionRow'

// IPC now flows through settingsBridge (was a raw Tauri-core invoke mock before
// the migration). The wechat_ilink branch also renders WechatIlinkBindingPanel,
// which calls the same bridge — so the binding methods are stubbed here too.
vi.mock('../../../../lib/bridge/settings', () => ({
  settingsBridge: {
    createImChannel: vi.fn().mockResolvedValue(undefined),
    updateImChannel: vi.fn().mockResolvedValue(undefined),
    toggleImChannel: vi.fn().mockResolvedValue(undefined),
    requestWechatIlinkQrcode: vi.fn().mockResolvedValue({ qrcode: '', qrcode_img_content: '' }),
    pollWechatIlinkQrcodeStatus: vi.fn().mockResolvedValue({ status: 'wait' }),
    saveWechatIlinkToken: vi.fn().mockResolvedValue(undefined),
    disconnectWechatIlink: vi.fn().mockResolvedValue(undefined),
  },
}))
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }))

import { settingsBridge } from '../../../../lib/bridge/settings'
const createMock = vi.mocked(settingsBridge.createImChannel)
const updateMock = vi.mocked(settingsBridge.updateImChannel)

const BASE_CHANNEL: ImChannelRow = {
  id: 'ch-1', spaceId: 'sp-1', channelType: 'wecom_bot', name: '客服机器人',
  config: { corp_id: 'wx99def', agent_id: '1000099' }, enabled: true,
  streaming: false, replyScope: 'all', permissionEnabled: false,
  owners: [], guestPolicy: { tool_allowlist: [], mcp_enabled: false },
  createdAt: 1_700_000_000_000, updatedAt: 1_700_000_000_000,
}
const SPACES = [{ id: 'sp-1', name: '工作区' }]

function renderRow(overrides: {
  channel?: ImChannelRow | null
  status?: ImChannelStatus
  open?: boolean
  newChannelType?: string
} = {}) {
  const onToggleOpen = vi.fn()
  const onToggleEnabled = vi.fn()
  const onSaved = vi.fn()
  const onDeleted = vi.fn()
  // Use null as a sentinel for "explicitly undefined channel" (new-instance mode).
  // When channel is not specified in overrides, fall back to BASE_CHANNEL.
  const channelProp = 'channel' in overrides
    ? (overrides.channel === null ? undefined : overrides.channel)
    : BASE_CHANNEL
  renderWithProviders(
    <ImChannelAccordionRow
      channel={channelProp}
      newChannelType={overrides.newChannelType}
      status={overrides.status}
      spaces={SPACES}
      open={overrides.open ?? false}
      onToggleOpen={onToggleOpen}
      onToggleEnabled={onToggleEnabled}
      onSaved={onSaved}
      onDeleted={onDeleted}
    />
  )
  return { onToggleOpen, onToggleEnabled, onSaved, onDeleted }
}

beforeEach(() => {
  createMock.mockClear().mockResolvedValue(undefined)
  updateMock.mockClear().mockResolvedValue(undefined)
})

describe('ImChannelAccordionRow', () => {
  it('renders channel name in closed state', () => {
    renderRow()
    expect(screen.getByText('客服机器人')).not.toBeNull()
  })

  it('shows error badge in closed state when status is error', () => {
    renderRow({
      status: { instanceId: 'ch-1', state: 'error', lastError: '认证失败 xyz' },
    })
    // The badge renders the first 10 chars of lastError
    const badge = screen.getByText('认证失败 xyz'.slice(0, 10))
    expect(badge.tagName).toBe('SPAN')
    expect(badge.className).toMatch(/destructive/)
  })

  it('renders status block in open state for online channel', () => {
    renderRow({
      open: true,
      status: { instanceId: 'ch-1', state: 'online', connectedSinceMs: Date.now() - 60000 },
    })
    expect(screen.getByText(/WebSocket 已连接/)).not.toBeNull()
  })

  it('renders status block with error message in open state', () => {
    renderRow({
      open: true,
      status: { instanceId: 'ch-1', state: 'error', lastError: 'corp_secret 过期' },
    })
    expect(screen.getByText(/连接错误/)).not.toBeNull()
    expect(screen.getByText('corp_secret 过期')).not.toBeNull()
  })

  it('save button is disabled when not dirty', () => {
    renderRow({ open: true })
    const saveBtn = screen.getByRole('button', { name: '保存' })
    expect(saveBtn.hasAttribute('disabled')).toBe(true)
  })

  it('save button enables after changing name', async () => {
    renderRow({ open: true })
    const nameInput = screen.getByPlaceholderText('我的企微机器人')
    fireEvent.change(nameInput, { target: { value: '新名称' } })
    await waitFor(() => {
      const saveBtn = screen.getByRole('button', { name: '保存' })
      expect(saveBtn.hasAttribute('disabled')).toBe(false)
    })
  })

  it('shows 保存并重连 when dirty and channel is online', async () => {
    renderRow({
      open: true,
      status: { instanceId: 'ch-1', state: 'online' },
    })
    const nameInput = screen.getByPlaceholderText('我的企微机器人')
    fireEvent.change(nameInput, { target: { value: '改名' } })
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '保存并重连' })).not.toBeNull()
    })
  })

  it('calls update_im_channel on save', async () => {
    renderRow({ open: true })
    const nameInput = screen.getByPlaceholderText('我的企微机器人')
    fireEvent.change(nameInput, { target: { value: '新名称' } })
    await waitFor(() => {
      fireEvent.click(screen.getByRole('button', { name: '保存' }))
    })
    await waitFor(() => {
      expect(updateMock).toHaveBeenCalledWith(
        'ch-1',
        expect.objectContaining({ name: '新名称' })
      )
    })
  })

  it('calls create_im_channel in new-instance mode', async () => {
    renderRow({ channel: null, newChannelType: 'wecom_bot', open: true })
    const nameInput = screen.getByPlaceholderText('我的企微机器人')
    fireEvent.change(nameInput, { target: { value: '新机器人' } })
    await waitFor(() => {
      fireEvent.click(screen.getByRole('button', { name: '保存' }))
    })
    await waitFor(() => {
      expect(createMock).toHaveBeenCalledWith(expect.objectContaining({ name: '新机器人' }))
    })
  })

  it('wechat_ilink expanded shows binding panel, not app_id field', () => {
    renderRow({
      channel: {
        ...BASE_CHANNEL,
        channelType: 'wechat_ilink',
        config: { account_id: 'wx_user_abc' },
      },
      open: true,
    })
    // Should show binding panel content (confirmed state since accountId present)
    expect(screen.getByText('重新绑定')).not.toBeNull()
    // Must NOT show the old app_id or api_key form fields
    expect(screen.queryByText('App ID')).toBeNull()
    expect(screen.queryByText('API Key')).toBeNull()
  })
})
