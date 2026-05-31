import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { DeveloperOptionsSection } from './DeveloperOptionsSection'

// The setup-script EVENT stream now flows through settingsBridge wrappers
// (onSetupScriptOutput/onSetupScriptEnd) — was the raw Tauri event `listen`
// directly in the component before the migration. Mock the wrappers so expanding
// the section subscribes without touching the native event bus. `runSetupScript`
// stays in @/lib/embedding-endpoint (dev/setup helper) — stub it; keep the real
// SETUP_SCRIPTS / SETUP_SCRIPT_DESCRIPTORS so the cards render their real labels.
vi.mock('@/lib/bridge/settings', () => ({
  // Return value defined inline (no top-level var — vi.mock is hoisted).
  onSetupScriptOutput: vi.fn().mockResolvedValue(vi.fn()),
  onSetupScriptEnd: vi.fn().mockResolvedValue(vi.fn()),
}))

vi.mock('@/lib/embedding-endpoint', async (importActual) => {
  const actual = await importActual<typeof import('@/lib/embedding-endpoint')>()
  return { ...actual, runSetupScript: vi.fn().mockResolvedValue(undefined) }
})

describe('DeveloperOptionsSection', () => {
  it('renders the collapsed 开发者选项 header', () => {
    renderWithProviders(<DeveloperOptionsSection />)
    expect(screen.getByText('开发者选项')).toBeTruthy()
    expect(screen.getByText('DEV')).toBeTruthy()
  })

  it('expands to show the setup-script cards + subscribes via the bridge wrappers', async () => {
    const bridge = await import('@/lib/bridge/settings')
    const { user } = renderWithProviders(<DeveloperOptionsSection />)
    await user.click(screen.getByText('开发者选项'))
    // A real script card (from the unmocked SETUP_SCRIPT_DESCRIPTORS) renders.
    expect(screen.getByText('安装 Bun 运行时')).toBeTruthy()
    // The expand effect subscribed to both setup-script event streams.
    await waitFor(() => {
      expect(bridge.onSetupScriptOutput).toHaveBeenCalled()
      expect(bridge.onSetupScriptEnd).toHaveBeenCalled()
    })
  })
})
