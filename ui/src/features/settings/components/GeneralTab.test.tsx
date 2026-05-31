import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { GeneralTab } from './GeneralTab'

// GeneralTab composes the migrated GeneralSettings + PromptSettings +
// AppearanceSettings, which now load through the settings bridge (P3). Mock it so
// their mount-time reads resolve — without this, GeneralSettings' getSettings()
// rejects with an unhandled "__TAURI_INTERNALS__ undefined" error in jsdom.
vi.mock('../../../lib/bridge/settings', () => ({
  settingsBridge: {
    getSettings: vi.fn().mockResolvedValue({ language: 'zh-CN' }),
    patchSettings: vi.fn().mockResolvedValue({ language: 'zh-CN' }),
    getSystemPromptConfig: vi.fn().mockResolvedValue({
      prompts: [],
      appendDateTimeAndUserName: false,
    }),
    setDefaultPrompt: vi.fn(),
    deleteSystemPrompt: vi.fn(),
    updateSystemPrompt: vi.fn(),
    createSystemPrompt: vi.fn(),
    getSystemPromptVersions: vi.fn().mockResolvedValue([]),
  },
}))

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
