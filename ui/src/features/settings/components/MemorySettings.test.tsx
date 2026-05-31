import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { MemorySettings } from './MemorySettings'

// The boot-node load flows through the typed @/lib/tauri-bridge memory-graph
// helper (via useMemorySettings) — mock it so the component renders the rows.
vi.mock('@/lib/tauri-bridge', () => ({
  memoryGraphListBoot: vi.fn(async () => [{ id: 'n1', title: '引导节点甲' }]),
}))

describe('MemorySettings', () => {
  it('renders the 记忆设置 sections and the loaded boot node', async () => {
    renderWithProviders(<MemorySettings />)
    expect(screen.getByText('记忆设置')).toBeInTheDocument()
    expect(screen.getByText('自动记忆')).toBeInTheDocument()
    expect(screen.getByText('引导节点')).toBeInTheDocument()
    await waitFor(() => expect(screen.getByText('引导节点甲')).toBeInTheDocument())
  })
})
