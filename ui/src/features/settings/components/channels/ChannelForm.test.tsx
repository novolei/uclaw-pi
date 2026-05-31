import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { ChannelForm } from './ChannelForm'

// IPC stays in the typed @/lib/tauri-bridge provider helpers (model-provider
// domain); stub the two the form uses.
vi.mock('@/lib/tauri-bridge', () => ({
  getProviderConfig: vi.fn().mockResolvedValue(null),
  configureProvider: vi.fn().mockResolvedValue(undefined),
}))

describe('ChannelForm', () => {
  it('renders the configure-provider modal with the provider id in the title', () => {
    renderWithProviders(
      <ChannelForm providerId="openai" onClose={() => {}} onSaved={() => {}} />,
    )
    expect(screen.getByText(/配置 Provider: openai/)).not.toBeNull()
    expect(screen.getByPlaceholderText('sk-...')).not.toBeNull()
  })
})
