import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { EmbeddingEndpointSection } from './EmbeddingEndpointSection'

// The section loads its config on mount via @/lib/embedding-endpoint (a gbrain/
// memU domain helper — not settings-domain IPC, so it stays there rather than
// routing through settingsBridge). Mock it so the fields populate from the loaded
// config.
vi.mock('@/lib/embedding-endpoint', () => ({
  getEmbeddingConfig: vi.fn().mockResolvedValue({
    base_url: 'http://localhost:9999/v1',
    model: 'llama-server:test-model',
    dimensions: 512,
    fastembed_model: 'BAAI/test',
  }),
  setEmbeddingConfig: vi.fn(async (c) => c),
  testEmbeddingEndpoint: vi.fn().mockResolvedValue(undefined),
}))

describe('EmbeddingEndpointSection', () => {
  it('renders the Embedding 端点配置 card + loads the config from the helper', async () => {
    renderWithProviders(<EmbeddingEndpointSection />)
    expect(screen.getByText('Embedding 端点配置')).toBeTruthy()
    // After the mount load resolves, the loaded base_url populates the field.
    await waitFor(() =>
      expect(screen.getByDisplayValue('http://localhost:9999/v1')).toBeTruthy(),
    )
  })
})
