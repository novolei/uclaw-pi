import { describe, it, expect } from 'vitest'
import { createStore } from 'jotai'
import { renderWithProviders, screen } from '@/test-utils/render'
import { PetSettings } from './PetSettings'
import { petEnabledAtom } from '@/atoms/pet-atoms'

// Pure atom-driven UI (no IPC, no effects). The character radiogroup only appears
// once the pet is enabled — so we cover both the disabled default and the enabled
// state via a pre-seeded store.
describe('PetSettings', () => {
  it('renders the 桌面宠物 toggle and hides the character picker by default', () => {
    renderWithProviders(<PetSettings />)
    expect(screen.getByText('桌面宠物')).not.toBeNull()
    expect(screen.getByText('启用桌面宠物')).not.toBeNull()
    expect(screen.queryByRole('radiogroup', { name: '选择宠物角色' })).toBeNull()
  })

  it('shows the character radiogroup when the pet is enabled', () => {
    const store = createStore()
    store.set(petEnabledAtom, true)
    renderWithProviders(<PetSettings />, { store })
    expect(screen.getByRole('radiogroup', { name: '选择宠物角色' })).not.toBeNull()
    expect(screen.getByText('小宇 Astro')).not.toBeNull()
    expect(screen.getByText('爪宝 Clawby')).not.toBeNull()
  })
})
