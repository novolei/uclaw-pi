import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { IntelligenceTab } from './IntelligenceTab'

vi.mock('@/features/settings', () => ({
  ModelSettings: () => <div data-testid="model-settings" />,
  AgentSettings: () => <div data-testid="agent-settings" />,
  PromptsSettings: () => <div data-testid="prompts-settings" />,
}))

vi.mock('@/lib/tauri-bridge', () => ({
  proactiveStatus: vi.fn(async () => ({ status: { status: 'Stopped' } })),
  proactiveStart: vi.fn(async () => undefined),
  proactiveStop: vi.fn(async () => undefined),
}))

describe('IntelligenceTab', () => {
  it('renders without throwing and contains data-settings-section markers', () => {
    const { container } = renderWithProviders(<IntelligenceTab />)
    const markers = container.querySelectorAll('[data-settings-section]')
    expect(markers.length).toBe(4)
    const names = Array.from(markers).map((m) => (m as HTMLElement).dataset.settingsSection)
    expect(names).toContain('模型分配')
    expect(names).toContain('Agent 行为')
    expect(names).toContain('提示词')
    expect(names).toContain('Gene 自进化')
  })
})
