import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { ExitPlanModeBanner } from './ExitPlanModeBanner'
import { allPendingExitPlanRequestsAtom } from '@/atoms/agent-atoms'
import type { ExitPlanModeRequest } from '@/lib/agent-types'

// IPC routes through the agent bridge.
vi.mock('@/lib/bridge/agent', () => ({
  respondExitPlanMode: vi.fn().mockResolvedValue(undefined),
  stopAgent: vi.fn().mockResolvedValue(undefined),
}))

const REQ: ExitPlanModeRequest = {
  requestId: 'plan-1',
  sessionId: 's1',
  plan: '# 计划\n\n1. 先读文件\n2. 再改代码',
  allowedPrompts: [],
}

function seeded() {
  const store = createStore()
  store.set(allPendingExitPlanRequestsAtom, new Map([['s1', [REQ]]]))
  return store
}

describe('ExitPlanModeBanner', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('renders nothing when no plan request is pending', () => {
    const { container } = renderWithProviders(<ExitPlanModeBanner sessionId="s1" />)
    expect(container.firstChild).toBeNull()
  })

  it('renders the plan-approval banner with the rendered plan markdown', () => {
    renderWithProviders(<ExitPlanModeBanner sessionId="s1" />, { store: seeded() })
    expect(screen.getByText('Agent 计划待审批')).toBeInTheDocument()
    expect(screen.getByText('先读文件')).toBeInTheDocument()
    expect(screen.getByText('接受 + 切到 Auto 执行')).toBeInTheDocument()
  })

  it('接受 + 切到 Auto 执行 routes accept_and_auto through respondExitPlanMode', async () => {
    const bridge = await import('@/lib/bridge/agent')
    const { user } = renderWithProviders(<ExitPlanModeBanner sessionId="s1" />, { store: seeded() })
    await user.click(screen.getByText('接受 + 切到 Auto 执行'))
    await waitFor(() => {
      expect(bridge.respondExitPlanMode).toHaveBeenCalledWith({
        requestId: 'plan-1',
        sessionId: 's1',
        decision: 'accept_and_auto',
        allowedPrompts: [],
      })
    })
  })
})
