import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createStore, Provider } from 'jotai'
import { describe, expect, it } from 'vitest'
import {
  petCharacterAtom,
  petEnabledAtom,
  petHoverActiveAtom,
  petPrimaryStateAtom,
} from '@/atoms/pet-atoms'
import { PetWidget } from './PetWidget'

function renderWith(setup: (store: ReturnType<typeof createStore>) => void = () => {}) {
  const store = createStore()
  setup(store)
  return {
    store,
    ...render(
      <Provider store={store}>
        <PetWidget data-testid="pet" />
      </Provider>,
    ),
  }
}

describe('PetWidget', () => {
  it('renders nothing when pet is disabled', () => {
    const { container } = renderWith()
    expect(container.firstChild).toBeNull()
  })

  it('renders idle img when enabled', () => {
    renderWith((s) => {
      s.set(petEnabledAtom, true)
    })
    const img = screen.getByRole('img', { hidden: true })
    expect(img.getAttribute('src')).toContain('/pet/astro-idle.webp')
  })

  it('switches character path when petCharacterAtom changes', async () => {
    const { store } = renderWith((s) => {
      s.set(petEnabledAtom, true)
    })
    act(() => { store.set(petCharacterAtom, 'clawby') })
    await waitFor(() => {
      const img = screen.getAllByRole('img', { hidden: true })[0]
      expect(img.getAttribute('src')).toContain('/pet/clawby-')
    })
  })

  it('hover triggers hover state when primary is idle', async () => {
    const user = userEvent.setup()
    const { store } = renderWith((s) => {
      s.set(petEnabledAtom, true)
    })
    const widget = document.querySelector('.pet-widget') as HTMLElement
    await user.hover(widget)
    expect(store.get(petHoverActiveAtom)).toBe(true)
    await user.unhover(widget)
    expect(store.get(petHoverActiveAtom)).toBe(false)
  })

  it('renders thinking state img when primary is thinking', () => {
    renderWith((s) => {
      s.set(petEnabledAtom, true)
      s.set(petPrimaryStateAtom, 'thinking')
    })
    const imgs = screen.getAllByRole('img', { hidden: true })
    // After crossfade swap, an img with src=...-thinking.webp must exist
    const hasThinking = imgs.some((i) => i.getAttribute('src')?.includes('-thinking.webp'))
    expect(hasThinking).toBe(true)
  })
})
