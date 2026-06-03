import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen, waitFor, fireEvent } from '@/test-utils/render'
import { AgentGrowthTab } from './AgentGrowthTab'

// Mock the bridge so the hook can resolve without a real Tauri invoke().
// Use importOriginal spread to keep any other bridge exports intact.
vi.mock('@/lib/tauri-bridge', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/tauri-bridge')>()),
  listReflections: vi.fn(async () => [
    { id: 'r1', insight: 'You prefer terse answers', confidence: 0.9, createdAt: new Date().toISOString(), archivedAt: null },
  ]),
  getAgentUserModel: vi.fn(async () => ({
    summary: 'An engineer who values concision',
    updatedAt: new Date().toISOString(),
  })),
  listDaydreams: vi.fn(async () => [
    { content: 'A speculative leap', createdAt: new Date().toISOString() },
  ]),
  listUserModelHistory: vi.fn(async () => []),
  listProfileFacts: vi.fn(async () => [
    { id: 'f1', title: 'Loves dark mode', createdAt: new Date().toISOString() },
  ]),
  archiveReflection: vi.fn(async () => undefined),
  restoreReflection: vi.fn(async () => undefined),
  triggerMemoryRefresh: vi.fn(async () => undefined),
  memoryGraphDeleteNode: vi.fn(async () => undefined),
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
    vi.mocked(bridge.listProfileFacts).mockResolvedValue([])

    renderWithProviders(<AgentGrowthTab />)

    await waitFor(() => {
      expect(
        screen.getByText('还没有形成用户模型 —— 多聊几轮就有了'),
      ).toBeInTheDocument()
      expect(screen.getByText('暂无反思')).toBeInTheDocument()
      expect(screen.getByText('agent 还没有遐想')).toBeInTheDocument()
      expect(screen.getByText('暂无事实')).toBeInTheDocument()
    })
  })

  it('renders 立即整合 button in header', async () => {
    const bridge = await import('@/lib/tauri-bridge')
    vi.mocked(bridge.listReflections).mockResolvedValue([
      { id: 'r1', insight: 'You prefer terse answers', confidence: 0.9, createdAt: new Date().toISOString(), archivedAt: null },
    ])
    vi.mocked(bridge.listProfileFacts).mockResolvedValue([
      { id: 'f1', title: 'Loves dark mode', createdAt: new Date().toISOString() },
    ])
    vi.mocked(bridge.getAgentUserModel).mockResolvedValue({
      summary: 'An engineer who values concision',
      updatedAt: new Date().toISOString(),
    })
    vi.mocked(bridge.listDaydreams).mockResolvedValue([])
    vi.mocked(bridge.listUserModelHistory).mockResolvedValue([])

    renderWithProviders(<AgentGrowthTab />)

    await waitFor(() => {
      expect(screen.getByText('立即整合')).toBeInTheDocument()
    })
  })

  it('renders a profile fact row and calls memoryGraphDeleteNode on delete', async () => {
    const bridge = await import('@/lib/tauri-bridge')
    vi.mocked(bridge.listReflections).mockResolvedValue([])
    vi.mocked(bridge.listProfileFacts).mockResolvedValue([
      { id: 'fact-abc', title: 'Prefers bullet points', createdAt: new Date().toISOString() },
    ])
    vi.mocked(bridge.getAgentUserModel).mockResolvedValue(null)
    vi.mocked(bridge.listDaydreams).mockResolvedValue([])
    vi.mocked(bridge.listUserModelHistory).mockResolvedValue([])
    vi.mocked(bridge.memoryGraphDeleteNode).mockResolvedValue(undefined)

    // Mock window.confirm to return true
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true)

    renderWithProviders(<AgentGrowthTab />)

    // Wait for fact to appear
    await waitFor(() => {
      expect(screen.getByText('Prefers bullet points')).toBeInTheDocument()
    })

    // Click the delete (Trash2) button for the fact
    const deleteButtons = screen.getAllByTitle('删除这条事实')
    fireEvent.click(deleteButtons[0])

    expect(confirmSpy).toHaveBeenCalledWith('删除这条事实?')
    await waitFor(() => {
      expect(bridge.memoryGraphDeleteNode).toHaveBeenCalledWith({ nodeId: 'fact-abc' })
    })

    confirmSpy.mockRestore()
  })
})
