import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { AgentStatusBar } from './AgentStatusBar'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { agentStreamingStatesAtom, type AgentStreamState } from '@/atoms/agent-atoms'

vi.mock('@/lib/bridge/agent', () => ({
  stopAgent: vi.fn(async () => {}),
}))

const SESSION_ID = 'sess-test-1'

function makeStream(overrides: Partial<AgentStreamState> = {}): AgentStreamState {
  return {
    running: true,
    content: '',
    toolActivities: [],
    teammates: [],
    startedAt: Date.now() - 5000,
    ...overrides,
  }
}

describe('AgentStatusBar', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('renders nothing when no stream exists for this session', () => {
    const { container } = renderWithProviders(<AgentStatusBar sessionId={SESSION_ID} />)
    expect(container.textContent).toBe('')
  })

  it('renders nothing when stream exists but is not running', () => {
    const { store, container } = renderWithProviders(<AgentStatusBar sessionId={SESSION_ID} />)
    store.set(agentStreamingStatesAtom, new Map([
      [SESSION_ID, makeStream({ running: false })],
    ]))
    // Just-completed snapshot only fires after a true running→idle transition,
    // so a stream that's never been running shows nothing.
    expect(container.textContent).toBe('')
  })

  it('shows tool phase with tool name when a tool is in flight', async () => {
    const { store } = renderWithProviders(<AgentStatusBar sessionId={SESSION_ID} />)
    store.set(agentStreamingStatesAtom, new Map([
      [SESSION_ID, makeStream({
        toolActivities: [{
          toolUseId: 't1',
          toolName: 'bash',
          input: { command: 'cargo build' },
          done: false,
        }],
      })],
    ]))
    await waitFor(() => {
      expect(screen.getByText('执行工具')).toBeInTheDocument()
      expect(screen.getByText('bash')).toBeInTheDocument()
      // tool count chip — should appear since toolActivities has 1 entry
      expect(screen.getByText(/1\s*工具/)).toBeInTheDocument()
    })
  })

  it('shows thinking phase when reasoning is set with no in-flight tool', async () => {
    const { store } = renderWithProviders(<AgentStatusBar sessionId={SESSION_ID} />)
    store.set(agentStreamingStatesAtom, new Map([
      [SESSION_ID, makeStream({ reasoning: 'pondering...' })],
    ]))
    await waitFor(() => {
      expect(screen.getByText('思考中')).toBeInTheDocument()
    })
  })

  it('shows retry phase with attempt count', async () => {
    const { store } = renderWithProviders(<AgentStatusBar sessionId={SESSION_ID} />)
    store.set(agentStreamingStatesAtom, new Map([
      [SESSION_ID, makeStream({
        retrying: { currentAttempt: 2, maxAttempts: 3, history: [], failed: false },
      })],
    ]))
    await waitFor(() => {
      expect(screen.getByText('重试中')).toBeInTheDocument()
      expect(screen.getByText(/2\/3/)).toBeInTheDocument()
    })
  })

  it('renders Stop button when running', async () => {
    const { store } = renderWithProviders(<AgentStatusBar sessionId={SESSION_ID} />)
    store.set(agentStreamingStatesAtom, new Map([
      [SESSION_ID, makeStream()],
    ]))
    await waitFor(() => {
      expect(screen.getByText('停止')).toBeInTheDocument()
    })
  })

  it('Stop button calls stopAgent', async () => {
    const bridge = await import('@/lib/bridge/agent')
    const { store, user } = renderWithProviders(<AgentStatusBar sessionId={SESSION_ID} />)
    store.set(agentStreamingStatesAtom, new Map([
      [SESSION_ID, makeStream()],
    ]))
    const stopButton = await screen.findByText('停止')
    await user.click(stopButton)
    expect(bridge.stopAgent).toHaveBeenCalledWith(SESSION_ID)
  })

  it('shows tokens and cost when present', async () => {
    const { store } = renderWithProviders(<AgentStatusBar sessionId={SESSION_ID} />)
    store.set(agentStreamingStatesAtom, new Map([
      [SESSION_ID, makeStream({
        inputTokens: 1234,
        outputTokens: 5678,
        costUsd: 0.0123,
      })],
    ]))
    await waitFor(() => {
      // Token line: "↑1.2k ↓5.7k"
      expect(screen.getByText(/↑1\.2k\s+↓5\.7k/)).toBeInTheDocument()
      // Cost: "$0.0123"
      expect(screen.getByText(/\$0\.0123/)).toBeInTheDocument()
    })
  })
})
