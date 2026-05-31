import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import * as React from 'react'
import { PlanModeSuggestBanner } from './PlanModeSuggestBanner'
import {
  pendingPlanModeSuggestsAtom,
  silencedPlanModeSessionsAtom,
} from '@/atoms/plan-mode-suggest-atoms'
import { planModeSuggestEnabledAtom } from '@/atoms/ui-preferences'

// IPC + the plan-mode-suggest event subscription both route through the agent
// bridge now (lib/bridge/agent.ts), so we mock that instead of @tauri-apps/api.
vi.mock('@/lib/bridge/agent', () => ({
  onPlanModeSuggest: vi.fn().mockResolvedValue(() => {}),
  respondPlanModeSuggest: vi.fn().mockResolvedValue(undefined),
  setSafetyMode: vi.fn().mockResolvedValue(undefined),
}))

// Payload uses snake_case to match what the Rust backend emits via serde_json::json!
const FRESH_REQ = {
  id: 'evt-1',
  session_id: 's1',
  source: 'keyword' as const,
  matched_pattern: '计划',
  reason: '建议先 Plan',
  fired_at_ms: 1_000,
}

function renderWithReq() {
  const store = createStore()
  store.set(pendingPlanModeSuggestsAtom, { s1: FRESH_REQ })
  store.set(planModeSuggestEnabledAtom, true)
  return { store, ...render(
    <Provider store={store}>
      <PlanModeSuggestBanner sessionId="s1" />
    </Provider>,
  )}
}

describe('PlanModeSuggestBanner', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('renders when a pending request exists and feature is enabled', () => {
    renderWithReq()
    expect(screen.getByRole('status')).toBeInTheDocument()
    expect(screen.getByText(/建议先 Plan/)).toBeInTheDocument()
    expect(screen.getByText('切到 Plan 模式')).toBeInTheDocument()
  })

  it('renders nothing when feature is disabled', () => {
    const store = createStore()
    store.set(pendingPlanModeSuggestsAtom, { s1: FRESH_REQ })
    store.set(planModeSuggestEnabledAtom, false)
    const { container } = render(
      <Provider store={store}>
        <PlanModeSuggestBanner sessionId="s1" />
      </Provider>,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders nothing for a sessionId with no pending request', () => {
    const store = createStore()
    store.set(pendingPlanModeSuggestsAtom, {})
    store.set(planModeSuggestEnabledAtom, true)
    const { container } = render(
      <Provider store={store}>
        <PlanModeSuggestBanner sessionId="s1" />
      </Provider>,
    )
    expect(container.firstChild).toBeNull()
  })

  it('clicking 本次不用 reports skipped', async () => {
    const bridge = await import('@/lib/bridge/agent')
    renderWithReq()
    fireEvent.click(screen.getByText('本次不用'))
    await waitFor(() => {
      expect(bridge.respondPlanModeSuggest).toHaveBeenCalledWith('evt-1', 'skipped')
    })
  })

  it('clicking 不再建议 flips the enabled atom off + reports silenced', async () => {
    const bridge = await import('@/lib/bridge/agent')
    const store = createStore()
    store.set(pendingPlanModeSuggestsAtom, { s1: FRESH_REQ })
    store.set(planModeSuggestEnabledAtom, true)
    render(
      <Provider store={store}>
        <PlanModeSuggestBanner sessionId="s1" />
      </Provider>,
    )
    fireEvent.click(screen.getByText('不再建议'))
    await waitFor(() => {
      expect(store.get(planModeSuggestEnabledAtom)).toBe(false)
      expect(bridge.respondPlanModeSuggest).toHaveBeenCalledWith('evt-1', 'silenced')
    })
  })

  it('clicking 切到 Plan 模式 calls setSafetyMode(plan) + reports accepted', async () => {
    const bridge = await import('@/lib/bridge/agent')
    renderWithReq()
    fireEvent.click(screen.getByText('切到 Plan 模式'))
    await waitFor(() => {
      expect(bridge.setSafetyMode).toHaveBeenCalledWith({ mode: 'plan' })
      expect(bridge.respondPlanModeSuggest).toHaveBeenCalledWith('evt-1', 'accepted')
    })
  })

  describe('per-session dedupe (silencedPlanModeSessionsAtom)', () => {
    it('adds session to silenced set when 本次不用 is clicked', async () => {
      const store = createStore()
      store.set(pendingPlanModeSuggestsAtom, { s1: FRESH_REQ })
      store.set(planModeSuggestEnabledAtom, true)
      render(
        <Provider store={store}>
          <PlanModeSuggestBanner sessionId="s1" />
        </Provider>,
      )
      fireEvent.click(screen.getByText('本次不用'))
      await waitFor(() => {
        expect(store.get(silencedPlanModeSessionsAtom).has('s1')).toBe(true)
      })
    })

    it('adds session to silenced set when 不再建议 is clicked', async () => {
      const store = createStore()
      store.set(pendingPlanModeSuggestsAtom, { s1: FRESH_REQ })
      store.set(planModeSuggestEnabledAtom, true)
      render(
        <Provider store={store}>
          <PlanModeSuggestBanner sessionId="s1" />
        </Provider>,
      )
      fireEvent.click(screen.getByText('不再建议'))
      await waitFor(() => {
        expect(store.get(silencedPlanModeSessionsAtom).has('s1')).toBe(true)
      })
    })

    it('adds session to silenced set when 切到 Plan 模式 is clicked', async () => {
      const store = createStore()
      store.set(pendingPlanModeSuggestsAtom, { s1: FRESH_REQ })
      store.set(planModeSuggestEnabledAtom, true)
      render(
        <Provider store={store}>
          <PlanModeSuggestBanner sessionId="s1" />
        </Provider>,
      )
      fireEvent.click(screen.getByText('切到 Plan 模式'))
      await waitFor(() => {
        expect(store.get(silencedPlanModeSessionsAtom).has('s1')).toBe(true)
      })
    })

    it('skips future events when session is silenced (listener gate via ref)', async () => {
      // Capture the handler the component registers with onPlanModeSuggest.
      // The bridge wrapper unwraps e.payload, so the handler receives the raw
      // payload directly (not a Tauri event object).
      let listenerHandler: ((payload: unknown) => void) | null = null
      const bridge = await import('@/lib/bridge/agent')
      ;(bridge.onPlanModeSuggest as ReturnType<typeof vi.fn>).mockImplementation(
        (h: (payload: unknown) => void) => {
          listenerHandler = h
          return Promise.resolve(() => {})
        },
      )

      const store = createStore()
      store.set(planModeSuggestEnabledAtom, true)
      // Session already silenced before the event arrives.
      store.set(silencedPlanModeSessionsAtom, new Set(['s1']))
      store.set(pendingPlanModeSuggestsAtom, {})  // start with empty queue

      render(
        <Provider store={store}>
          <PlanModeSuggestBanner sessionId="s1" />
        </Provider>,
      )
      // Wait for the onPlanModeSuggest() promise to resolve and set the handler.
      await waitFor(() => expect(listenerHandler).not.toBeNull())

      // Simulate a backend emit for the silenced session.
      listenerHandler!(FRESH_REQ)
      await Promise.resolve()

      // Queue must remain empty — silenced ref blocked the write.
      expect(store.get(pendingPlanModeSuggestsAtom)['s1']).toBeUndefined()
    })
  })
})
