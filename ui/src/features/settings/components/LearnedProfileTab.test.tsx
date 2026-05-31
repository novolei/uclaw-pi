/**
 * LearnedProfileTab — Sprint 2.2 visual smoke + interaction tests.
 *
 * Mocks the three Tauri IPC wrappers so the component can render
 * without a backend. Verifies:
 *  - empty state when list returns [].
 *  - grouped rendering of returned facets, one section per class.
 *  - dismiss button flips state to "forgotten" locally.
 *  - rebuild button calls the IPC and re-fetches.
 *  - "learning disabled" structured error surfaces inline.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fireEvent, waitFor } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { LearnedProfileTab } from './LearnedProfileTab'
import * as bridge from '@/lib/tauri-bridge'
import type { FacetDto } from '@/lib/types'

vi.mock('@/lib/tauri-bridge', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-bridge')>(
    '@/lib/tauri-bridge',
  )
  return {
    ...actual,
    memoryLearningListFacets: vi.fn(),
    memoryLearningDismissFacet: vi.fn(),
    memoryLearningRebuildNow: vi.fn(),
    memoryLearningPromoteFacet: vi.fn(),
    memoryLearningDemoteFacet: vi.fn(),
  }
})

vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}))

const listMock = bridge.memoryLearningListFacets as unknown as ReturnType<
  typeof vi.fn
>
const dismissMock = bridge.memoryLearningDismissFacet as unknown as ReturnType<
  typeof vi.fn
>
const rebuildMock = bridge.memoryLearningRebuildNow as unknown as ReturnType<
  typeof vi.fn
>
const promoteMock = bridge.memoryLearningPromoteFacet as unknown as ReturnType<
  typeof vi.fn
>
const demoteMock = bridge.memoryLearningDemoteFacet as unknown as ReturnType<
  typeof vi.fn
>

function fac(overrides: Partial<FacetDto> = {}): FacetDto {
  return {
    facetId: 'fid-1',
    class: 'identity',
    name: 'name',
    value: 'Alice',
    state: 'active',
    stability: 1.8,
    evidenceCount: 3,
    lastSeenAtMs: 1_700_000_000_000,
    ...overrides,
  }
}

describe('LearnedProfileTab', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('shows empty state when list returns no facets', async () => {
    listMock.mockResolvedValueOnce([])
    const { findByText } = renderWithProviders(<LearnedProfileTab />)
    expect(await findByText(/还没有学到任何偏好/)).toBeTruthy()
  })

  it('groups facets by class and renders rows', async () => {
    listMock.mockResolvedValueOnce([
      fac({ facetId: 'a', class: 'identity', name: 'name', value: 'Alice' }),
      fac({
        facetId: 'b',
        class: 'tooling',
        name: 'editor',
        value: 'helix',
        state: 'provisional',
        stability: 0.9,
      }),
    ])
    const { findByText, container } = renderWithProviders(<LearnedProfileTab />)

    expect(await findByText(/身份 \(Identity\)/)).toBeTruthy()
    expect(await findByText('Alice')).toBeTruthy()
    expect(await findByText(/工具 \(Tooling\)/)).toBeTruthy()
    expect(await findByText('helix')).toBeTruthy()

    // The Veto group has no facets — should show "(还没学到)" placeholder.
    const vetoSection = container.querySelector('[data-class-group="veto"]')
    expect(vetoSection?.textContent).toContain('还没学到')
  })

  it('dismiss optimistically flips state to forgotten', async () => {
    listMock.mockResolvedValueOnce([
      fac({ facetId: 'a', class: 'tooling', name: 'editor', value: 'helix' }),
    ])
    dismissMock.mockResolvedValueOnce({
      facet_id: 'a',
      rows_updated: 1,
      new_state: 'forgotten',
    })
    const { findByLabelText, container } = renderWithProviders(
      <LearnedProfileTab />,
    )
    const btn = await findByLabelText('dismiss-a')
    fireEvent.click(btn)

    await waitFor(() => {
      const row = container.querySelector('[data-facet-id="a"]')
      // After dismiss the row stays but the state badge text is "forgotten"
      expect(row?.textContent).toContain('forgotten')
    })
  })

  it('rebuild triggers the IPC and re-fetches the list', async () => {
    listMock.mockResolvedValueOnce([])
    rebuildMock.mockResolvedValueOnce({ promoted_to_active: 1 })
    // Second fetch (after rebuild) — returns one new facet.
    listMock.mockResolvedValueOnce([
      fac({ facetId: 'new', class: 'goal', name: 'project', value: 'memory-os' }),
    ])

    const { findByText, getByText } = renderWithProviders(<LearnedProfileTab />)
    // Wait for initial empty render
    await findByText(/还没有学到任何偏好/)

    fireEvent.click(getByText('立即重建'))

    expect(await findByText('memory-os')).toBeTruthy()
    expect(rebuildMock).toHaveBeenCalledTimes(1)
    expect(listMock).toHaveBeenCalledTimes(2)
  })

  it('promote flips state to active optimistically', async () => {
    listMock.mockResolvedValueOnce([
      fac({
        facetId: 'p1',
        class: 'tooling',
        name: 'editor',
        value: 'helix',
        state: 'provisional',
      }),
    ])
    promoteMock.mockResolvedValueOnce({
      facet_id: 'p1',
      rows_updated: 1,
      new_state: 'active',
    })
    const { findByLabelText, container } = renderWithProviders(
      <LearnedProfileTab />,
    )
    const btn = await findByLabelText('promote-p1')
    fireEvent.click(btn)

    await waitFor(() => {
      const row = container.querySelector('[data-facet-id="p1"]')
      expect(row?.textContent).toContain('active')
    })
    expect(promoteMock).toHaveBeenCalledWith({ facetId: 'p1' })
  })

  it('demote flips state to provisional optimistically', async () => {
    listMock.mockResolvedValueOnce([
      fac({
        facetId: 'd1',
        class: 'identity',
        name: 'role',
        value: 'engineer',
        state: 'active',
      }),
    ])
    demoteMock.mockResolvedValueOnce({
      facet_id: 'd1',
      rows_updated: 1,
      new_state: 'provisional',
    })
    const { findByLabelText, container } = renderWithProviders(
      <LearnedProfileTab />,
    )
    const btn = await findByLabelText('demote-d1')
    fireEvent.click(btn)

    await waitFor(() => {
      const row = container.querySelector('[data-facet-id="d1"]')
      expect(row?.textContent).toContain('provisional')
    })
    expect(demoteMock).toHaveBeenCalledWith({ facetId: 'd1' })
  })

  it('demote button is hidden on candidate / forgotten rows', async () => {
    listMock.mockResolvedValueOnce([
      fac({ facetId: 'c1', class: 'goal', name: 'task', value: 'x', state: 'candidate' }),
      fac({ facetId: 'f1', class: 'goal', name: 'task', value: 'y', state: 'forgotten' }),
    ])
    const { container, queryByLabelText } = renderWithProviders(
      <LearnedProfileTab />,
    )
    // First render: wait for the rows to appear.
    await waitFor(() => {
      expect(container.querySelector('[data-facet-id="c1"]')).toBeTruthy()
    })
    // Candidate: promote shown, demote hidden.
    expect(queryByLabelText('promote-c1')).toBeTruthy()
    expect(queryByLabelText('demote-c1')).toBeNull()
    // Forgotten: promote shown (recovery), demote hidden, dismiss hidden.
    expect(queryByLabelText('promote-f1')).toBeTruthy()
    expect(queryByLabelText('demote-f1')).toBeNull()
    expect(queryByLabelText('dismiss-f1')).toBeNull()
  })

  it('surfaces the disabled error inline', async () => {
    listMock.mockResolvedValueOnce([])
    rebuildMock.mockRejectedValueOnce(
      'Learning pipeline disabled (memory_os.learning_enabled=false). Enable it and restart to use this command.',
    )
    const { findByText, getByText } = renderWithProviders(<LearnedProfileTab />)
    await findByText(/还没有学到任何偏好/)
    fireEvent.click(getByText('立即重建'))

    expect(await findByText(/disabled/i)).toBeTruthy()
  })
})
