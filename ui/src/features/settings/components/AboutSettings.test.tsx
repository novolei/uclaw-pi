import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { AboutSettings } from './AboutSettings'

// The hook loads version/platform via the typed tauri-bridge helpers; mock them
// so the component renders without a backend (never resolving keeps the "加载中"
// / "-" placeholders deterministic).
vi.mock('@/lib/tauri-bridge', () => ({
  getVersion: vi.fn(() => new Promise(() => {})),
  getPlatform: vi.fn(() => new Promise(() => {})),
}))

describe('AboutSettings', () => {
  it('renders the 关于 uClaw heading + 系统信息 section', () => {
    renderWithProviders(<AboutSettings />)
    expect(screen.getByText('关于 uClaw')).not.toBeNull()
    expect(screen.getByText('系统信息')).not.toBeNull()
    // Version not yet resolved → the loading placeholder is shown.
    expect(screen.getByText('加载中...')).not.toBeNull()
  })
})
