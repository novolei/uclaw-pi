import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { memoryCoreFixture } from '@/lib/dev-memory-core-mock'
import { MemoryRecallTab } from './MemoryRecallTab'

// The recall tuning form loads config through the typed @/lib/tauri-bridge
// helper; the engine card goes through @/lib/bridge/memory-core.
vi.mock('@/lib/tauri-bridge', () => ({
  getMemoryRecallConfig: vi.fn(async () => ({})),
  patchMemoryRecallConfig: vi.fn(async (cfg: Record<string, unknown>) => cfg),
}))
vi.mock('@/lib/bridge/memory-core', async (orig) => ({
  ...(await orig<typeof import('@/lib/bridge/memory-core')>()),
  memoryCoreBridge: {
    getStatus: vi.fn(async () => memoryCoreFixture('ready')),
    setModelsEnabled: vi.fn(),
    retryDownload: vi.fn(),
  },
  onMemoryCoreStatus: vi.fn(async () => () => {}),
}))

describe('MemoryRecallTab', () => {
  it('shows the engine card and keeps recall tuning under a collapsed 高级', async () => {
    const { container, user } = renderWithProviders(<MemoryRecallTab />)
    expect(await screen.findByText('本地检索引擎')).toBeInTheDocument()
    expect(container.querySelector('[data-settings-section="记忆召回配置"]')).toBeInTheDocument()
    const trigger = screen.getByRole('button', { name: /高级/ })
    expect(trigger).toHaveAttribute('aria-expanded', 'false')
    expect(screen.queryByText(/修改后点击「保存」生效/)).toBeNull()
    await user.click(trigger)
    expect(trigger).toHaveAttribute('aria-expanded', 'true')
    expect(await screen.findByText(/修改后点击「保存」生效/)).toBeInTheDocument()
  })
})
