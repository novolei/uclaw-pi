// Settings-domain IPC bridge — the single entry for settings commands.
//
// Per the code-organization ADR (docs/adr/2026-05-31-pi-code-organization-discipline.md):
// components/atoms never call `@tauri-apps/api` directly; all IPC goes through a
// per-domain `lib/bridge/*` module. This is the first per-domain bridge (the
// frontend mirror of the backend `commands::settings` + `services::settings_service`),
// extracted out of the legacy catch-all `lib/tauri-bridge.ts`.
//
// Command names mirror the Rust `#[tauri::command]` fns; when `tauri-specta`/`ts-rs`
// generation lands these signatures become generated, not hand-written.

import { invoke } from '@tauri-apps/api/core'
import type { DefaultPromptsResponse, PatchSettingsInput, Settings } from '../types'
import type {
  BrowserRuntimeControlCenterReport,
  BrowserRuntimeProviderId,
  BrowserRuntimeProviderProbeSummary,
  StartupRuntimePackStatusReport,
} from '../startup/startup-doctor'
import type {
  BrowserIdentityRevocationReport,
  BrowserIdentityStatusReport,
  PlaywrightSetupAction,
  PlaywrightSetupExecutionReport,
} from '../tauri-bridge'
import type { SystemPrompt, SystemPromptConfig, SystemPromptVersion } from '../chat-types'

export const settingsBridge = {
  /** Whether the optional local HTTP API server is enabled (persisted; restart to apply). */
  getHttpApiEnabled: (): Promise<boolean> => invoke<boolean>('get_http_api_enabled'),
  /** Enable/disable the optional local HTTP API server (persisted; restart to apply). */
  setHttpApiEnabled: (enabled: boolean): Promise<void> =>
    invoke<void>('set_http_api_enabled', { enabled }),
  /** Snapshot the system-diagnostics report (health, bridges, services, processes). */
  getSystemDiagnostics: <T = unknown>(): Promise<T> => invoke<T>('get_system_diagnostics'),
  /** Run an eval suite by its command name (e.g. `run_agent_control_plane_eval`). */
  runEval: <T = unknown>(command: string): Promise<T> => invoke<T>(command),
  /** Invoke a side-effecting bridge/recovery action by command name (e.g. `restart_memu_bridge`). */
  bridgeAction: (command: string): Promise<void> => invoke<void>(command),
  /** Persist whether the agent suggests Plan mode for complex multi-step requests. */
  setPlanModeSuggestEnabled: (enabled: boolean): Promise<void> =>
    invoke<void>('set_plan_mode_suggest_enabled', { enabled }),
  /** Read the workspace-level `uclaw.md` project-context file (empty string if absent). */
  readWorkspaceUclawMd: (): Promise<string> => invoke<string>('read_workspace_uclaw_md'),
  /** Persist the workspace-level `uclaw.md` project-context file. */
  writeWorkspaceUclawMd: (content: string): Promise<void> =>
    invoke<void>('write_workspace_uclaw_md', { content }),
  /** Read the built-in default prompts (Karpathy baseline + per-mode additions). */
  readDefaultPrompts: (): Promise<DefaultPromptsResponse> =>
    invoke<DefaultPromptsResponse>('read_default_prompts'),
  /** Open `<workspace>/uclaw.md` in the OS default editor (creates it if missing). */
  openWorkspaceUclawMdExternally: (): Promise<void> =>
    invoke<void>('open_workspace_uclaw_md_externally'),

  // ── General-preferences IPC (was `getSettings`/`patchSettings` in tauri-bridge.ts) ──
  /** Read the persisted app settings (language, etc.). */
  getSettings: (): Promise<Settings> => invoke<Settings>('get_settings'),
  /** Patch the persisted app settings. */
  patchSettings: (input: PatchSettingsInput): Promise<Settings> =>
    invoke<Settings>('patch_settings', { input }),

  // ── System-prompt management IPC (was the prompt-config fns in tauri-bridge.ts).
  // The `.catch(...)` fallbacks are preserved verbatim so PromptSettings behavior is
  // identical: a failed read yields an empty/synthetic value rather than throwing.
  /** Read the full system-prompt config; `{ prompts: [] }` on failure. */
  getSystemPromptConfig: (): Promise<SystemPromptConfig> =>
    invoke<SystemPromptConfig>('get_system_prompt_config').catch(
      // Behavior preserved from the legacy `Promise<any>` wrapper: a failed read
      // yields a minimal `{ prompts: [] }` (the consumer reads `prompts`/
      // `defaultPromptId` defensively). Through `unknown` to keep the value exact.
      () => ({ prompts: [] }) as unknown as SystemPromptConfig,
    ),
  /** Create a custom system prompt; returns a synthetic record on failure. */
  createSystemPrompt: (input: { name: string; content: string }): Promise<SystemPrompt> =>
    invoke<SystemPrompt>('create_system_prompt', { input }).catch(
      () =>
        ({
          id: crypto.randomUUID(),
          name: input?.name ?? '',
          content: input?.content ?? '',
          isBuiltin: false,
        }) as SystemPrompt,
    ),
  /** Update a custom system prompt; echoes the input back on failure. */
  updateSystemPrompt: (
    id: string,
    input: { name: string; content: string },
  ): Promise<SystemPrompt> =>
    invoke<SystemPrompt>('update_system_prompt', { id, input }).catch(
      () => ({ id, ...input }) as SystemPrompt,
    ),
  /** Delete a custom system prompt; no-ops on failure. */
  deleteSystemPrompt: (id: string): Promise<void> =>
    invoke<void>('delete_system_prompt', { id }).catch(() => {}),
  /** Set the default system prompt; no-ops on failure. */
  setDefaultPrompt: (id: string): Promise<void> =>
    invoke<void>('set_default_prompt', { id }).catch(() => {}),
  /** Read a prompt's version history; `[]` on failure. */
  getSystemPromptVersions: (promptId: string): Promise<SystemPromptVersion[]> =>
    invoke<SystemPromptVersion[]>('get_system_prompt_versions', { promptId }).catch(() => []),

  // ── Browser Runtime supervisor + identity IPC (was the browser-runtime fns in
  // tauri-bridge.ts). Thin wrappers — these never swallowed errors. ──
  /** Snapshot the live browser-runtime status report. */
  getBrowserRuntimeStatus: (): Promise<StartupRuntimePackStatusReport> =>
    invoke<StartupRuntimePackStatusReport>('get_browser_runtime_status'),
  /** Snapshot the browser-runtime control-center report. */
  getBrowserRuntimeControlCenter: (): Promise<BrowserRuntimeControlCenterReport> =>
    invoke<BrowserRuntimeControlCenterReport>('get_browser_runtime_control_center'),
  /** Enable/disable a browser-runtime provider; returns the updated control center. */
  setBrowserRuntimeProviderEnabled: (
    providerId: BrowserRuntimeProviderId,
    enabled: boolean,
  ): Promise<BrowserRuntimeControlCenterReport> =>
    invoke<BrowserRuntimeControlCenterReport>('set_browser_runtime_provider_enabled', {
      providerId,
      enabled,
    }),
  /** Reorder the desired provider priority; returns the updated control center. */
  setBrowserRuntimeProviderPriority: (
    providerIds: BrowserRuntimeProviderId[],
  ): Promise<BrowserRuntimeControlCenterReport> =>
    invoke<BrowserRuntimeControlCenterReport>('set_browser_runtime_provider_priority', {
      providerIds,
    }),
  /** Toggle whether raw Playwright MCP tools are exposed; returns the updated control center. */
  setBrowserRuntimeMcpRawToolsExposed: (
    exposed: boolean,
  ): Promise<BrowserRuntimeControlCenterReport> =>
    invoke<BrowserRuntimeControlCenterReport>('set_browser_runtime_mcp_raw_tools_exposed', {
      exposed,
    }),
  /** Run a provider readiness probe through the Rust adapter. */
  runBrowserRuntimeProviderProbe: (
    providerId: BrowserRuntimeProviderId,
  ): Promise<BrowserRuntimeProviderProbeSummary> =>
    invoke<BrowserRuntimeProviderProbeSummary>('run_browser_runtime_provider_probe', {
      providerId,
    }),
  /** Run the official Playwright setup flow. */
  runPlaywrightSetup: (
    action: PlaywrightSetupAction,
  ): Promise<PlaywrightSetupExecutionReport> =>
    invoke<PlaywrightSetupExecutionReport>('run_playwright_setup', { action }),
  /** List managed browser identities. */
  listBrowserIdentities: (): Promise<BrowserIdentityStatusReport> =>
    invoke<BrowserIdentityStatusReport>('list_browser_identities'),
  /** Revoke a browser identity by profile id. */
  revokeBrowserIdentity: (profileId: string): Promise<BrowserIdentityRevocationReport> =>
    invoke<BrowserIdentityRevocationReport>('revoke_browser_identity', { profileId }),
}
