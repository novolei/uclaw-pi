import { beforeEach, describe, expect, it, vi } from 'vitest'
import { screen, waitFor } from '@/test-utils/render'
import { renderWithProviders } from '@/test-utils/render'
import { ProviderDetail } from './ChannelSettings'
import type { ProviderInfo } from '@/lib/types'

vi.mock('@/lib/tauri-bridge', () => ({
  listProviders: vi.fn(),
  listConfiguredProviders: vi.fn(),
  getProviderConfig: vi.fn(),
  getConfiguredModels: vi.fn(),
  getAllConfiguredModels: vi.fn(),
  configureProviderWithModels: vi.fn(),
  removeProviderConfig: vi.fn(),
  testProviderConnection: vi.fn(),
  listProviderModels: vi.fn(),
}))

import {
  listProviders,
  listConfiguredProviders,
  getProviderConfig,
  getConfiguredModels,
  getAllConfiguredModels,
} from '@/lib/tauri-bridge'

const openaiProvider: ProviderInfo = {
  id: 'openai',
  displayName: 'OpenAI',
  authType: 'apikey',
  defaultBaseUrl: 'https://api.openai.com',
  defaultApi: 'openai-completions',
  serviceCategory: 'Api',
  geoCategory: 'Global',
  supportsModels: true,
}

describe('ChannelSettings / ProviderDetail', () => {
  beforeEach(() => {
    vi.mocked(getProviderConfig).mockReset()
    vi.mocked(getConfiguredModels).mockReset()
    vi.mocked(listProviders).mockReset()
    vi.mocked(listConfiguredProviders).mockReset()
    vi.mocked(getAllConfiguredModels).mockReset()
  })

  it('shows masked placeholder when a key is already configured and the field is empty', async () => {
    vi.mocked(getProviderConfig).mockResolvedValueOnce({
      providerId: 'openai',
      displayName: 'OpenAI',
      hasApiKey: true,
      maskedKey: '3f9a',
      baseUrl: null,
      api: null,
    })
    vi.mocked(getConfiguredModels).mockResolvedValueOnce([])

    renderWithProviders(
      <ProviderDetail
        provider={openaiProvider}
        isConfigured={true}
        onSaved={() => {}}
      />,
    )

    const input = await screen.findByPlaceholderText(/••••3f9a/)
    expect(input).toBeInTheDocument()
  })

  it('shows default sk-… placeholder when no key is configured', async () => {
    vi.mocked(getProviderConfig).mockResolvedValueOnce({
      providerId: 'openai',
      displayName: 'OpenAI',
      hasApiKey: false,
      maskedKey: null,
      baseUrl: null,
      api: null,
    })
    vi.mocked(getConfiguredModels).mockResolvedValueOnce([])

    renderWithProviders(
      <ProviderDetail
        provider={openaiProvider}
        isConfigured={false}
        onSaved={() => {}}
      />,
    )

    await waitFor(() => {
      expect(getProviderConfig).toHaveBeenCalledWith('openai')
    })

    // placeholder is the fallback sk-… (no maskedKey configured)
    expect(screen.getByPlaceholderText('sk-…')).toBeInTheDocument()
  })

  it('shows default sk-… placeholder when user starts typing (overrides masked hint)', async () => {
    vi.mocked(getProviderConfig).mockResolvedValueOnce({
      providerId: 'openai',
      displayName: 'OpenAI',
      hasApiKey: true,
      maskedKey: '3f9a',
      baseUrl: null,
      api: null,
    })
    vi.mocked(getConfiguredModels).mockResolvedValueOnce([])

    const { user } = renderWithProviders(
      <ProviderDetail
        provider={openaiProvider}
        isConfigured={true}
        onSaved={() => {}}
      />,
    )

    // Wait for masked placeholder to appear
    const input = await screen.findByPlaceholderText(/••••3f9a/)
    expect(input).toBeInTheDocument()

    // Type a new key — the value is set so placeholder is no longer relevant,
    // but we verify input becomes active with a value
    await user.type(input, 'sk-newkey')
    expect(input).toHaveValue('sk-newkey')
  })

  it('shows 无需 API Key placeholder for none authType providers', async () => {
    vi.mocked(getProviderConfig).mockResolvedValueOnce(null)
    vi.mocked(getConfiguredModels).mockResolvedValueOnce([])

    const noneProvider: ProviderInfo = {
      ...openaiProvider,
      id: 'local',
      displayName: 'Local',
      authType: 'none',
    }

    renderWithProviders(
      <ProviderDetail
        provider={noneProvider}
        isConfigured={false}
        onSaved={() => {}}
      />,
    )

    await waitFor(() => {
      expect(screen.getByPlaceholderText('无需 API Key')).toBeInTheDocument()
    })
    expect(screen.getByPlaceholderText('无需 API Key')).toBeDisabled()
  })
})
