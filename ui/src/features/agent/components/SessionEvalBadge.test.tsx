import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { SessionEvalBadge } from './SessionEvalBadge'
import type { SessionEvalPayload } from '@/lib/bridge/agent'

// Capture the handlers the component registers so the test can drive events
// through the (mocked) agent-bridge wrappers instead of @tauri-apps/api.
let completeHandler: ((p: SessionEvalPayload) => void) | undefined
let warningHandler: ((p: SessionEvalPayload) => void) | undefined

vi.mock('@/lib/bridge/agent', () => ({
  onSessionEvalComplete: vi.fn((h: (p: SessionEvalPayload) => void) => {
    completeHandler = h
    return Promise.resolve(() => {})
  }),
  onSessionEvalWarning: vi.fn((h: (p: SessionEvalPayload) => void) => {
    warningHandler = h
    return Promise.resolve(() => {})
  }),
}))

const SESSION_ID = 'sess-eval-1'

describe('SessionEvalBadge', () => {
  beforeEach(() => {
    completeHandler = undefined
    warningHandler = undefined
  })

  it('renders nothing before an eval arrives', () => {
    const { container } = renderWithProviders(<SessionEvalBadge sessionId={SESSION_ID} />)
    expect(container.textContent).toBe('')
  })

  it('subscribes through the agent-bridge wrappers (not @tauri-apps/api)', async () => {
    const bridge = await import('@/lib/bridge/agent')
    renderWithProviders(<SessionEvalBadge sessionId={SESSION_ID} />)
    expect(bridge.onSessionEvalComplete).toHaveBeenCalled()
    expect(bridge.onSessionEvalWarning).toHaveBeenCalled()
  })

  it('renders the score once an eval-complete event for this session fires', async () => {
    renderWithProviders(<SessionEvalBadge sessionId={SESSION_ID} />)
    await waitFor(() => expect(completeHandler).toBeDefined())
    completeHandler!({ sessionId: SESSION_ID, score: 0.8, reasoning: 'looked good' })
    expect(await screen.findByText('Self-Evaluation')).toBeInTheDocument()
    expect(screen.getByText('80%')).toBeInTheDocument()
  })

  it('ignores eval events addressed to a different session', async () => {
    const { container } = renderWithProviders(<SessionEvalBadge sessionId={SESSION_ID} />)
    await waitFor(() => expect(completeHandler).toBeDefined())
    completeHandler!({ sessionId: 'other', score: 0.9, reasoning: 'nope' })
    expect(container.textContent).toBe('')
  })
})
