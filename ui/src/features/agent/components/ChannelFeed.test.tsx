import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { ChannelFeed } from './ChannelFeed'
import type { TeamChannelMessage } from '@/lib/bridge/agent'

// ChannelFeed auto-scrolls via scrollIntoView; jsdom doesn't implement it.
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = vi.fn()
}

const msg = (over: Partial<TeamChannelMessage> = {}): TeamChannelMessage => ({
  id: 'm1',
  fromRole: 'worker',
  toRole: null,
  message: 'hello team',
  createdAt: 0,
  ...over,
})

describe('ChannelFeed', () => {
  it('renders the empty state with no messages', () => {
    renderWithProviders(<ChannelFeed messages={[]} />)
    expect(screen.getByText('No messages yet.')).toBeInTheDocument()
  })

  it('renders a row per channel message', () => {
    renderWithProviders(<ChannelFeed messages={[msg()]} />)
    expect(screen.getByText('hello team')).toBeInTheDocument()
  })
})
