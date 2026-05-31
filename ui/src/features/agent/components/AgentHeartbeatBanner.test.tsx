import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { AgentHeartbeatBanner } from './AgentHeartbeatBanner'

// All event subscriptions + IPC route through the agent bridge (via the
// useAgentHeartbeat hook). Mock them: the subscriptions are inert no-ops; the
// pull-on-mount recovery probe drives the visible recovery banner.
const consumePendingRecovery = vi.fn().mockResolvedValue(null)
const dismissPendingRecovery = vi.fn().mockResolvedValue(undefined)

vi.mock('@/lib/bridge/agent', () => ({
  onAgentHeartbeat: vi.fn().mockResolvedValue(() => {}),
  onAgentStalled: vi.fn().mockResolvedValue(() => {}),
  onAgentStallRecovered: vi.fn().mockResolvedValue(() => {}),
  onAgentInterruptedRecovered: vi.fn().mockResolvedValue(() => {}),
  onChatStreamComplete: vi.fn().mockResolvedValue(() => {}),
  consumePendingRecovery: (...args: unknown[]) => consumePendingRecovery(...args),
  dismissPendingRecovery: (...args: unknown[]) => dismissPendingRecovery(...args),
  interruptCurrentAgentRun: vi.fn().mockResolvedValue({}),
}))

describe('AgentHeartbeatBanner', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    consumePendingRecovery.mockResolvedValue(null)
  })

  it('renders inertly (no banner) when there is no pending recovery / heartbeat', async () => {
    const { container } = renderWithProviders(<AgentHeartbeatBanner sessionId="s1" />)
    // Probes the backend on mount; nothing pending → no visible block.
    await waitFor(() => expect(consumePendingRecovery).toHaveBeenCalledWith('s1'))
    expect(container.querySelector('[role="status"]')).toBeNull()
    expect(container.querySelector('[role="alert"]')).toBeNull()
  })

  it('renders the boot-time recovery banner when consumePendingRecovery resolves a payload', async () => {
    consumePendingRecovery.mockResolvedValue({
      conversationId: 's1',
      spaceId: '',
      iteration: 3,
      stage: 'llm_stream',
      startedAt: 0,
      lastActivityAt: 0,
      partialText: '半截回复内容',
      partialChars: 6,
      deadPid: 1234,
    })
    renderWithProviders(<AgentHeartbeatBanner sessionId="s1" />)
    expect(await screen.findByText('上一轮被异常中断 — 已恢复部分回复')).toBeInTheDocument()
    expect(screen.getByText('半截回复内容')).toBeInTheDocument()
  })
})
