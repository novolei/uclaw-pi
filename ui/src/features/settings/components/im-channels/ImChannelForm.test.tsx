import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { ImChannelForm } from './ImChannelForm'

// IPC flows through settingsBridge; stub the create/update used on save.
vi.mock('../../../../lib/bridge/settings', () => ({
  settingsBridge: {
    createImChannel: vi.fn().mockResolvedValue(undefined),
    updateImChannel: vi.fn().mockResolvedValue(undefined),
  },
}))

const SPACES = [{ id: 'sp-1', name: '工作区' }]

describe('ImChannelForm', () => {
  it('renders the channel-type selector and name field for add mode', () => {
    renderWithProviders(<ImChannelForm spaces={SPACES} onDone={() => {}} />)
    expect(screen.getByText('渠道类型')).not.toBeNull()
    expect(screen.getByPlaceholderText('我的企微机器人')).not.toBeNull()
  })

  it('shows the webhook URL field by default (webhook type)', () => {
    renderWithProviders(<ImChannelForm spaces={SPACES} onDone={() => {}} />)
    expect(screen.getByPlaceholderText('https://example.com/hook')).not.toBeNull()
  })
})
