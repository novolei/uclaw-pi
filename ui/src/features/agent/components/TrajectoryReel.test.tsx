import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, waitFor } from '@/test-utils/render'
import { TrajectoryReel } from './TrajectoryReel'

// TrajectoryReel + its nested SessionEvalBadge both talk to the agent bridge.
// Stub the whole module so the render test never reaches `@tauri-apps/api`.
const getSessionTrajectory = vi.fn()
vi.mock('@/lib/bridge/agent', () => ({
  getSessionTrajectory: (...args: unknown[]) => getSessionTrajectory(...args),
  onSessionEvalComplete: vi.fn().mockResolvedValue(() => {}),
  onSessionEvalWarning: vi.fn().mockResolvedValue(() => {}),
}))

describe('TrajectoryReel', () => {
  beforeEach(() => {
    getSessionTrajectory.mockReset()
  })

  it('shows the loading state first', () => {
    getSessionTrajectory.mockReturnValue(new Promise(() => {}))
    const { getByText } = renderWithProviders(<TrajectoryReel sessionId="s1" />)
    expect(getByText(/Loading trajectory/)).toBeTruthy()
  })

  it('renders the empty state when no turns are recorded', async () => {
    getSessionTrajectory.mockResolvedValue([])
    const { findByText } = renderWithProviders(<TrajectoryReel sessionId="s1" />)
    expect(await findByText(/No turns recorded/)).toBeTruthy()
  })

  it('renders turn rows when the session has turns', async () => {
    getSessionTrajectory.mockResolvedValue([
      {
        id: 't1',
        sessionId: 's1',
        turnIndex: 0,
        role: 'assistant',
        content: 'hello there',
        toolName: 'bash',
        isError: false,
        durationMs: 1200,
        createdAt: 0,
      },
    ])
    const { findByText, getByText } = renderWithProviders(<TrajectoryReel sessionId="s1" />)
    expect(await findByText(/hello there/)).toBeTruthy()
    expect(getByText('bash')).toBeTruthy()
  })

  it('surfaces the error state on failure', async () => {
    getSessionTrajectory.mockRejectedValue(new Error('boom'))
    const { findByText } = renderWithProviders(<TrajectoryReel sessionId="s1" />)
    await waitFor(() => expect(findByText(/Failed to load trajectory/)).toBeTruthy())
  })
})
