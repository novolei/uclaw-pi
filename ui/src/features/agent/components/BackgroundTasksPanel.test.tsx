import { describe, it, expect } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { BackgroundTasksPanel } from './BackgroundTasksPanel'
import type { BackgroundTask } from '@/atoms/agent-atoms'

const task = (over: Partial<BackgroundTask> = {}): BackgroundTask => ({
  id: 't1',
  type: 'shell',
  toolUseId: 'tool-1',
  startTime: 0,
  elapsedSeconds: 0,
  ...over,
})

describe('BackgroundTasksPanel', () => {
  it('renders nothing when there are no tasks', () => {
    const { container } = renderWithProviders(<BackgroundTasksPanel tasks={[]} />)
    expect(container.firstChild).toBeNull()
  })

  it('renders a row per running task with its intent', () => {
    renderWithProviders(
      <BackgroundTasksPanel tasks={[task({ intent: 'build the app' })]} />,
    )
    expect(screen.getByText(/1 个后台任务/)).toBeInTheDocument()
    expect(screen.getByText('build the app')).toBeInTheDocument()
    expect(screen.getByText('运行中')).toBeInTheDocument()
  })
})
