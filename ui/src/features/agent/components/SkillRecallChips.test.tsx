import { describe, it, expect } from 'vitest'
import { Provider as JotaiProvider, createStore } from 'jotai'
import { render, screen } from '@testing-library/react'
import { TooltipProvider } from '@/components/ui/tooltip'
import { SkillRecallChips } from './SkillRecallChips'
import { skillRecallsMapAtom, type SkillRecall } from '@/atoms/agent-atoms'

function renderWith(sessionId: string, recalls: SkillRecall[]) {
  const store = createStore()
  store.set(skillRecallsMapAtom, new Map([[sessionId, recalls]]))
  return render(
    <JotaiProvider store={store}>
      <TooltipProvider>
        <SkillRecallChips sessionId={sessionId} />
      </TooltipProvider>
    </JotaiProvider>
  )
}

describe('SkillRecallChips', () => {
  it('renders nothing when no recalls', () => {
    const { container } = renderWith('s1', [])
    expect(container.firstChild).toBeNull()
  })

  it('renders search chip with query and count', () => {
    renderWith('s1', [{
      toolCallId: 't1',
      kind: 'search',
      timestamp: '2026-05-12T00:00:00Z',
      query: 'stock financials',
      results: [
        { name: 'stock-research', summary: '...', score: 0.8, provenance: 'learned' },
        { name: 'api-blacklist', summary: '...', score: 0.5, provenance: 'learned' },
      ],
    }])
    expect(screen.getByText(/搜索"stock financials"/)).toBeInTheDocument()
    expect(screen.getByText(/2 命中/)).toBeInTheDocument()
  })

  it('renders load chip with skill name', () => {
    renderWith('s1', [{
      toolCallId: 't2',
      kind: 'load',
      timestamp: '2026-05-12T00:00:00Z',
      name: 'stock-research',
      reason: 'User asked about Apple',
      provenance: 'learned',
    }])
    expect(screen.getByText(/加载"stock-research"/)).toBeInTheDocument()
  })

  it('renders multiple chips for multiple recalls', () => {
    renderWith('s1', [
      { toolCallId: 't1', kind: 'search', timestamp: 'x', query: 'a', results: [] },
      { toolCallId: 't2', kind: 'load', timestamp: 'y', name: 'b', reason: 'r', provenance: 'learned' },
    ])
    expect(screen.getByText(/搜索"a"/)).toBeInTheDocument()
    expect(screen.getByText(/加载"b"/)).toBeInTheDocument()
  })

  it('renders search chip showing 0 命中 when results is empty', () => {
    renderWith('s1', [{
      toolCallId: 't3',
      kind: 'search',
      timestamp: '2026-05-12T00:00:00Z',
      query: 'nothing matches',
      results: [],
    }])
    expect(screen.getByText(/搜索"nothing matches" → 0 命中/)).toBeInTheDocument()
  })
})
