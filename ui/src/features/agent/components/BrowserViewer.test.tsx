import { describe, it, expect } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { BrowserViewer } from './BrowserViewer'

describe('BrowserViewer', () => {
  it('renders the no-session message when no Agent session is selected', () => {
    // Default store has currentAgentSessionIdAtom = null, so BrowserViewer
    // short-circuits to the empty state and never mounts BrowserPanel (no IPC).
    renderWithProviders(<BrowserViewer />)
    expect(screen.getByText('当前没有选中的 Agent 会话')).toBeInTheDocument()
  })
})
