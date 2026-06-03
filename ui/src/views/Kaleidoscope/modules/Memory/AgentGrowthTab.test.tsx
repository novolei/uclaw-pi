import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { AgentGrowthTab } from './AgentGrowthTab'

// Mock the bridge so the hook can resolve without a real Tauri invoke().
// Use importOriginal spread to keep any other bridge exports intact.
vi.mock('@/lib/tauri-bridge', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-bridge')>()),
  listReflections: vi.fn(async () => [
    { insight: 'You prefer terse answers', confidence: 0.9, createdAt: new Date().toISOString() },
  ]),
  getAgentUserModel: vi.fn(async () => ({
    summary: 'An engineer who values concision',
    updatedAt: new Date().toISOString(),
  })),
  listDaydreams: vi.fn(async () => [
    { content: 'A speculative leap', createdAt: new Date().toISOString() },
  ]),
  listUserModelHistory: vi.fn(async () => []),
}))

describe('AgentGrowthTab', () => {
  it('renders data — three section headers, user model summary, and confidence badge', async () => {
    renderWithProviders(<AgentGrowthTab />)

    await waitFor(() => {
      expect(screen.getByText('用户模型')).toBeInTheDocument()
      expect(screen.getByText('反思')).toBeInTheDocument()
      expect(screen.getByText('遐想')).toBeInTheDocument()
      expect(screen.getByText(/An engineer who values concision/)).toBeInTheDocument()
      expect(screen.getByText('90%')).toBeInTheDocument()
    })
  })

  it('renders empty states when the bridge returns no data', async () => {
    const bridge = await import('@/lib/tauri-bridge')
    vi.mocked(bridge.listReflections).mockResolvedValue([])
    vi.mocked(bridge.getAgentUserModel).mockResolvedValue(null)
    vi.mocked(bridge.listDaydreams).mockResolvedValue([])
    vi.mocked(bridge.listUserModelHistory).mockResolvedValue([])

    renderWithProviders(<AgentGrowthTab />)

    await waitFor(() => {
      expect(
        screen.getByText('还没有形成用户模型 —— 多聊几轮就有了'),
      ).toBeInTheDocument()
      expect(screen.getByText('暂无反思')).toBeInTheDocument()
      expect(screen.getByText('agent 还没有遐想')).toBeInTheDocument()
    })
  })
})
