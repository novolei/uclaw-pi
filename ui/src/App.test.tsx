import { act } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { renderWithProviders, screen, fireEvent } from '@/test-utils/render'
import App, {
  ONBOARDING_COMPLETE_KEY,
  STARTUP_BROWSER_RUNTIME_STATUS_TIMEOUT_MS,
  STARTUP_SPLASH_EXIT_TRANSITION_MS,
  STARTUP_SPLASH_MIN_VISIBLE_MS,
} from './App'
import * as bridge from './lib/tauri-bridge'
import type { StartupRuntimePackStatusReport } from './lib/startup/startup-doctor'

vi.mock('./lib/tauri-bridge', () => ({
  getSettings: vi.fn(),
  getActiveModel: vi.fn(),
  getBrowserRuntimeStatus: vi.fn(),
}))

vi.mock('./components/app-shell/AppShell', () => ({
  AppShell: () => <main aria-label="App shell">App shell ready</main>,
}))

vi.mock('./components/onboarding/OnboardingView', () => ({
  OnboardingView: ({ onComplete }: { onComplete: () => void }) => (
    <main aria-label="Onboarding">
      <button type="button" onClick={onComplete}>
        finish onboarding
      </button>
    </main>
  ),
}))

vi.mock('./hooks/useGlobalChatListeners', () => ({
  useGlobalChatListeners: vi.fn(),
}))

vi.mock('./hooks/useGlobalAgentListeners', () => ({
  useGlobalAgentListeners: vi.fn(),
}))

vi.mock('./hooks/usePetStateSync', () => ({
  usePetStateSync: vi.fn(),
}))

const getSettings = vi.mocked(bridge.getSettings)
const getActiveModel = vi.mocked(bridge.getActiveModel)
const getBrowserRuntimeStatus = vi.mocked(bridge.getBrowserRuntimeStatus)

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((res) => {
    resolve = res
  })
  return { promise, resolve }
}

const settings = {
  language: 'zh-CN',
  theme: 'system',
  theme_style: 'default',
  safety_mode: 'yolo',
}

function runtimeReport(overrides: Partial<StartupRuntimePackStatusReport> = {}): StartupRuntimePackStatusReport {
  return {
    manifestPackVersion: '1.48.2-uclaw.1',
    ready: true,
    canRunBrowserTasks: true,
    primaryAction: 'keep_current',
    eventNames: ['browser.runtime.doctor.completed'],
    supervisorEventNames: ['browser.startup_doctor.check'],
    supervisor: {
      providerId: 'browser.local_chromium',
      selectedSessionId: 'startup',
      runtimeState: 'stopped',
      doctorStatus: 'deferred',
      activeContextCount: 0,
      activeContextSessions: [],
    },
    doctor: {
      status: 'ready',
      ready: true,
      remediation: 'Browser runtime is ready.',
      actions: ['keep_current'],
      manifestPackVersion: '1.48.2-uclaw.1',
      rollbackAvailable: true,
      activeTasks: 0,
    },
    operationPlan: {
      status: 'ready',
      summary: 'Browser runtime is ready.',
      eventNames: ['browser.runtime.keep_current.planned'],
    },
    ...overrides,
  }
}

describe('App startup route', () => {
  beforeEach(() => {
    localStorage.clear()
    // Default these splash/handoff tests to a returning user (onboarding done)
    // so the first-run gate doesn't divert them to OnboardingView. The
    // dedicated 'first-run onboarding gate' describe block clears this.
    localStorage.setItem(ONBOARDING_COMPLETE_KEY, '1')
    getSettings.mockReset()
    getActiveModel.mockReset().mockResolvedValue(null)
    getBrowserRuntimeStatus.mockReset().mockResolvedValue(runtimeReport())
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('renders the branded startup splash while initialization is pending', () => {
    const pendingSettings = deferred<typeof settings>()
    const pendingRuntime = deferred<StartupRuntimePackStatusReport>()
    getSettings.mockReturnValue(pendingSettings.promise)
    getBrowserRuntimeStatus.mockReturnValue(pendingRuntime.promise)

    renderWithProviders(<App />)

    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()
    expect(screen.getByText('Preparing uClaw')).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()
    expect(getBrowserRuntimeStatus).toHaveBeenCalledTimes(1)
  })

  it('keeps the splash visible for a perceptible minimum before AppShell handoff', async () => {
    vi.useFakeTimers()
    getSettings.mockResolvedValue(settings)

    renderWithProviders(<App />)

    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()

    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(getActiveModel).toHaveBeenCalledTimes(1)
    expect(localStorage.getItem('uclaw:language')).toBe('zh-CN')
    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_SPLASH_MIN_VISIBLE_MS - 1)
    })

    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1)
    })

    expect(screen.getByRole('heading', { name: 'uClaw' }).closest('[data-startup-splash-state]'))
      .toHaveAttribute('data-startup-splash-state', 'exiting')
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_SPLASH_EXIT_TRANSITION_MS)
    })

    expect(screen.getByRole('main', { name: 'App shell' })).toBeInTheDocument()
  })

  it('waits for Rust Browser Runtime status before AppShell handoff', async () => {
    vi.useFakeTimers()
    getSettings.mockResolvedValue(settings)
    const pendingRuntime = deferred<StartupRuntimePackStatusReport>()
    getBrowserRuntimeStatus.mockReturnValue(pendingRuntime.promise)

    renderWithProviders(<App />)

    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_SPLASH_MIN_VISIBLE_MS + STARTUP_SPLASH_EXIT_TRANSITION_MS)
    })

    expect(screen.getByRole('heading', { name: 'uClaw' })).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()

    await act(async () => {
      pendingRuntime.resolve(runtimeReport())
      await Promise.resolve()
    })

    expect(screen.getByRole('heading', { name: 'uClaw' }).closest('[data-startup-splash-state]'))
      .toHaveAttribute('data-startup-splash-state', 'exiting')

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_SPLASH_EXIT_TRANSITION_MS)
    })

    expect(screen.getByRole('main', { name: 'App shell' })).toBeInTheDocument()
  })

  it('records a bounded fallback when Rust Browser Runtime status fails', async () => {
    vi.useFakeTimers()
    getSettings.mockResolvedValue(settings)
    getBrowserRuntimeStatus.mockRejectedValue(new Error('runtime status unavailable'))
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)

    renderWithProviders(<App />)

    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(screen.getAllByText(/Rust Browser Runtime status is unavailable/).length).toBeGreaterThan(0)

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_SPLASH_MIN_VISIBLE_MS)
    })

    expect(screen.getByRole('heading', { name: 'uClaw' }).closest('[data-startup-splash-state]'))
      .toHaveAttribute('data-startup-splash-state', 'exiting')
    expect(consoleError).toHaveBeenCalledWith(
      '[App] Browser Runtime 状态读取失败:',
      expect.any(Error),
    )
  })

  it('records a bounded fallback when Rust Browser Runtime status hangs', async () => {
    vi.useFakeTimers()
    getSettings.mockResolvedValue(settings)
    const pendingRuntime = deferred<StartupRuntimePackStatusReport>()
    getBrowserRuntimeStatus.mockReturnValue(pendingRuntime.promise)
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)

    renderWithProviders(<App />)

    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_BROWSER_RUNTIME_STATUS_TIMEOUT_MS)
    })

    expect(screen.getAllByText(/did not respond within/).length).toBeGreaterThan(0)
    expect(screen.getByRole('heading', { name: 'uClaw' }).closest('[data-startup-splash-state]'))
      .toHaveAttribute('data-startup-splash-state', 'exiting')
    expect(consoleError).toHaveBeenCalledWith(
      '[App] Browser Runtime 状态读取失败:',
      expect.any(Error),
    )
  })
})

describe('App first-run onboarding gate', () => {
  beforeEach(() => {
    localStorage.clear()
    getSettings.mockReset().mockResolvedValue(settings)
    getActiveModel.mockReset().mockResolvedValue(null)
    getBrowserRuntimeStatus.mockReset().mockResolvedValue(runtimeReport())
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  // Drive past the startup splash so the post-splash branch (gate) renders.
  async function settleAfterSplash() {
    // Flush the init + runtime-status promises (microtasks) first.
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    // Then advance the min-visible timer (triggers exit) ...
    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_SPLASH_MIN_VISIBLE_MS)
    })
    // ... and the exit-transition timer (hides the splash).
    await act(async () => {
      await vi.advanceTimersByTimeAsync(STARTUP_SPLASH_EXIT_TRANSITION_MS)
    })
  }

  it('shows OnboardingView on first run (no active model, no flag)', async () => {
    vi.useFakeTimers()
    // No flag set; getActiveModel → null → activeProviderModelAtom stays null.
    renderWithProviders(<App />)
    await settleAfterSplash()

    expect(screen.getByRole('main', { name: 'Onboarding' })).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'App shell' })).not.toBeInTheDocument()
  })

  it('shows AppShell (not onboarding) when an active model is configured', async () => {
    vi.useFakeTimers()
    // Pre-seed the persisted active-model atom so the gate predicate is false
    // even though the onboarding flag is unset (returning/configured user).
    localStorage.setItem(
      'uclaw-active-provider-model',
      JSON.stringify({ providerId: 'openai', modelId: 'gpt-4o' }),
    )
    renderWithProviders(<App />)
    await settleAfterSplash()

    expect(screen.getByRole('main', { name: 'App shell' })).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'Onboarding' })).not.toBeInTheDocument()
  })

  it('shows AppShell when the onboarding-complete flag is set', async () => {
    vi.useFakeTimers()
    localStorage.setItem(ONBOARDING_COMPLETE_KEY, '1')
    renderWithProviders(<App />)
    await settleAfterSplash()

    expect(screen.getByRole('main', { name: 'App shell' })).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'Onboarding' })).not.toBeInTheDocument()
  })

  it('completing onboarding sets the flag and hands off to AppShell', async () => {
    vi.useFakeTimers()
    renderWithProviders(<App />)
    await settleAfterSplash()

    expect(screen.getByRole('main', { name: 'Onboarding' })).toBeInTheDocument()

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'finish onboarding' }))
    })

    expect(localStorage.getItem(ONBOARDING_COMPLETE_KEY)).toBe('1')
    expect(screen.getByRole('main', { name: 'App shell' })).toBeInTheDocument()
    expect(screen.queryByRole('main', { name: 'Onboarding' })).not.toBeInTheDocument()
  })
})
