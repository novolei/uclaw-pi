import { describe, it, expect, beforeEach } from 'vitest'
import * as React from 'react'
import { ModeBanner } from './ModeBanner'
import { renderWithProviders, screen } from '@/test-utils/render'
import { safetyModeAtom } from '@/atoms/safety-atoms'

describe('ModeBanner', () => {
  beforeEach(() => { document.body.innerHTML = '' })

  it('renders nothing in Auto mode', () => {
    const { store, container } = renderWithProviders(<ModeBanner />)
    store.set(safetyModeAtom, 'supervised')
    expect(container.textContent).toBe('')
  })

  it('renders nothing in Ask mode', () => {
    const { store, container } = renderWithProviders(<ModeBanner />)
    store.set(safetyModeAtom, 'ask')
    expect(container.textContent).toBe('')
  })

  it('renders nothing in Bypass mode', () => {
    const { store, container } = renderWithProviders(<ModeBanner />)
    store.set(safetyModeAtom, 'yolo')
    expect(container.textContent).toBe('')
  })

  it('renders the Plan mode banner in plan', async () => {
    const { store } = renderWithProviders(<ModeBanner />)
    store.set(safetyModeAtom, 'plan')
    expect(await screen.findByText(/Plan mode — investigating only/i)).toBeInTheDocument()
  })

  it('renders the Accept edits banner in acceptedits', async () => {
    const { store } = renderWithProviders(<ModeBanner />)
    store.set(safetyModeAtom, 'acceptedits')
    expect(await screen.findByText(/Accept edits — file edits auto-pass/i)).toBeInTheDocument()
  })
})
