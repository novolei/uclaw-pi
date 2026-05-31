import { beforeEach, describe, expect, it, vi } from 'vitest'
import { screen, waitFor } from '@/test-utils/render'
import { renderWithProviders } from '@/test-utils/render'
import { kaleidoscopeModuleAtom, selectedBuiltinIntegrationAtom } from '@/atoms/kaleidoscope'
import { topLevelViewAtom } from '@/atoms/top-level-view'
import { BrowserRuntimeSettings } from './BrowserRuntimeSettings'
import type {
  BrowserRuntimeControlCenterReport,
  StartupRuntimePackStatusReport,
} from '@/lib/startup/startup-doctor'
import type { BrowserIdentityStatusReport } from '@/lib/tauri-bridge'
import { settingsBridge } from '../../../lib/bridge/settings'

// IPC now flows through the per-domain settings bridge (P3 migration); the test
// mocks that bridge instead of the legacy `@/lib/tauri-bridge`. The bare names
// below alias the mocked bridge methods so the call-count / arg assertions below
// stay identical to the pre-migration test.
vi.mock('../../../lib/bridge/settings', () => ({
  settingsBridge: {
    getBrowserRuntimeControlCenter: vi.fn(),
    getBrowserRuntimeStatus: vi.fn(),
    listBrowserIdentities: vi.fn(),
    revokeBrowserIdentity: vi.fn(),
    runBrowserRuntimeProviderProbe: vi.fn(),
    runPlaywrightSetup: vi.fn(),
    setBrowserRuntimeProviderEnabled: vi.fn(),
    setBrowserRuntimeProviderPriority: vi.fn(),
    setBrowserRuntimeMcpRawToolsExposed: vi.fn(),
  },
}))

const {
  getBrowserRuntimeControlCenter,
  getBrowserRuntimeStatus,
  listBrowserIdentities,
  revokeBrowserIdentity,
  runBrowserRuntimeProviderProbe,
  runPlaywrightSetup,
  setBrowserRuntimeProviderEnabled,
  setBrowserRuntimeProviderPriority,
} = settingsBridge

function runtimeReport(manifestPackVersion = '1.48.2-uclaw.1'): StartupRuntimePackStatusReport {
  return {
    manifestPackVersion,
    runtimeRoot: '/uclaw/browser-runtime',
    currentPackDir: '/uclaw/browser-runtime/current',
    ready: true,
    canRunBrowserTasks: true,
    primaryAction: 'keep_current',
    eventNames: ['browser.runtime.doctor.completed'],
    doctor: {
      status: 'ready',
      ready: true,
      remediation: 'Browser runtime is ready.',
      actions: ['keep_current'],
      manifestPackVersion,
      rollbackAvailable: true,
      activeTasks: 0,
    },
    operationPlan: {
      status: 'ready',
      summary: 'Runtime pack is ready.',
      eventNames: ['browser.runtime.keep_current.ready'],
    },
    supervisor: {
      providerId: 'browser.local_chromium',
      selectedSessionId: 'startup-runtime-status',
      runtimeState: 'ready',
      doctorStatus: 'ready',
      activeContextCount: 0,
      activeContextSessions: [],
      detail: 'Local Chromium supervisor is ready.',
    },
    providerReadiness: {
      localChromium: {
        providerId: 'browser.local_chromium',
        displayName: 'Local Chromium',
        readiness: 'ready',
        ready: true,
        setupComplete: true,
        activeContexts: 0,
        remediation: [],
        notes: [],
      },
      playwrightCli: {
        providerId: 'browser.playwright_cli',
        displayName: 'Playwright CLI',
        readiness: 'needs_setup',
        ready: false,
        setupComplete: false,
        activeContexts: 0,
        remediation: ['Run official Playwright setup.'],
        notes: [],
      },
      playwrightMcp: {
        providerId: 'browser.playwright_mcp',
        displayName: 'Playwright MCP',
        readiness: 'needs_setup',
        ready: false,
        setupComplete: false,
        activeContexts: 0,
        remediation: ['Run official Playwright setup.'],
        notes: [],
      },
    },
    controlCenter: {
      featureFlags: {
        playwrightCli: false,
        playwrightMcp: false,
        hostedBrowser: false,
        forceLegacyLocalChromium: false,
      },
      desiredProviderPriority: [
        'browser.playwright_cli',
        'browser.playwright_mcp',
        'browser.local_chromium',
      ],
      activeProviderRoute: {
        providerId: 'browser.local_chromium',
        displayName: 'Local Chromium',
      },
      providerLanes: [
        {
          providerId: 'browser.playwright_cli',
          displayName: 'Playwright CLI',
          enabled: false,
          priorityRank: 1,
          readiness: 'needs_setup',
          routable: false,
          routeRole: 'disabled',
          probeState: 'not_run',
          fallbackReason: 'provider_disabled',
          nextAction: 'enable_provider',
        },
        {
          providerId: 'browser.playwright_mcp',
          displayName: 'Playwright MCP',
          enabled: false,
          priorityRank: 2,
          readiness: 'needs_setup',
          routable: false,
          routeRole: 'disabled',
          probeState: 'not_run',
          fallbackReason: 'provider_disabled',
          nextAction: 'enable_mcp',
        },
        {
          providerId: 'browser.local_chromium',
          displayName: 'Local Chromium',
          enabled: true,
          priorityRank: 3,
          readiness: 'ready',
          routable: true,
          routeRole: 'active',
          probeState: 'passed',
          nextAction: 'none',
        },
      ],
      mcpIntegrationSummary: {
        builtIn: true,
        enabled: false,
        rawToolsExposed: false,
        configureRouteReady: false,
      },
      updatedAtMs: 0,
    },
    supervisorEventNames: ['browser.startup_doctor.ready'],
  }
}

function controlCenterWithCliEnabled(): BrowserRuntimeControlCenterReport {
  const controlCenter = runtimeReport().controlCenter as BrowserRuntimeControlCenterReport
  return {
    ...controlCenter,
    featureFlags: {
      ...controlCenter.featureFlags,
      playwrightCli: true,
    },
    providerLanes: controlCenter.providerLanes.map((lane) =>
      lane.providerId === 'browser.playwright_cli'
        ? {
            ...lane,
            enabled: true,
            routeRole: 'desired_first',
            fallbackReason: 'probe_not_passed',
            nextAction: 'run_probe',
          }
        : lane,
    ),
  }
}

function controlCenterNeedingPlaywrightSetup(): BrowserRuntimeControlCenterReport {
  const controlCenter = runtimeReport().controlCenter as BrowserRuntimeControlCenterReport
  return {
    ...controlCenter,
    featureFlags: {
      ...controlCenter.featureFlags,
      playwrightCli: true,
      playwrightMcp: true,
    },
    providerLanes: controlCenter.providerLanes.map((lane) =>
      lane.providerId === 'browser.playwright_cli' ||
      lane.providerId === 'browser.playwright_mcp'
        ? {
            ...lane,
            enabled: true,
            routeRole: lane.providerId === 'browser.playwright_cli' ? 'desired_first' : 'desired',
            fallbackReason: 'playwright_setup_not_ready',
            nextAction: 'run_playwright_setup',
          }
        : lane,
    ),
  }
}

function controlCenterWithCliProbePassed(): BrowserRuntimeControlCenterReport {
  const controlCenter = controlCenterWithCliEnabled()
  return {
    ...controlCenter,
    activeProviderRoute: {
      providerId: 'browser.playwright_cli',
      displayName: 'Playwright CLI',
    },
    providerLanes: controlCenter.providerLanes.map((lane) =>
      lane.providerId === 'browser.playwright_cli'
        ? {
            ...lane,
            routable: true,
            routeRole: 'active',
            probeState: 'passed',
            fallbackReason: undefined,
            nextAction: 'none',
            lastProbeArtifact: 'browser-runtime-provider-probe-passed',
          }
        : lane.providerId === 'browser.local_chromium'
          ? {
              ...lane,
              routeRole: 'desired',
            }
          : lane,
    ),
  }
}

function controlCenterWithMcpRouteReady(): BrowserRuntimeControlCenterReport {
  const controlCenter = runtimeReport().controlCenter as BrowserRuntimeControlCenterReport
  return {
    ...controlCenter,
    mcpIntegrationSummary: {
      ...controlCenter.mcpIntegrationSummary,
      configureRouteReady: true,
    },
  }
}

function identityReport(
  overrides: Partial<BrowserIdentityStatusReport> = {},
): BrowserIdentityStatusReport {
  return {
    profiles: [],
    authorizedCount: 0,
    revokedCount: 0,
    activeTaskCount: 0,
    activeTasks: [],
    ...overrides,
  }
}

function authorizedIdentityReport(): BrowserIdentityStatusReport {
  return identityReport({
    profiles: [
      {
        id: 'auth-example',
        label: 'Example',
        originPattern: 'https://*.example.com',
        kind: 'storage_state',
        provider: 'playwright',
        scope: 'global',
        createdAtMs: 1_770_000_000_000,
        lastUsedAtMs: 1_770_000_010_000,
        lastVerifiedAtMs: null,
        expiresAtMs: null,
        revokedAtMs: null,
        status: 'live',
        revoked: false,
      },
    ],
    authorizedCount: 1,
    revokedCount: 0,
  })
}

function revokedIdentityReport(): BrowserIdentityStatusReport {
  return identityReport({
    profiles: [
      {
        ...authorizedIdentityReport().profiles[0],
        status: 'revoked',
        revoked: true,
        revokedAtMs: 1_770_000_020_000,
      },
    ],
    authorizedCount: 0,
    revokedCount: 1,
  })
}

function activeIdentityTaskReport(): BrowserIdentityStatusReport {
  return identityReport({
    ...authorizedIdentityReport(),
    activeTaskCount: 2,
    activeTasks: [
      {
        profileId: 'auth-example',
        runId: 'run-active-1',
        sessionId: 'session-active-1',
        task: 'Use an authorized dashboard',
        status: 'running',
        startedAtMs: 1_770_000_010_000,
        updatedAtMs: 1_770_000_020_000,
        drainDeadlineMs: null,
      },
      {
        profileId: 'auth-example',
        runId: 'run-draining-2',
        sessionId: 'session-draining-2',
        task: 'Finish a revoked identity action',
        status: 'paused_checkpointed',
        startedAtMs: 1_770_000_030_000,
        updatedAtMs: 1_770_000_040_000,
        drainDeadlineMs: 1_770_000_045_000,
      },
    ],
  })
}

describe('BrowserRuntimeSettings', () => {
  beforeEach(() => {
    vi.mocked(getBrowserRuntimeControlCenter).mockReset()
    vi.mocked(getBrowserRuntimeStatus).mockReset()
    vi.mocked(listBrowserIdentities).mockReset()
    vi.mocked(revokeBrowserIdentity).mockReset()
    vi.mocked(runBrowserRuntimeProviderProbe).mockReset()
    vi.mocked(runPlaywrightSetup).mockReset()
    vi.mocked(setBrowserRuntimeProviderEnabled).mockReset()
    vi.mocked(setBrowserRuntimeProviderPriority).mockReset()
    vi.mocked(getBrowserRuntimeControlCenter).mockReturnValue(
      new Promise(() => {}),
    )
    vi.mocked(listBrowserIdentities).mockReturnValue(
      new Promise<BrowserIdentityStatusReport>(() => {}),
    )
  })

  it('renders a readonly default surface while live status is pending', () => {
    vi.mocked(getBrowserRuntimeStatus).mockReturnValue(
      new Promise<StartupRuntimePackStatusReport>(() => {}),
    )

    renderWithProviders(<BrowserRuntimeSettings />)

    expect(screen.getByText('Browser Automation')).toBeInTheDocument()
    expect(screen.getByText('运行时 Supervisor')).toBeInTheDocument()
    expect(screen.queryByText(['Playwright', 'runtime', 'pack'].join(' '))).not.toBeInTheDocument()
    expect(screen.getAllByText('未检查').length).toBeGreaterThan(1)
    expect(screen.queryByRole('button', { name: '预览准备' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '运行诊断' })).not.toBeInTheDocument()
  })

  it('loads browser identity status through the dedicated bridge', async () => {
    vi.mocked(listBrowserIdentities).mockResolvedValueOnce(authorizedIdentityReport())

    renderWithProviders(<BrowserRuntimeSettings />)

    await waitFor(() => {
      expect(listBrowserIdentities).toHaveBeenCalledTimes(1)
    })
    expect(screen.getByText('浏览器身份')).toBeInTheDocument()
    expect(screen.getByText('Example')).toBeInTheDocument()
    expect(screen.getByText('https://*.example.com · Playwright · Global')).toBeInTheDocument()
    expect(screen.getByText('1 可用 / 0 已撤销')).toBeInTheDocument()
    expect(screen.getByText('0 个任务')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '撤销 Example' })).toBeEnabled()
  })

  it('revokes a browser identity and refreshes status', async () => {
    vi.mocked(listBrowserIdentities)
      .mockResolvedValueOnce(authorizedIdentityReport())
      .mockResolvedValueOnce(revokedIdentityReport())
    vi.mocked(revokeBrowserIdentity).mockResolvedValueOnce({
      profile: revokedIdentityReport().profiles[0],
      revoked: true,
      activeTaskCount: 0,
      activeTasks: [],
      drainDeadlineMs: null,
    })

    const { user } = renderWithProviders(<BrowserRuntimeSettings />)

    await waitFor(() => {
      expect(screen.getByRole('button', { name: '撤销 Example' })).toBeEnabled()
    })

    await user.click(screen.getByRole('button', { name: '撤销 Example' }))

    await waitFor(() => {
      expect(revokeBrowserIdentity).toHaveBeenCalledWith('auth-example')
    })
    await waitFor(() => {
      expect(listBrowserIdentities).toHaveBeenCalledTimes(2)
    })
    expect(screen.getByRole('button', { name: '已撤销 Example' })).toBeDisabled()
    expect(screen.getByText('0 可用 / 1 已撤销')).toBeInTheDocument()
  })

  it('renders browser identity active task details', async () => {
    vi.mocked(listBrowserIdentities).mockResolvedValueOnce(activeIdentityTaskReport())

    renderWithProviders(<BrowserRuntimeSettings />)

    await waitFor(() => {
      expect(screen.getByText('Use an authorized dashboard')).toBeInTheDocument()
    })
    expect(screen.getByText('Finish a revoked identity action')).toBeInTheDocument()
    expect(screen.getByText('2 个任务')).toBeInTheDocument()
    expect(screen.getByText('运行中')).toBeInTheDocument()
    expect(screen.getByText('已检查点暂停')).toBeInTheDocument()
    expect(screen.getByText('session-active-1 · run-active-1')).toBeInTheDocument()
    expect(screen.getByText('session-draining-2 · run-draining-2')).toBeInTheDocument()
    expect(screen.getByText(/撤销 drain 至/)).toBeInTheDocument()
  })

  it('loads live runtime status through the dedicated read-only bridge', async () => {
    vi.mocked(getBrowserRuntimeStatus).mockResolvedValueOnce(runtimeReport())

    renderWithProviders(<BrowserRuntimeSettings />)

    await waitFor(() => {
      expect(getBrowserRuntimeStatus).toHaveBeenCalledTimes(1)
    })
    expect(screen.getByText('Rust Browser Runtime Supervisor')).toBeInTheDocument()
    expect(screen.getByText('Local Chromium: Ready, 0 个上下文')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '预览保持当前' })).not.toBeInTheDocument()
  })

  it('renders control center provider lanes and enables CLI through IPC', async () => {
    vi.mocked(getBrowserRuntimeStatus).mockResolvedValueOnce(runtimeReport())
    vi.mocked(setBrowserRuntimeProviderEnabled).mockResolvedValueOnce(controlCenterWithCliEnabled())

    const { user } = renderWithProviders(<BrowserRuntimeSettings />)

    await waitFor(() => {
      expect(screen.getByText('Browser Automation')).toBeInTheDocument()
    })
    expect(screen.getByText('Playwright CLI > Playwright MCP > Local Chromium')).toBeInTheDocument()
    expect(screen.getByText('Built-in Playwright Skills')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Enable provider' }))

    expect(setBrowserRuntimeProviderEnabled).toHaveBeenCalledWith('browser.playwright_cli', true)
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Run Playwright CLI probe' })).toBeEnabled()
    })
    expect(screen.getByText(
      'Official Playwright CLI is installed; run the Rust adapter probe before routing browser actions.',
    )).toBeInTheDocument()
  })

  it('runs official Playwright setup from an actionable setup button', async () => {
    vi.mocked(getBrowserRuntimeStatus).mockResolvedValueOnce({
      ...runtimeReport(),
      controlCenter: controlCenterNeedingPlaywrightSetup(),
    })
    vi.mocked(runPlaywrightSetup).mockResolvedValueOnce({
      action: 'auto_setup',
      status: 'succeeded',
      blockedReason: null,
      stepReports: [
        {
          stepId: 'install_playwright_cli',
          command: 'npm',
          args: ['install', '-g', '@playwright/cli@latest'],
          status: 'succeeded',
          exitCode: 0,
          stdout: '',
          stderr: '',
          error: null,
        },
      ],
    })
    vi.mocked(getBrowserRuntimeControlCenter)
      .mockResolvedValueOnce(controlCenterNeedingPlaywrightSetup())
      .mockResolvedValueOnce(controlCenterWithCliEnabled())

    const { user } = renderWithProviders(<BrowserRuntimeSettings />)

    await screen.findAllByText('Needs setup')
    const setupButton = screen
      .getAllByRole('button', { name: 'Set up' })
      .find((button) => !button.hasAttribute('disabled'))
    expect(setupButton).toBeDefined()
    await user.click(setupButton as HTMLElement)

    expect(runPlaywrightSetup).toHaveBeenCalledWith('auto_setup')
    await waitFor(() => {
      expect(screen.getByText('Last setup succeeded; 1 step(s).')).toBeInTheDocument()
    })
  })

  it('runs a CLI probe and refreshes the control center lane', async () => {
    vi.mocked(getBrowserRuntimeStatus).mockResolvedValueOnce({
      ...runtimeReport(),
      controlCenter: controlCenterWithCliEnabled(),
    })
    vi.mocked(runBrowserRuntimeProviderProbe).mockResolvedValueOnce({
      providerId: 'browser.playwright_cli',
      state: 'passed',
      checkedAtMs: 1,
      artifactId: 'browser-runtime-provider-probe-passed',
      message: 'Provider probe passed.',
      eventNames: ['browser.runtime.provider.probe.passed'],
    })
    vi.mocked(getBrowserRuntimeControlCenter)
      .mockResolvedValueOnce(controlCenterWithCliEnabled())
      .mockResolvedValueOnce(controlCenterWithCliProbePassed())

    const { user } = renderWithProviders(<BrowserRuntimeSettings />)

    await user.click(await screen.findByRole('button', { name: 'Run Playwright CLI probe' }))

    expect(runBrowserRuntimeProviderProbe).toHaveBeenCalledWith('browser.playwright_cli')
    await waitFor(() => {
      expect(screen.getAllByText('Playwright CLI').length).toBeGreaterThan(1)
      expect(screen.getByText('Active')).toBeInTheDocument()
    })
  })

  it('routes Configure MCP to Kaleidoscope Integrations built-in detail', async () => {
    const { store, user } = renderWithProviders(
      <BrowserRuntimeSettings
        status={{
          report: {
            ...runtimeReport(),
            controlCenter: controlCenterWithMcpRouteReady(),
          },
        }}
      />,
    )

    await user.click(await screen.findByRole('button', { name: 'Configure Playwright MCP' }))

    expect(store.get(topLevelViewAtom)).toBe('kaleidoscope')
    expect(store.get(kaleidoscopeModuleAtom)).toBe('integrations')
    expect(store.get(selectedBuiltinIntegrationAtom)).toBe('playwright_mcp')
  })

  it('renders raw JSON diagnostics collapsed by default', async () => {
    const { user } = renderWithProviders(
      <BrowserRuntimeSettings
        status={{
          report: runtimeReport(),
        }}
      />,
    )

    expect(screen.getByText('Diagnostics')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Show raw Browser Runtime report' })).toBeInTheDocument()
    expect(screen.queryByText('"desiredProviderPriority"')).not.toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Show raw Browser Runtime report' }))

    expect(screen.getByText(/"desiredProviderPriority"/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Hide raw Browser Runtime report' })).toBeInTheDocument()
  })

  it('does not render legacy runtime pack metadata from the status report adapter', () => {
    renderWithProviders(
      <BrowserRuntimeSettings
        status={{
          report: runtimeReport(),
          artifactSizeBytes: 734 * 1024 * 1024,
          runtimePackPath: '/uclaw/browser-runtime/v1',
          releaseChannel: 'stable',
          updateState: 'current',
          developerFallbackEnabled: false,
          autoPrepareEnabled: true,
        }}
      />,
    )

    expect(screen.getAllByText('可用').length).toBeGreaterThan(1)
    expect(screen.queryByText('更新状态')).not.toBeInTheDocument()
    expect(screen.queryByText('当前 pack')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '预览保持当前' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '关闭自动准备' })).not.toBeInTheDocument()
    expect(screen.queryByText('操作预览')).not.toBeInTheDocument()
  })
})
