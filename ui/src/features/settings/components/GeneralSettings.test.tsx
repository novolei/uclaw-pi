import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { GeneralSettings } from './GeneralSettings'

// Interface-language load/persist flows through the settings bridge (P3); mock
// it so the mount-time getSettings() resolves (no unhandled rejection).
vi.mock('../../../lib/bridge/settings', () => ({
  settingsBridge: {
    getSettings: vi.fn().mockResolvedValue({ language: 'zh-CN' }),
    patchSettings: vi.fn().mockResolvedValue({ language: 'zh-CN' }),
  },
}))

describe('GeneralSettings', () => {
  it('renders the 语言与地区 / 消息 / 外观 sections', () => {
    const { getByText } = renderWithProviders(<GeneralSettings />)
    expect(getByText('语言与地区')).toBeTruthy()
    expect(getByText('消息')).toBeTruthy()
    expect(getByText('外观')).toBeTruthy()
    expect(getByText('底部 Dock 导航栏')).toBeTruthy()
  })
})
