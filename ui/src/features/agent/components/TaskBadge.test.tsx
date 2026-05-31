import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { TaskBadge } from './TaskBadge'
import type { BackgroundTask } from '@/atoms/agent-atoms'

function makeTask(overrides: Partial<BackgroundTask> = {}): BackgroundTask {
  return {
    id: 'abc123def456',
    type: 'shell',
    toolUseId: 'tool-1',
    startTime: Date.now() - 30_000,
    elapsedSeconds: 30,
    intent: 'cargo build',
    ...overrides,
  }
}

describe('TaskBadge', () => {
  it('renders the shell task pill with a shortened id', () => {
    renderWithProviders(<TaskBadge task={makeTask()} onClick={() => {}} />)
    expect(screen.getByText('Shell')).toBeInTheDocument()
    expect(screen.getByText('abc123de...')).toBeInTheDocument()
  })

  it('renders the Task label for agent (non-shell) tasks', () => {
    renderWithProviders(
      <TaskBadge task={makeTask({ type: 'agent' })} onClick={() => {}} />,
    )
    expect(screen.getByText('Task')).toBeInTheDocument()
  })

  it('invokes onClick when the pill is clicked', async () => {
    const onClick = vi.fn()
    const { user } = renderWithProviders(<TaskBadge task={makeTask()} onClick={onClick} />)
    await user.click(screen.getByRole('button'))
    expect(onClick).toHaveBeenCalledTimes(1)
  })
})
