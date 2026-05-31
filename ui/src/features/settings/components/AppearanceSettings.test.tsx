import { describe, it, expect } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { AppearanceSettings } from './AppearanceSettings'

// AppearanceSettings drives theme/UI-preference *atoms* (no direct IPC), and its
// atom-helper side effects fire only from event handlers — never on mount — so a
// plain render needs no bridge mock. Assert the section markers render.
describe('AppearanceSettings', () => {
  it('renders the 外观设置 heading + theme + 界面行为 sections', () => {
    const { getByText, getAllByText } = renderWithProviders(<AppearanceSettings />)
    expect(getByText('外观设置')).toBeTruthy()
    expect(getAllByText('主题').length).toBeGreaterThan(0)
    expect(getByText('界面行为')).toBeTruthy()
    // A couple of theme cards are present.
    expect(getByText('浅色')).toBeTruthy()
    expect(getByText('深色')).toBeTruthy()
  })
})
