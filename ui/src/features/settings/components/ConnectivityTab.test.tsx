import { describe, it, expect } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { ConnectivityTab } from './ConnectivityTab'

describe('ConnectivityTab', () => {
  it('renders without throwing and contains data-settings-section markers', () => {
    const { container } = renderWithProviders(<ConnectivityTab />)
    const markers = container.querySelectorAll('[data-settings-section]')
    expect(markers.length).toBe(2)
    const names = Array.from(markers).map((m) => (m as HTMLElement).dataset.settingsSection)
    expect(names).toContain('服务商')
    expect(names).toContain('用量与预算')
  })
})
