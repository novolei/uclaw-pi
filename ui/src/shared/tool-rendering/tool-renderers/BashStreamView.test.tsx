import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { BashStreamView } from './BashStreamView'
import type { LiveOutput } from '@/atoms/agent-atoms'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

describe('BashStreamView', () => {
  it('renders stdout and stderr segments', () => {
    const live: LiveOutput = {
      segments: [
        { stream: 'stdout', text: 'building...\n' },
        { stream: 'stderr', text: 'warning: x\n' },
      ],
      bytes: 22,
      droppedHead: false,
    }
    render(<BashStreamView command="npm run build" live={live} logPath={undefined} />)
    expect(screen.getByText(/building/)).toBeInTheDocument()
    expect(screen.getByText(/warning: x/)).toBeInTheDocument()
  })

  it('shows truncation affordance + load button when droppedHead with logPath', () => {
    const live: LiveOutput = { segments: [{ stream: 'stdout', text: 'tail' }], bytes: 4, droppedHead: true }
    render(<BashStreamView command="cat big" live={live} logPath="/home/u/.uclaw/temp/bash-x.log" />)
    expect(screen.getByText(/早期输出已截断/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /加载完整日志/ })).toBeInTheDocument()
  })
})
