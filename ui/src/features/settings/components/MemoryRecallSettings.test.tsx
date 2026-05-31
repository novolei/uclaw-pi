import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { MemoryRecallSettings } from './MemoryRecallSettings'

// Config load/save flows through useMemoryRecallSettings → the typed
// @/lib/tauri-bridge get/patchMemoryRecallConfig helpers. Mock the bridge so the
// form renders without invoke().
vi.mock('@/lib/tauri-bridge', () => ({
  getMemoryRecallConfig: vi.fn(async () => ({ tokenBudget: 4242 })),
  patchMemoryRecallConfig: vi.fn(async (cfg: Record<string, unknown>) => cfg),
}))

describe('MemoryRecallSettings', () => {
  it('renders the section cards once the config loads', async () => {
    renderWithProviders(<MemoryRecallSettings />)
    await waitFor(() => expect(screen.getByText('Token 预算')).toBeInTheDocument())
    expect(screen.getByText('召回数量限制')).toBeInTheDocument()
    expect(screen.getByText('融合策略')).toBeInTheDocument()
    expect(screen.getByText('技能挂载')).toBeInTheDocument()
    expect(screen.getByText('高级设置')).toBeInTheDocument()
  })

  it('saves the config through the bridge when 保存 is clicked', async () => {
    const { user } = renderWithProviders(<MemoryRecallSettings />)
    await waitFor(() => expect(screen.getByText('Token 预算')).toBeInTheDocument())
    // Make the form dirty by editing the token_budget input, then save.
    const input = screen.getByDisplayValue('4242')
    await user.clear(input)
    await user.type(input, '5000')
    await user.click(screen.getByRole('button', { name: /保存/ }))
    const { patchMemoryRecallConfig } = await import('@/lib/tauri-bridge')
    await waitFor(() => expect(patchMemoryRecallConfig).toHaveBeenCalled())
  })
})
