import { describe, it, expect, vi } from 'vitest'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { ModelSettings } from './ModelSettings'

// ModelSettings loads configured models + role assignments on mount through
// `settingsBridge`. Mock the bridge so the role cards render with the mocked
// assignment (chat → openai/gpt-4o) and a non-empty 已配置 count.
vi.mock('../../../lib/bridge/settings', () => ({
  settingsBridge: {
    getAllConfiguredModels: vi.fn().mockResolvedValue([['openai', ['gpt-4o', 'gpt-4o-mini']]]),
    getRoleModels: vi.fn().mockResolvedValue([{ role: 'chat', model_ref: 'openai/gpt-4o' }]),
    setRoleModel: vi.fn().mockResolvedValue(undefined),
  },
}))

describe('ModelSettings', () => {
  it('renders the role cards + a configured count from the bridge', async () => {
    renderWithProviders(<ModelSettings />)
    // Role cards map over `roleConfigs`, which is empty until the mount load
    // resolves — so the labels, the assigned model, and the 已配置 count all
    // appear together after the bridge call settles.
    await waitFor(() => {
      expect(screen.getByText('主对话模型')).toBeTruthy()
      expect(screen.getByText('轻工具模型')).toBeTruthy()
      expect(screen.getByText('gpt-4o')).toBeTruthy()
      expect(screen.getByText('1/5 已配置')).toBeTruthy()
    })
  })

  it('always offers the built-in local model as an assignable option (even when not in configured models)', async () => {
    // The mocked bridge returns ONLY openai configured models — no local-minicpm.
    // The local model must still appear in a role dropdown so it can be assigned
    // to summarizer/utility without manual provider setup.
    renderWithProviders(<ModelSettings />)
    await waitFor(() => expect(screen.getByText('gpt-4o')).toBeTruthy())
    // Open the chat role's dropdown (its trigger shows the assigned "gpt-4o").
    fireEvent.click(screen.getByText('gpt-4o'))
    await waitFor(() => {
      expect(screen.getByText('minicpm5-1b')).toBeTruthy()
      expect(screen.getByText('local-minicpm')).toBeTruthy()
    })
  })
})
