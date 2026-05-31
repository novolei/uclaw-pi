import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { QueuedMessagesBanner } from './QueuedMessagesBanner'
import type { QueuedAgentMessage } from '@/atoms/agent-queue-messages'

// Purely presentational — actions arrive as callback props, so there is no
// bridge/IPC to mock. We assert it renders the queued rows + wires 引导.

const MSGS: QueuedAgentMessage[] = [
  { id: 'q1', text: 'first queued message', queuedAt: Date.now() },
  { id: 'q2', text: 'second queued message', queuedAt: Date.now() },
]

describe('QueuedMessagesBanner', () => {
  it('renders nothing when the queue is empty', () => {
    const { container } = renderWithProviders(
      <QueuedMessagesBanner messages={[]} onSteer={vi.fn()} onEdit={vi.fn()} onDelete={vi.fn()} />,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders one row per queued message with the count header', () => {
    renderWithProviders(
      <QueuedMessagesBanner messages={MSGS} onSteer={vi.fn()} onEdit={vi.fn()} onDelete={vi.fn()} />,
    )
    expect(screen.getByText('first queued message')).toBeInTheDocument()
    expect(screen.getByText('second queued message')).toBeInTheDocument()
    expect(screen.getByText(/2 条排队消息/)).toBeInTheDocument()
  })

  it('引导 fires onSteer with the row message', async () => {
    const onSteer = vi.fn()
    const { user } = renderWithProviders(
      <QueuedMessagesBanner messages={[MSGS[0]]} onSteer={onSteer} onEdit={vi.fn()} onDelete={vi.fn()} />,
    )
    await user.click(screen.getByText('引导'))
    expect(onSteer).toHaveBeenCalledWith(MSGS[0])
  })
})
