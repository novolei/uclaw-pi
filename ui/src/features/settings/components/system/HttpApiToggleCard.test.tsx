import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { HttpApiToggleCard } from './HttpApiToggleCard'

vi.mock('../../../../lib/bridge/settings', () => ({
  settingsBridge: {
    getHttpApiEnabled: vi.fn().mockResolvedValue(false),
    setHttpApiEnabled: vi.fn().mockResolvedValue(undefined),
  },
}))

describe('HttpApiToggleCard', () => {
  it('renders the 本地 HTTP API 服务 card', async () => {
    const { findByText } = renderWithProviders(<HttpApiToggleCard />)
    expect(await findByText(/本地 HTTP API 服务/)).toBeTruthy()
  })
})
