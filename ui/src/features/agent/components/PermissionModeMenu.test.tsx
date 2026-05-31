import { describe, it, expect, vi } from 'vitest'
import * as React from 'react'
import { PermissionModeMenu, MODE_ITEMS } from './PermissionModeMenu'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'

describe('PermissionModeMenu', () => {
  it('renders 5 modes with their number keys when open', async () => {
    const onPick = vi.fn()
    const onOpenChange = vi.fn()
    renderWithProviders(
      <PermissionModeMenu
        current="supervised"
        onPick={onPick}
        open={true}
        onOpenChange={onOpenChange}
        trigger={<button>trigger</button>}
      />
    )
    for (const m of MODE_ITEMS) {
      expect(await screen.findByText(m.label)).toBeInTheDocument()
      expect(screen.getByText(m.numberKey)).toBeInTheDocument()
    }
  })

  it('keyboard 1-5 selects corresponding mode and closes', async () => {
    const onPick = vi.fn()
    const onOpenChange = vi.fn()
    renderWithProviders(
      <PermissionModeMenu
        current="supervised"
        onPick={onPick}
        open={true}
        onOpenChange={onOpenChange}
        trigger={<button>trigger</button>}
      />
    )
    // press '3' → Plan
    window.dispatchEvent(new KeyboardEvent('keydown', { key: '3', bubbles: true }))
    await waitFor(() => expect(onPick).toHaveBeenCalledWith('plan'))
    expect(onOpenChange).toHaveBeenCalledWith(false)
  })

  it('shows checkmark on current mode', async () => {
    renderWithProviders(
      <PermissionModeMenu
        current="plan"
        onPick={() => {}}
        open={true}
        onOpenChange={() => {}}
        trigger={<button>trigger</button>}
      />
    )
    const planRow = (await screen.findByText('Plan mode')).closest('button')!
    expect(planRow.querySelector('svg.lucide-check')).not.toBeNull()
  })
})
