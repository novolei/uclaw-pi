import { describe, it, expect } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { TaskProgressCard, TASK_TOOL_NAMES } from './TaskProgressCard'
import type { ToolActivity } from '@/atoms/agent-atoms'

function todoWrite(todos: Array<Record<string, unknown>>): ToolActivity {
  return {
    toolUseId: 'tw-1',
    toolName: 'TodoWrite',
    input: { todos },
    done: true,
  }
}

describe('TaskProgressCard', () => {
  it('exports the task-tool name set the renderers gate on', () => {
    expect(TASK_TOOL_NAMES.has('TaskCreate')).toBe(true)
    expect(TASK_TOOL_NAMES.has('TaskUpdate')).toBe(true)
    expect(TASK_TOOL_NAMES.has('TodoWrite')).toBe(true)
  })

  it('renders nothing when there are no task activities', () => {
    const { container } = renderWithProviders(<TaskProgressCard activities={[]} />)
    expect(container.textContent).toBe('')
  })

  it('aggregates TodoWrite items into the progress card with a count', () => {
    renderWithProviders(
      <TaskProgressCard
        activities={[
          todoWrite([
            { subject: '写测试', status: 'completed' },
            { subject: '改实现', status: 'in_progress' },
          ]),
        ]}
      />,
    )
    expect(screen.getByText('任务进度')).toBeInTheDocument()
    expect(screen.getByText('1/2')).toBeInTheDocument()
    expect(screen.getByText('改实现')).toBeInTheDocument()
  })
})
