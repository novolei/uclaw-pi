import { clearMocks, mockConvertFileSrc, mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import type { InvokeArgs } from '@tauri-apps/api/core'

declare global {
  interface Window {
    __TAURI_INTERNALS__?: Record<string, unknown>
    __UCLAW_DEV_TAURI_MOCK__?: boolean
  }
}

type MockHandler = (cmd: string, payload?: InvokeArgs) => unknown

const settingsFixture = {
  language: 'zh-CN',
  theme: 'system',
  theme_style: 'default',
  provider: null,
  model: null,
  safety_mode: 'yolo',
}

const diagnosticsFixture = {
  app_version: 'dev-mock',
  platform: 'browser',
  arch: 'mock',
  memory_used_mb: 256,
  memory_total_mb: 1024,
  uptime_secs: 1,
  consecutive_failures: 0,
  recovery_attempts: 0,
  active_processes: 1,
  orphan_processes: 0,
  services: [
    { name: 'AppRuntimeService', status: 'Running', detail: 'mocked browser runtime' },
  ],
  memu: {
    running: true,
    pid: 1,
    reason: null,
    python_path: '/mock/python',
    script_path: '/mock/memu_bridge.py',
    db_path: '/mock/memu.db',
  },
  gbrain: {
    connected: true,
    tool_count: 6,
    pgdata_ready: true,
    error: null,
    status: 'connected',
    error_kind: null,
    suggested_action: null,
    home_path: '/mock/gbrain',
    launcher_path: '/mock/bun',
    pgdata_path: '/mock/pgdata',
    config_command: '/mock/bun',
    config_entry_path: '/mock/gbrain/src/cli.ts',
    config_command_exists: true,
    config_entry_exists: true,
    config_gbrain_home: '/mock/gbrain',
    path_stale: false,
  },
  gbrain_init: { status: 'skipped_already_initialized', at_ms: 1 },
}

const evalSuiteFixture = {
  passed: true,
  averageScore: 1,
  runIds: ['mock-run'],
  scorecards: [
    {
      caseId: 'mock.browser.ui_debug',
      title: 'Mock bridge keeps browser UI debuggable',
      passed: true,
      score: 1,
      checks: [{ id: 'mock_bridge_installed', passed: true, score: 1, message: 'ok' }],
    },
  ],
}

const selfImprovementFixture = [
  {
    candidateId: 'candidate.mock.ui_debug_loop',
    verdict: 'promote',
    score: 1,
    checks: [{ id: 'rollback_ref', passed: true, message: 'ok' }],
  },
]

const browserRuntimeControlCenterFixture = {
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
      probeHistory: [],
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
      probeHistory: [],
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
      probeHistory: [],
    },
  ],
  mcpIntegrationSummary: {
    builtIn: true,
    enabled: false,
    rawToolsExposed: false,
    configureRouteReady: true,
  },
  updatedAtMs: 0,
}

const browserRuntimeStatusFixture = {
  manifestPackVersion: 'browser-runtime-pack-v1',
  runtimeRoot: '/mock/uclaw/browser-runtime',
  currentPackDir: '/mock/uclaw/browser-runtime/packs/browser-runtime-pack-v1',
  ready: true,
  canRunBrowserTasks: true,
  primaryAction: 'keep_current',
  eventNames: ['browser.runtime.doctor.completed'],
  doctor: {
    status: 'ready',
    ready: true,
    remediation: 'Browser runtime is ready.',
    actions: ['keep_current', 'run_doctor'],
    manifestPackVersion: 'browser-runtime-pack-v1',
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
    selectedSessionId: 'mock-browser-runtime',
    runtimeState: 'ready',
    doctorStatus: 'ready',
    activeContextCount: 0,
    activeContextSessions: [],
    detail: 'Local Chromium fallback can create a supervised context on demand.',
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
      remediation: ['Enable and probe the provider before routing.'],
      notes: [],
    },
    playwrightMcp: {
      providerId: 'browser.playwright_mcp',
      displayName: 'Playwright MCP',
      readiness: 'needs_setup',
      ready: false,
      setupComplete: false,
      activeContexts: 0,
      remediation: ['Configure in Kaleidoscope Integrations.'],
      notes: [],
    },
  },
  controlCenter: browserRuntimeControlCenterFixture,
  supervisorEventNames: ['browser.startup_doctor.ready'],
}

const browserIdentityStatusFixture = {
  profiles: [],
  authorizedCount: 0,
  revokedCount: 0,
  activeTaskCount: 0,
  activeTasks: [],
}

export function shouldInstallDevTauriMock(): boolean {
  return import.meta.env.VITE_UCLAW_MOCK_TAURI === '1'
    && typeof window !== 'undefined'
    && !window.__TAURI_INTERNALS__?.invoke
}

export function createUclawMockIpcHandler(): MockHandler {
  return (cmd: string, payload?: InvokeArgs): unknown => {
    console.info('[uClaw mock Tauri IPC]', cmd, payload ?? {})

    switch (cmd) {
      case 'get_settings':
      case 'patch_settings':
        return settingsFixture
      case 'get_platform':
        return { platform: 'browser', arch: 'mock' }
      case 'get_version':
        return { version: 'dev-mock', commit: null, build_time: null }
      case 'get_bootstrap_status':
        return { complete: true, steps: [] }
      case 'get_active_model':
        return null
      case 'get_user_profile':
        return { userName: 'Mock User', avatar: '' }
      case 'list_conversations':
      case 'list_agent_sessions':
      case 'list_spaces':
      case 'list_notifications':
      case 'list_background_tasks':
      case 'list_mcp_servers':
      case 'list_mcp_tools':
      case 'list_skills':
      case 'list_channels':
      case 'list_pending_escalations':
      case 'get_daily_costs':
      case 'get_model_costs':
      case 'get_session_costs':
      case 'list_workspace_cost_rollup':
      case 'list_providers':
      case 'list_configured_providers':
      case 'get_all_configured_models':
      case 'list_provider_models':
      case 'get_configured_models':
      case 'list_automations':
      case 'get_automation_activity':
      case 'automation_list_specs':
      case 'automation_list_activities':
        return []
      case 'get_provider_config':
        return null
      case 'get_month_cost_total':
        return 0
      case 'get_system_diagnostics':
        return diagnosticsFixture
      case 'get_browser_runtime_status':
        return browserRuntimeStatusFixture
      case 'get_browser_runtime_control_center':
        return browserRuntimeControlCenterFixture
      case 'list_browser_identities':
        return browserIdentityStatusFixture
      case 'run_browser_runtime_provider_probe':
        return {
          providerId: payload?.providerId ?? 'browser.playwright_cli',
          state: 'passed',
          checkedAtMs: Date.now(),
          artifactId: 'browser-runtime-provider-probe-passed',
          message: 'Provider probe passed.',
          eventNames: ['browser.runtime.provider.probe.passed'],
        }
      case 'dry_run_browser_runtime_action':
        return {
          operation: payload?.action ?? 'keep_current',
          mode: 'dry_run',
          status: 'succeeded',
          summary: 'Mock dry-run completed without side effects.',
          artifactId: 'browser-runtime-mock-dry-run',
          eventNames: ['browser.runtime.mock.dry_run_succeeded'],
          stepReports: [],
          manifestPackVersion: browserRuntimeStatusFixture.manifestPackVersion,
          runtimeRoot: browserRuntimeStatusFixture.runtimeRoot,
          currentPackDir: browserRuntimeStatusFixture.currentPackDir,
          usesNetwork: false,
          destructive: false,
          requiresConfirmation: false,
          keepsCurrentPack: true,
        }
      case 'execute_browser_runtime_action':
        return {
          operation: payload?.action ?? 'keep_current',
          mode: 'managed',
          status: payload?.confirmed ? 'succeeded' : 'requires_confirmation',
          summary: payload?.confirmed
            ? 'Mock runtime action executed in uClaw-managed storage.'
            : 'Mock runtime action requires confirmation.',
          artifactId: 'browser-runtime-mock-managed',
          eventNames: ['browser.runtime.mock.managed_succeeded'],
          stepReports: [],
          manifestPackVersion: browserRuntimeStatusFixture.manifestPackVersion,
          runtimeRoot: browserRuntimeStatusFixture.runtimeRoot,
          currentPackDir: browserRuntimeStatusFixture.currentPackDir,
          sourceKind: 'dev_staging',
          sourceDir: '/mock/uclaw/src-tauri/.runtime-pack-staging/browser-runtime-pack-v1',
          usesNetwork: false,
          destructive: false,
          requiresConfirmation: !payload?.confirmed,
          keepsCurrentPack: payload?.action === 'keep_current',
        }
      case 'run_browser_parity_eval':
      case 'run_memory_gbrain_eval':
      case 'run_agent_control_plane_eval':
        return evalSuiteFixture
      case 'run_self_improvement_gate_eval':
        return selfImprovementFixture
      case 'restart_memu_bridge':
      case 'restart_gbrain_mcp':
      case 'reset_ai_engine':
        return { ok: true, mocked: true }
      case 'get_safety_policy':
        return { mode: 'yolo', tool_overrides: [] }
      case 'get_default_prompts':
        return { prompts: [] }
      case 'create_agent_session': {
        // Return a valid AgentSessionMeta so the new-session flow doesn't push a
        // null into agentSessions (which crashes working-atoms `sessions.map`).
        // `updatedAt`/`createdAt` are ms — WelcomeView formats them (else "Invalid Date").
        const now = Date.now()
        const id = `mock-agent-${now}`
        return { id, title: '新会话', archived: false, pinned: false, messageCount: 0, updatedAt: now, createdAt: now }
      }
      case 'create_conversation': {
        const now = Date.now()
        const id = `mock-conv-${now}`
        return { id, title: 'New Conversation', archived: false, updatedAt: now, createdAt: now }
      }
      default:
        console.warn(`[uClaw mock Tauri IPC] unhandled command: ${cmd}`)
        return null
    }
  }
}

export function installDevTauriMock(): void {
  if (!shouldInstallDevTauriMock() || window.__UCLAW_DEV_TAURI_MOCK__) return

  clearMocks()
  mockWindows('main')
  mockConvertFileSrc('macos')
  mockIPC(createUclawMockIpcHandler(), { shouldMockEvents: true })
  window.__UCLAW_DEV_TAURI_MOCK__ = true

  console.info('[uClaw mock Tauri IPC] installed for browser-only UI debugging')
}

installDevTauriMock()
