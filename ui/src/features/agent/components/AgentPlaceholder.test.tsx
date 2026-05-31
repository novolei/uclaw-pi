import { describe, it, expect } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { AgentPlaceholder } from './AgentPlaceholder'

describe('AgentPlaceholder', () => {
  it('renders the Agent-mode placeholder copy', () => {
    renderWithProviders(<AgentPlaceholder />)
    expect(screen.getByText('Agent 模式')).toBeInTheDocument()
    expect(screen.getByText('即将推出')).toBeInTheDocument()
  })
})
