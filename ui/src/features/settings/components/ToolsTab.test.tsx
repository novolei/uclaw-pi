import { describe, it, expect } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { ToolsTab } from './ToolsTab'

describe('ToolsTab', () => {
  it('renders 2 sub-section markers (learned skills and MCP have moved to Kaleidoscope)', () => {
    const { container } = renderWithProviders(<ToolsTab />)
    const markers = container.querySelectorAll('[data-settings-section]')
    expect(markers.length).toBe(2)
    const names = Array.from(markers).map((m) => (m as HTMLElement).dataset.settingsSection)
    expect(names).toContain('工具与 MCP')
    expect(names).toContain('工具权限')
  })
})
