import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { AskUserBanner } from './AskUserBanner'
import { allPendingAskUserRequestsAtom } from '@/atoms/agent-atoms'
import type { AskUserRequest } from '@/lib/agent-types'

// IPC routes through the agent bridge (via the useAskUserBanner hook).
vi.mock('@/lib/bridge/agent', () => ({
  respondAskUser: vi.fn().mockResolvedValue(undefined),
  stopAgent: vi.fn().mockResolvedValue(undefined),
}))

const REQ: AskUserRequest = {
  requestId: 'ask-1',
  sessionId: 's1',
  questions: [
    {
      question: '选择部署目标',
      header: '部署',
      multiSelect: false,
      options: [
        { label: '生产环境' },
        { label: '预发环境' },
      ],
    },
  ],
}

function seeded() {
  const store = createStore()
  store.set(allPendingAskUserRequestsAtom, new Map([['s1', [REQ]]]))
  return store
}

describe('AskUserBanner', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('renders nothing when no ask request is pending', () => {
    const { container } = renderWithProviders(<AskUserBanner sessionId="s1" />)
    expect(container.firstChild).toBeNull()
  })

  it('renders the question + options when a request is pending', () => {
    renderWithProviders(<AskUserBanner sessionId="s1" />, { store: seeded() })
    expect(screen.getByText('Agent 需要你的输入')).toBeInTheDocument()
    expect(screen.getByText('选择部署目标')).toBeInTheDocument()
    expect(screen.getByText('生产环境')).toBeInTheDocument()
    expect(screen.getByText('预发环境')).toBeInTheDocument()
  })

  it('selecting an option then 确认 submits the answer through respondAskUser', async () => {
    const bridge = await import('@/lib/bridge/agent')
    const { user } = renderWithProviders(<AskUserBanner sessionId="s1" />, { store: seeded() })
    await user.click(screen.getByText('预发环境'))
    await user.click(screen.getByText('确认'))
    await waitFor(() => {
      expect(bridge.respondAskUser).toHaveBeenCalledWith({
        requestId: 'ask-1',
        answers: { '选择部署目标': '预发环境' },
      })
    })
  })
})
