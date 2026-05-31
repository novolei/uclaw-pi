import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { ActiveTasksBar } from './ActiveTasksBar'
import type { BackgroundTask } from '@/atoms/agent-atoms'

const task = (over: Partial<BackgroundTask> = {}): BackgroundTask => ({
  id: 't1',
  type: 'shell',
  toolUseId: 'tool-1',
  startTime: 0,
  elapsedSeconds: 0,
  ...over,
})

describe('ActiveTasksBar', () => {
  it('renders nothing when there are no tasks', () => {
    const { container } = renderWithProviders(
      <ActiveTasksBar sessionId="s1" tasks={[]} onTaskClick={vi.fn()} />,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders the running-tasks label and a TaskBadge per task', () => {
    renderWithProviders(
      <ActiveTasksBar
        sessionId="s1"
        tasks={[task({ intent: 'lint' })]}
        onTaskClick={vi.fn()}
      />,
    )
    expect(screen.getByText('运行中任务:')).toBeInTheDocument()
  })
})
