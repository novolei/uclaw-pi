import { describe, expect, it } from 'vitest'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Provider, createStore } from 'jotai'
import { MemoryRecallChip, countRecalledMemories, hasRecalledMemories } from './MemoryRecallChip'
import type { MemoryRecallEvent } from '@/atoms/agent-atoms'
import { settingsOpenAtom, settingsTabAtom } from '@/atoms/settings-tab'

function makeEvent(over: Partial<MemoryRecallEvent> = {}): MemoryRecallEvent {
  return {
    totalCandidates: 0,
    skillsCount: 0,
    bootCount: 0,
    triggeredCount: 0,
    relevantCount: 0,
    expandedCount: 0,
    recentCount: 0,
    items: [],
    coreHits: [],
    mode: 'hybrid',
    conversationId: 's1',
    timestamp: '2026-11-09T00:00:00Z',
    ...over,
  }
}

const CORE_HITS: NonNullable<MemoryRecallEvent['coreHits']> = [
  { title: '家庭', snippet: '用户的儿子叫刘书维', source: 'graph', score: 0.82, scoreKind: 'rerank' },
  { title: '上周讨论的旅行计划', snippet: '打算十二月去京都，住在岚山附近', source: 'chunk', score: 0.031, scoreKind: 'rrf' },
  { title: '回答风格', snippet: '偏好简洁回答', source: 'item', score: 0.4, scoreKind: 'rerank' },
]

function renderChip(event: MemoryRecallEvent) {
  const store = createStore()
  const utils = render(
    <Provider store={store}>
      <MemoryRecallChip event={event} inline />
    </Provider>,
  )
  return { store, ...utils }
}

describe('MemoryRecallChip', () => {
  it('renders nothing when there are no core hits and no graph items', () => {
    const { container } = renderChip(makeEvent())
    expect(container).toBeEmptyDOMElement()
    expect(hasRecalledMemories(makeEvent())).toBe(false)
  })

  it('shows a chip for core-only turns (graph empty)', () => {
    renderChip(makeEvent({ coreHits: CORE_HITS }))
    expect(screen.getByRole('button', { name: /参考了 3 条记忆/ })).toBeInTheDocument()
  })

  it('counts graph items once when a core graph hit duplicates them by title', () => {
    const ev = makeEvent({
      coreHits: CORE_HITS,
      totalCandidates: 2,
      relevantCount: 2,
      items: [
        { nodeId: 'a', title: '家庭', kind: 'knowledge', source: 'graph' },
        { nodeId: 'b', title: '常用编辑器', kind: 'knowledge', source: 'graph' },
      ],
    })
    expect(countRecalledMemories(ev)).toBe(4)
    renderChip(ev)
    expect(screen.getByRole('button', { name: /参考了 4 条记忆/ })).toBeInTheDocument()
  })

  it('still works for legacy payloads without coreHits/mode', () => {
    const legacy = makeEvent({
      totalCandidates: 1,
      relevantCount: 1,
      items: [{ nodeId: 'a', title: 'x', kind: 'knowledge', source: 'graph' }],
    })
    delete (legacy as Partial<MemoryRecallEvent>).coreHits
    delete (legacy as Partial<MemoryRecallEvent>).mode
    renderChip(legacy)
    expect(screen.getByRole('button', { name: /参考了 1 条记忆/ })).toBeInTheDocument()
  })

  it('popover lists core hits with source labels and a relevance bar only for rerank', async () => {
    const user = userEvent.setup()
    renderChip(makeEvent({ coreHits: CORE_HITS }))
    await user.click(screen.getByRole('button', { name: /参考了 3 条记忆/ }))

    const list = await screen.findByRole('list', { name: '最相关' })
    const rows = within(list).getAllByRole('listitem')
    expect(rows).toHaveLength(3)

    expect(within(rows[0]).getByText('家庭')).toBeInTheDocument()
    expect(within(rows[0]).getByText('用户的儿子叫刘书维')).toBeInTheDocument()
    expect(within(rows[0]).getByText('记忆')).toBeInTheDocument()
    expect(within(rows[0]).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '82')

    expect(within(rows[1]).getByText('对话片段')).toBeInTheDocument()
    expect(within(rows[1]).queryByRole('progressbar')).toBeNull()

    expect(within(rows[2]).getByText('记忆')).toBeInTheDocument()
    expect(within(rows[2]).getByRole('progressbar')).toBeInTheDocument()

    // list items are not interactive yet (Memory Center deep links come later)
    expect(within(list).queryByRole('link')).toBeNull()
    expect(within(list).queryByRole('button')).toBeNull()
  })

  it('opens from the keyboard and closes on Escape', async () => {
    const user = userEvent.setup()
    renderChip(makeEvent({ coreHits: CORE_HITS }))
    const chip = screen.getByRole('button', { name: /参考了 3 条记忆/ })
    chip.focus()
    await user.keyboard('{Enter}')
    expect(await screen.findByRole('list', { name: '最相关' })).toBeInTheDocument()
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('list', { name: '最相关' })).toBeNull()
  })

  it('shows the keyword-mode hint that opens Settings › 记忆', async () => {
    const user = userEvent.setup()
    const { store } = renderChip(makeEvent({ coreHits: CORE_HITS, mode: 'keyword' }))
    await user.click(screen.getByRole('button', { name: /参考了 3 条记忆/ }))
    expect(await screen.findByText(/本轮按关键词检索/)).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: '打开记忆设置' }))
    expect(store.get(settingsOpenAtom)).toBe(true)
    expect(store.get(settingsTabAtom)).toBe('memoryRecall')
  })

  it('hides the keyword hint in hybrid mode', async () => {
    const user = userEvent.setup()
    renderChip(makeEvent({ coreHits: CORE_HITS, mode: 'hybrid' }))
    await user.click(screen.getByRole('button', { name: /参考了 3 条记忆/ }))
    await screen.findByRole('list', { name: '最相关' })
    expect(screen.queryByText(/本轮按关键词检索/)).toBeNull()
  })

  it('keeps the graph layer breakdown below the core hits', async () => {
    const user = userEvent.setup()
    renderChip(
      makeEvent({
        coreHits: CORE_HITS.slice(0, 1),
        totalCandidates: 2,
        bootCount: 1,
        relevantCount: 1,
        items: [
          { nodeId: 'a', title: '启动记忆', kind: 'boot', source: 'graph' },
          { nodeId: 'b', title: '常用编辑器', kind: 'knowledge', source: 'graph' },
        ],
      }),
    )
    await user.click(screen.getByRole('button', { name: /参考了 3 条记忆/ }))
    expect(await screen.findByText('常用编辑器')).toBeInTheDocument()
    expect(screen.getByText('层级分布')).toBeInTheDocument()
  })

  it('names the popover by its heading', async () => {
    const user = userEvent.setup()
    renderChip(makeEvent({ coreHits: CORE_HITS }))
    await user.click(screen.getByRole('button', { name: /参考了 3 条记忆/ }))
    expect(await screen.findByRole('dialog', { name: '本轮参考的记忆' })).toBeInTheDocument()
  })

  it('lists a graph item a core hit already shows only once, and the layers agree with the total', async () => {
    const user = userEvent.setup()
    renderChip(
      makeEvent({
        coreHits: CORE_HITS,
        totalCandidates: 2,
        bootCount: 1,
        relevantCount: 1,
        items: [
          { nodeId: 'a', title: '家庭', kind: 'knowledge', source: 'graph' },
          { nodeId: 'b', title: '常用编辑器', kind: 'knowledge', source: 'graph' },
        ],
      }),
    )
    await user.click(screen.getByRole('button', { name: /参考了 4 条记忆/ }))
    expect(await screen.findByText('共 4 条')).toBeInTheDocument()
    // 家庭 appears once (under 最相关), not again under 层级分布.
    expect(screen.getAllByText('家庭')).toHaveLength(1)
    expect(screen.getByText('常用编辑器')).toBeInTheDocument()
    // 常驻 (家庭's layer) drops to 0 and is hidden; 相关 1 remains: 3 + 1 = 4.
    expect(screen.queryByText('常驻')).toBeNull()
    expect(screen.getByText('相关')).toBeInTheDocument()
  })

  it.each(['mouse', 'keyboard'] as const)(
    'opening by %s focuses the panel, not the keyword hint, and the hint ring is focus-visible only',
    async (how) => {
      const user = userEvent.setup()
      renderChip(makeEvent({ coreHits: CORE_HITS, mode: 'keyword' }))
      const chip = screen.getByRole('button', { name: /参考了 3 条记忆/ })
      if (how === 'mouse') {
        await user.click(chip)
      } else {
        chip.focus()
        await user.keyboard('{Enter}')
      }
      const panel = await screen.findByRole('dialog', { name: '本轮参考的记忆' })
      const hint = screen.getByRole('button', { name: '打开记忆设置' })
      expect(panel).toHaveFocus()
      expect(hint).not.toHaveFocus()
      // Ring classes only behind :focus-visible — never plain :focus.
      expect(hint.className).toMatch(/focus-visible:ring-2/)
      expect(hint.className).not.toMatch(/(^|\s)focus:/)
      // Tab still reaches it from the panel (non-modal popover, no trap).
      await user.tab()
      expect(hint).toHaveFocus()
    },
  )
})
