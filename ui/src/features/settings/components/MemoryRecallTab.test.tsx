import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { MemoryRecallTab } from './MemoryRecallTab'

// The tab composes the MemoryRecallSettings form, which loads config through the
// typed @/lib/tauri-bridge helper — mock it so the tab mounts quietly.
vi.mock('@/lib/tauri-bridge', () => ({
  getMemoryRecallConfig: vi.fn(async () => ({})),
  patchMemoryRecallConfig: vi.fn(async (cfg: Record<string, unknown>) => cfg),
}))

describe('MemoryRecallTab', () => {
  it('renders the 记忆召回配置 section wrapper', () => {
    const { container } = renderWithProviders(<MemoryRecallTab />)
    expect(
      container.querySelector('[data-settings-section="记忆召回配置"]'),
    ).toBeInTheDocument()
  })
})
