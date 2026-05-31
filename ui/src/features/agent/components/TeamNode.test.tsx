import { describe, it, expect } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { TeamNode } from './TeamNode'
import type { TeamNode as TeamNodeType } from '@/atoms/agent-teams'

const node = (over: Partial<TeamNodeType> = {}): TeamNodeType => ({
  id: 'n1',
  role: 'worker',
  label: 'Builder',
  status: 'running',
  ...over,
})

describe('TeamNode', () => {
  it('renders the node label and status', () => {
    renderWithProviders(<TeamNode node={node()} />)
    expect(screen.getByText('Builder')).toBeInTheDocument()
    expect(screen.getByText('Running')).toBeInTheDocument()
  })

  it('renders the last message when present', () => {
    renderWithProviders(
      <TeamNode node={node({ status: 'done', lastMessage: 'all green' })} />,
    )
    expect(screen.getByText('all green')).toBeInTheDocument()
    expect(screen.getByText('Done')).toBeInTheDocument()
  })
})
