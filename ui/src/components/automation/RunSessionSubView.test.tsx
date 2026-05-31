import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent, act } from '@testing-library/react'
import { RunSessionSubView } from './RunSessionSubView'
import type { AutomationActivity } from '@/lib/tauri-bridge'

vi.mock('@/lib/tauri-bridge', () => ({
  getAgentSessionMessages: vi.fn().mockResolvedValue([]),
  openFile: vi.fn(),
  openExternal: vi.fn(),
}))
vi.mock('@/features/agent', () => ({
  AgentMessages: () => <div data-testid="agent-messages" />,
}))

const baseActivity: AutomationActivity = {
  id: 'act-1', specId: 'spec-1', subscriptionId: null,
  triggerSourceType: 'manual', triggerPayloadJson: '{}',
  status: 'completed', errorText: null,
  queuedAt: 1_700_000_000_000, startedAt: null, completedAt: null,
  durationMs: 0, llmIterations: 0, llmTokensIn: 0, llmTokensOut: 0,
  sessionId: 'sess-1', reportArtifactsJson: '[]',
  reportText: '**done**', reportOutcome: 'useful',
  escalationId: null, resumedFromActivityId: null, resumedFromEscalationId: null,
  workingDir: '/workdir',
}

describe('RunSessionSubView', () => {
  it('renders AgentMessages', async () => {
    await act(async () => {
      render(
        <RunSessionSubView sessionId="sess-1" onBack={() => {}} activity={baseActivity} />
      )
    })
    expect(screen.getByTestId('agent-messages')).toBeTruthy()
  })

  it('shows report card when activity has reportText', async () => {
    await act(async () => {
      render(
        <RunSessionSubView sessionId="sess-1" onBack={() => {}} activity={baseActivity} />
      )
    })
    expect(screen.getByText('运行报告')).toBeTruthy()
    expect(screen.getByText('有效')).toBeTruthy()
  })

  it('shows running placeholder when running and no reportText', async () => {
    const running = { ...baseActivity, status: 'running', reportText: null }
    await act(async () => {
      render(
        <RunSessionSubView sessionId="sess-1" isRunning onBack={() => {}} activity={running} />
      )
    })
    expect(screen.getByText(/运行中，暂无报告/)).toBeTruthy()
  })

  it('hides report card when complete and no reportText', async () => {
    const noReport = { ...baseActivity, reportText: null }
    await act(async () => {
      render(
        <RunSessionSubView sessionId="sess-1" onBack={() => {}} activity={noReport} />
      )
    })
    expect(screen.queryByText('运行报告')).toBeNull()
  })

  it('collapses report card on chevron click', async () => {
    await act(async () => {
      render(
        <RunSessionSubView sessionId="sess-1" onBack={() => {}} activity={baseActivity} />
      )
    })
    // Before collapse: markdown content rendered
    expect(document.querySelector('strong')).toBeTruthy()
    // Click the header button to collapse
    fireEvent.click(screen.getByText('运行报告').closest('button')!)
    // After collapse: markdown content gone
    expect(document.querySelector('strong')).toBeNull()
  })
})
