import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { createStore } from 'jotai'
import { imChannelsAtom, imChannelStatusesAtom } from '@/atoms/im-channel-atoms'
import type { ImChannelRow, ImChannelStatus } from '@/atoms/im-channel-atoms'
import { ImChannelsSettings } from './ImChannelsSettings'

// The component + accordion row route IPC through settingsBridge (mocked below).
// The list/status *atoms* still own their own Tauri-core invoke (they live
// outside the feature, in @/atoms/im-channel-atoms), so their boundary is
// stubbed here — this Tauri-core mock is for the out-of-feature atoms, not the
// migrated component. `onImChannelStatusChanged` is the named realtime export.
const invokeMock = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }))
vi.mock('../../../../lib/bridge/settings', () => ({
  settingsBridge: {
    listSpaces: vi.fn().mockResolvedValue([]),
    toggleImChannel: vi.fn().mockResolvedValue(undefined),
    deleteImChannel: vi.fn().mockResolvedValue(undefined),
    createImChannel: vi.fn().mockResolvedValue(undefined),
    updateImChannel: vi.fn().mockResolvedValue(undefined),
  },
  onImChannelStatusChanged: vi.fn(() => Promise.resolve(() => {})),
}))
vi.mock('sonner', () => ({ toast: { error: vi.fn() } }))

import { settingsBridge } from '../../../../lib/bridge/settings'
const toggleMock = vi.mocked(settingsBridge.toggleImChannel)

const makeChannel = (overrides: Partial<ImChannelRow> = {}): ImChannelRow => ({
  id: 'ch-1', spaceId: 'sp-1', channelType: 'wecom_bot', name: '产品组机器人',
  config: { corp_id: 'wx12abc', agent_id: '1000042' }, enabled: true,
  streaming: false, replyScope: 'all', permissionEnabled: false,
  owners: [], guestPolicy: { tool_allowlist: [], mcp_enabled: false },
  createdAt: 1_700_000_000_000, updatedAt: 1_700_000_000_000,
  ...overrides,
})

beforeEach(() => {
  invokeMock.mockReset()
  // Default: list_im_channels, get_im_channel_statuses all return empty
  invokeMock.mockResolvedValue([])
  toggleMock.mockClear().mockResolvedValue(undefined)
})

describe('ImChannelsSettings', () => {
  it('renders tab with instance count badge', async () => {
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel()])
    renderWithProviders(<ImChannelsSettings />, { store })
    expect(screen.getByText('企业微信')).not.toBeNull()
    expect(screen.getByText('1')).not.toBeNull()
  })

  it('shows error badge on tab when any instance has error status', async () => {
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel({ id: 'ch-err' })])
    store.set(imChannelStatusesAtom, {
      'ch-err': { instanceId: 'ch-err', state: 'error', lastError: '认证失败' } as ImChannelStatus,
    })
    renderWithProviders(<ImChannelsSettings />, { store })
    const badge = screen.getByText('1')
    expect(badge.className).toMatch(/destructive/)
  })

  it('renders instance name in the list', async () => {
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel({ name: '测试机器人' })])
    renderWithProviders(<ImChannelsSettings />, { store })
    expect(screen.getByText('测试机器人')).not.toBeNull()
  })

  it('renders add-new dashed button for current tab', () => {
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel()])
    renderWithProviders(<ImChannelsSettings />, { store })
    expect(screen.getByText(/新增企业微信实例/)).not.toBeNull()
  })

  it('calls toggle_im_channel and optimistically updates enabled state', async () => {
    const store = createStore()
    store.set(imChannelsAtom, [makeChannel({ enabled: true })])
    renderWithProviders(<ImChannelsSettings />, { store })
    const toggleBtn = screen.getByRole('button', { name: '停用' })
    fireEvent.click(toggleBtn)
    await waitFor(() => {
      expect(toggleMock).toHaveBeenCalledWith('ch-1', false)
    })
  })

  it('reverts optimistic toggle on invoke failure', async () => {
    const ch = makeChannel({ enabled: true })
    // toggle rejects → hook re-fetches channels (the atom's list_im_channels invoke)
    toggleMock.mockRejectedValue(new Error('network error'))
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'list_im_channels') return Promise.resolve([ch])
      return Promise.resolve([])
    })
    const store = createStore()
    store.set(imChannelsAtom, [ch])
    renderWithProviders(<ImChannelsSettings />, { store })
    const toggleBtn = screen.getByRole('button', { name: '停用' })
    fireEvent.click(toggleBtn)
    // After revert, the toggle should be back to '停用' (enabled=true restored)
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '停用' })).not.toBeNull()
    })
  })
})
