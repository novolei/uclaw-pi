import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, fireEvent, waitFor } from '@/test-utils/render'
import {
  OnboardingView,
  ONBOARDING_LOCAL_MODEL_KEY,
  readLocalModelOnboardingFlag,
} from './OnboardingView'

// OnboardingView embeds LocalModelStep, whose hook subscribes to the download
// progress event + runs the env preflight on mount. Stub the bridge so the
// preflight resolves with a disk-ok report and the subscription is a no-op.
// (The factory is hoisted above module scope, so the report is inlined here.)
vi.mock('@/lib/bridge/settings', () => ({
  settingsBridge: {
    checkLocalModelEnvironment: vi.fn().mockResolvedValue({
      diskFreeBytes: 50_000_000_000,
      diskOk: true,
      diskRequiredBytes: 1_000_000_000,
      ramTotalBytes: 16_000_000_000,
      ramAvailableBytes: 8_000_000_000,
      ramOk: true,
      metalAvailable: true,
      network: {
        modelscopeReachable: true,
        huggingfaceReachable: true,
        fastest: 'modelscope',
        anyReachable: true,
      },
    }),
    downloadLocalModel: vi.fn().mockResolvedValue(''),
    warmupLocalModel: vi.fn().mockResolvedValue(undefined),
    assignLocalModelToRoles: vi.fn().mockResolvedValue(undefined),
  },
  onLocalModelDownloadProgress: () => Promise.resolve(() => {}),
}))

function advanceTo(label: RegExp | string) {
  // Click 下一步 until the target step content is on screen.
  for (let i = 0; i < 6; i++) {
    if (screen.queryByText(label)) return
    const next = screen.queryByRole('button', { name: /下一步/ })
    if (!next) return
    fireEvent.click(next)
  }
}

describe('OnboardingView', () => {
  beforeEach(() => {
    localStorage.clear()
  })

  it('includes the local-model step in the sequence after API Key', () => {
    renderWithProviders(<OnboardingView onComplete={vi.fn()} />)
    // welcome → api-key → local-model
    fireEvent.click(screen.getByRole('button', { name: /下一步/ }))
    fireEvent.click(screen.getByRole('button', { name: /下一步/ }))
    expect(screen.getByText('本地模型（可选）')).toBeTruthy()
  })

  it('skipping the step persists the skipped flag and advances', async () => {
    renderWithProviders(<OnboardingView onComplete={vi.fn()} />)
    advanceTo('本地模型（可选）')
    await waitFor(() => expect(screen.getByRole('button', { name: '跳过' })).toBeTruthy())
    fireEvent.click(screen.getByRole('button', { name: '跳过' }))
    await waitFor(() => expect(readLocalModelOnboardingFlag()).toBe('skipped'))
    expect(localStorage.getItem(ONBOARDING_LOCAL_MODEL_KEY)).toBe('skipped')
    // Advanced off the local-model step to theme.
    await waitFor(() => expect(screen.getByText('选择你的风格')).toBeTruthy())
  })

  it('still reaches done + calls onComplete via the bottom nav', () => {
    const onComplete = vi.fn()
    renderWithProviders(<OnboardingView onComplete={onComplete} />)
    advanceTo('一切就绪！')
    expect(screen.getByText('一切就绪！')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: /开始使用/ }))
    expect(onComplete).toHaveBeenCalled()
  })
})
