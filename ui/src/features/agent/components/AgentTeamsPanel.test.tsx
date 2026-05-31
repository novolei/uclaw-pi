import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import { renderWithProviders, screen } from '@/test-utils/render'
import { AgentTeamsPanel } from './AgentTeamsPanel'
import { activeTeamAtom, type TeamState } from '@/atoms/agent-teams'
import type { TeamChannelMessage } from '@/lib/bridge/agent'

// The panel subscribes to `agent:team-message` through the agent bridge's
// onTeamMessage wrapper — mock it (no @tauri-apps/api in tests) and capture the
// handler the way the other migrated banner tests do.
let teamMessageHandler: ((p: TeamChannelMessage) => void) | undefined

vi.mock('@/lib/bridge/agent', () => ({
  onTeamMessage: vi.fn((h: (p: TeamChannelMessage) => void) => {
    teamMessageHandler = h
    return Promise.resolve(() => {})
  }),
}))

// The nested ChannelFeed auto-scrolls via scrollIntoView; jsdom lacks it.
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = vi.fn()
}

const team = (): TeamState => ({
  teamId: 't1',
  sessionId: 's1',
  task: 'ship the feature',
  nodes: [{ id: 'n1', role: 'supervisor', label: 'Lead', status: 'running' }],
  messages: [],
  status: 'running',
})

describe('AgentTeamsPanel', () => {
  beforeEach(() => {
    teamMessageHandler = undefined
  })

  it('renders the no-active-team empty state by default', () => {
    renderWithProviders(<AgentTeamsPanel />)
    expect(screen.getByText(/No active Agent Teams session/)).toBeInTheDocument()
  })

  it('renders the task + agent node when a team is active, and subscribes via the bridge', async () => {
    const store = createStore()
    store.set(activeTeamAtom, team())
    const bridge = await import('@/lib/bridge/agent')
    renderWithProviders(<AgentTeamsPanel />, { store })
    expect(screen.getByText('ship the feature')).toBeInTheDocument()
    expect(screen.getByText('Lead')).toBeInTheDocument()
    expect(bridge.onTeamMessage).toHaveBeenCalled()
  })
})
