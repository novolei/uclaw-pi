import { describe, it, expect, vi } from 'vitest'
import { createStore } from 'jotai'
import { renderWithProviders, screen } from '@/test-utils/render'
import { AgentHeader } from './AgentHeader'
import { agentSessionsAtom } from '@/atoms/agent-atoms'
import type { AgentSessionMeta } from '@/lib/agent-types'

vi.mock('@/lib/bridge/agent', () => ({
  updateAgentSessionTitle: vi.fn().mockResolvedValue({}),
  listAgentSessions: vi.fn().mockResolvedValue([]),
}))

const SESSION_ID = 'sess-1'

function makeSession(overrides: Partial<AgentSessionMeta> = {}): AgentSessionMeta {
  return {
    id: SESSION_ID,
    title: '我的会话',
    messageCount: 0,
    createdAt: 0,
    updatedAt: 0,
    ...overrides,
  }
}

describe('AgentHeader', () => {
  it('renders nothing when the session is not in the store', () => {
    const { container } = renderWithProviders(<AgentHeader sessionId="missing" />)
    expect(container.textContent).toBe('')
  })

  it('renders the session title and an edit affordance', () => {
    const store = createStore()
    store.set(agentSessionsAtom, [makeSession()])
    renderWithProviders(<AgentHeader sessionId={SESSION_ID} />, { store })
    expect(screen.getByText('我的会话')).toBeInTheDocument()
    expect(screen.getByLabelText('编辑标题')).toBeInTheDocument()
  })

  it('enters edit mode and persists via the agent bridge on Enter', async () => {
    const bridge = await import('@/lib/bridge/agent')
    const store = createStore()
    store.set(agentSessionsAtom, [makeSession()])
    const { user } = renderWithProviders(<AgentHeader sessionId={SESSION_ID} />, { store })

    await user.click(screen.getByLabelText('编辑标题'))
    const input = screen.getByDisplayValue('我的会话')
    await user.clear(input)
    await user.type(input, '新标题{Enter}')

    expect(bridge.updateAgentSessionTitle).toHaveBeenCalledWith(SESSION_ID, '新标题')
  })
})
