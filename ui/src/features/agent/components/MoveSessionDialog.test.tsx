import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { Provider } from 'jotai'
import { MoveSessionDialog } from './MoveSessionDialog'

const moveAgentSessionToWorkspace = vi.fn().mockResolvedValue({ id: 's-1', workspaceId: 'ws-b' })
vi.mock('@/lib/bridge/agent', () => ({
  moveAgentSessionToWorkspace: (args: unknown) => moveAgentSessionToWorkspace(args),
}))

describe('MoveSessionDialog', () => {
  it('renders dialog title when open', () => {
    const workspaces = [
      { id: 'ws-a', name: 'A', icon: '📁', path: null, createdAt: 0, updatedAt: 0 },
      { id: 'ws-b', name: 'B', icon: '📁', path: null, createdAt: 0, updatedAt: 0 },
      { id: 'ws-c', name: 'C', icon: '📁', path: null, createdAt: 0, updatedAt: 0 },
    ]
    const onMoved = vi.fn()
    render(
      <Provider>
        <MoveSessionDialog
          open
          onOpenChange={() => {}}
          sessionId="s-1"
          currentWorkspaceId="ws-a"
          workspaces={workspaces}
          onMoved={onMoved}
        />
      </Provider>
    )
    // Confirm dialog renders without errors and shows its title.
    expect(screen.getByText(/迁移到其他工作区/)).toBeTruthy()
  })
})
