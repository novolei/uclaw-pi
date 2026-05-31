import { describe, it, expect, vi, beforeAll } from 'vitest'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { SettingsBreadcrumb } from './SettingsBreadcrumb'
import * as React from 'react'

// jsdom doesn't ship IntersectionObserver
beforeAll(() => {
  ;(globalThis as unknown as { IntersectionObserver: unknown }).IntersectionObserver = class {
    observe() {}
    disconnect() {}
    unobserve() {}
    takeRecords() {
      return []
    }
    root = null
    rootMargin = ''
    thresholds = []
  } as unknown as typeof IntersectionObserver
})

describe('SettingsBreadcrumb', () => {
  it('renders 「设置 / <tabLabel>」 when no subsection is active', () => {
    const ref = React.createRef<HTMLElement>()
    renderWithProviders(
      <SettingsBreadcrumb tabLabel="智能" scrollContainerRef={ref as React.MutableRefObject<HTMLElement | null>} onClose={() => {}} />,
    )
    expect(screen.getByText('设置')).not.toBeNull()
    expect(screen.getByText('智能')).not.toBeNull()
  })

  it('close button triggers onClose', () => {
    const ref = React.createRef<HTMLElement>()
    const onClose = vi.fn()
    renderWithProviders(
      <SettingsBreadcrumb tabLabel="智能" scrollContainerRef={ref as React.MutableRefObject<HTMLElement | null>} onClose={onClose} />,
    )
    fireEvent.click(screen.getByRole('button', { name: /关闭/ }))
    expect(onClose).toHaveBeenCalledOnce()
  })
})
