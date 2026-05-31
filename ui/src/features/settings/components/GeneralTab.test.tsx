import { describe, it, expect } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { GeneralTab } from './GeneralTab'

describe('GeneralTab', () => {
  it('renders 2 sub-section markers', () => {
    const { container } = renderWithProviders(<GeneralTab />)
    const markers = container.querySelectorAll('[data-settings-section]')
    expect(markers.length).toBe(3)
    const names = Array.from(markers).map((m) => (m as HTMLElement).dataset.settingsSection)
    expect(names).toContain('通用偏好')
    expect(names).toContain('主题与字体')
  })
})
