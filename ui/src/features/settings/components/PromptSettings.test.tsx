import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { PromptSettings } from './PromptSettings'

// IPC flows through the settings bridge (P3); mock it with a minimal config so
// the component leaves its loading state and renders the list.
vi.mock('../../../lib/bridge/settings', () => ({
  settingsBridge: {
    getSystemPromptConfig: vi.fn().mockResolvedValue({
      prompts: [
        { id: 'builtin-default', name: '默认', content: 'hi', isBuiltin: true },
        { id: 'custom-1', name: '我的提示词', content: 'custom content' },
      ],
      defaultPromptId: 'builtin-default',
      appendDateTimeAndUserName: false,
    }),
    createSystemPrompt: vi.fn(),
    updateSystemPrompt: vi.fn(),
    deleteSystemPrompt: vi.fn(),
    setDefaultPrompt: vi.fn(),
    getSystemPromptVersions: vi.fn().mockResolvedValue([]),
  },
}))

describe('PromptSettings', () => {
  it('renders the 系统提示词 header + prompt rows once loaded', async () => {
    const { findByText, getByRole } = renderWithProviders(<PromptSettings />)
    expect(await findByText('我的提示词')).toBeTruthy()
    expect(getByRole('heading', { name: '系统提示词' })).toBeTruthy()
    expect(await findByText('默认')).toBeTruthy()
  })
})
