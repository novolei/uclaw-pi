import { describe, it, expect, vi } from 'vitest'
import { createStore } from 'jotai'
import { renderWithProviders } from '@/test-utils/render'
import { BrowserPreviewOverlay } from './BrowserPreviewOverlay'
import { sessionBrowserPreviewMapAtom, type BrowserPreviewState } from '@/atoms/agent-atoms'

// The overlay subscribes to a live screencast (real CDP IPC) and opens URLs in
// the OS browser. Stub both so the render test never reaches `@tauri-apps/api`.
vi.mock('@/hooks/useBrowserScreencast', () => ({
  useBrowserScreencast: vi.fn(),
}))
vi.mock('@/lib/bridge/agent', () => ({
  openExternal: vi.fn().mockResolvedValue(undefined),
}))

function visiblePreview(over: Partial<BrowserPreviewState> = {}): BrowserPreviewState {
  return {
    url: 'https://example.com/path',
    tabId: 'tab-1',
    screenshotData: null,
    visible: true,
    minimized: false,
    ...over,
  } as BrowserPreviewState
}

describe('BrowserPreviewOverlay', () => {
  it('renders nothing when there is no visible preview for the session', () => {
    const { container } = renderWithProviders(<BrowserPreviewOverlay sessionId="s1" />)
    expect(container.firstChild).toBeNull()
  })

  it('renders the URL bar (hostname) when a preview is visible', () => {
    const store = createStore()
    store.set(sessionBrowserPreviewMapAtom, new Map([['s1', visiblePreview()]]))
    const { getByText } = renderWithProviders(<BrowserPreviewOverlay sessionId="s1" />, { store })
    expect(getByText('example.com')).toBeTruthy()
    // No frame yet → waiting placeholder.
    expect(getByText(/等待画面/)).toBeTruthy()
  })

  it('hides the screencast area when minimized', () => {
    const store = createStore()
    store.set(sessionBrowserPreviewMapAtom, new Map([['s1', visiblePreview({ minimized: true })]]))
    const { getByText, queryByText } = renderWithProviders(
      <BrowserPreviewOverlay sessionId="s1" />,
      { store },
    )
    expect(getByText('example.com')).toBeTruthy()
    expect(queryByText(/等待画面/)).toBeNull()
  })
})
