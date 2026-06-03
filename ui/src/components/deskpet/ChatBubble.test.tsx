import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { petPrimaryStateAtom } from '@/atoms/pet-atoms'

// The pet bridge talks to Tauri (invoke + listen). Mock the whole module so the
// test drives the streaming events directly via the captured callbacks.
type Cb<T> = (payload: T) => void
const deltaCbs: Array<Cb<{ text: string }>> = []
const doneCbs: Array<Cb<Record<string, never>>> = []
const errorCbs: Array<Cb<{ message: string }>> = []

const petChat = vi.fn((_message: string, _personaId?: string) => Promise.resolve())

vi.mock('@/lib/bridge/pet', () => ({
  petChat: (message: string, personaId?: string) => petChat(message, personaId),
  onPetReplyDelta: (cb: Cb<{ text: string }>) => {
    deltaCbs.push(cb)
    return Promise.resolve(() => {})
  },
  onPetReplyDone: (cb: Cb<Record<string, never>>) => {
    doneCbs.push(cb)
    return Promise.resolve(() => {})
  },
  onPetReplyError: (cb: Cb<{ message: string }>) => {
    errorCbs.push(cb)
    return Promise.resolve(() => {})
  },
}))

import { ChatBubble } from './ChatBubble'

beforeEach(() => {
  deltaCbs.length = 0
  doneCbs.length = 0
  errorCbs.length = 0
  petChat.mockClear()
})

function emitDelta(text: string) {
  for (const cb of deltaCbs) cb({ text })
}
function emitDone() {
  for (const cb of doneCbs) cb({})
}
function emitError(message: string) {
  for (const cb of errorCbs) cb({ message })
}

describe('ChatBubble', () => {
  it('sends via petChat with the active persona and streams a mocked reply', async () => {
    const store = createStore()
    store.set(petPrimaryStateAtom, 'idle')
    const { user } = renderWithProviders(<ChatBubble />, { store })

    // Wait for the event listeners to bind.
    await waitFor(() => expect(deltaCbs.length).toBeGreaterThan(0))

    await user.type(screen.getByLabelText('消息'), '你好')
    await user.click(screen.getByRole('button', { name: '发送' }))

    expect(petChat).toHaveBeenCalledWith('你好', 'astro')
    expect(store.get(petPrimaryStateAtom)).toBe('thinking')

    // Stream two deltas → balloon accumulates, pet enters typing.
    emitDelta('世界')
    emitDelta('！')
    await waitFor(() => expect(screen.getByRole('status').textContent).toBe('世界！'))
    expect(store.get(petPrimaryStateAtom)).toBe('typing')

    // Done → pet returns to idle.
    emitDone()
    await waitFor(() => expect(store.get(petPrimaryStateAtom)).toBe('idle'))
  })

  it('shows the not-ready message and error state on reply-error', async () => {
    const store = createStore()
    const { user } = renderWithProviders(<ChatBubble />, { store })
    await waitFor(() => expect(errorCbs.length).toBeGreaterThan(0))

    await user.type(screen.getByLabelText('消息'), 'hi')
    await user.click(screen.getByRole('button', { name: '发送' }))

    emitError('本地模型未就绪')
    await waitFor(() => expect(screen.getByRole('status').textContent).toContain('本地模型未就绪'))
    expect(store.get(petPrimaryStateAtom)).toBe('error')
  })

  it('does not send empty input', async () => {
    const { user } = renderWithProviders(<ChatBubble />)
    await waitFor(() => expect(deltaCbs.length).toBeGreaterThan(0))
    // Button is disabled with empty input; force-clicking does nothing.
    await user.click(screen.getByRole('button', { name: '发送' }))
    expect(petChat).not.toHaveBeenCalled()
  })
})
