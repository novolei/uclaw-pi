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
import type { DefaultPromptsResponse, PatchSettingsInput, Settings, SpaceSummary } from '../types'
import type { ImChannelInput, ImChannelStatus } from '../../atoms/im-channel-atoms'
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
import type {
  ActiveManifestSkill,
  CreatePermissionRuleInput,
  PermissionAuditEntry,
  PermissionRule,
  SafetyPolicyResponse,
  ToolNameInput,
} from '../types'

/** Role→model assignment row (mirrors the Rust `ModelRoleConfig`). */
export interface ModelRoleConfig {
  role: string
  model_ref: string | null
}

/** Backend STT model-readiness probe result (mirrors the Rust `stt_model_status`). */
export interface SttModelStatusResult {
  openflow_ready: boolean
  openflow_model_dir: string
}

/** Request payload for `stt_download_model`. */
export interface SttDownloadModelRequest {
  preset: string
  force: boolean
}

/** Quantization of the local MiniCPM GGUF (mirrors the Rust `Quant` serde repr). */
export type LocalModelQuant = 'q4_k_m' | 'q8_0' | 'f16'

/** Download mirror label (mirrors the Rust `Source::label`). */
export type LocalModelSource = 'modelscope' | 'huggingface'

/** Result of `probe_download_sources`: fastest reachable mirror + per-source ms. */
export interface ProbeSourcesResult {
  fastest: LocalModelSource | null
  latencies: Record<string, number | null>
}

/** Payload of the `local-model:download-progress` event. */
export interface LocalModelProgress {
  downloaded: number
  total: number
  source: string | null
  phase: 'probing' | 'downloading' | 'verifying'
}

/** A freshly-issued WeChat iLink login QR code (polling token + image payload). */
export interface WechatIlinkQrcode {
  qrcode: string
  qrcode_img_content: string
}

/** Poll result for a pending WeChat iLink QR binding. */
export interface WechatIlinkQrcodeStatus {
  status: string
  bot_token?: string
  account_id?: string
}

export const settingsBridge = {
  /** Whether the optional local HTTP API server is enabled (persisted; restart to apply). */
  getHttpApiEnabled: (): Promise<boolean> => invoke<boolean>('get_http_api_enabled'),
  /** Enable/disable the optional local HTTP API server (persisted; restart to apply). */
  setHttpApiEnabled: (enabled: boolean): Promise<void> =>
    invoke<void>('set_http_api_enabled', { enabled }),
  /** Whether the agent routes through the experimental PiEngine backend (live runtime value). */
  getPiEngineEnabled: (): Promise<boolean> => invoke<boolean>('get_pi_engine_enabled'),
  /** Switch the agent engine backend (pi vs legacy); persisted + applied on the next message. */
  setPiEngineEnabled: (enabled: boolean): Promise<void> =>
    invoke<void>('set_pi_engine_enabled', { enabled }),
  /** Whether a skills.sh API key is stored (status only — the key is never returned). */
  getSkillsShApiKeySet: (): Promise<boolean> => invoke<boolean>('get_skills_sh_api_key_set'),
  /** Store (or clear, with `''`) the skills.sh API key the marketplace client uses. */
  setSkillsShApiKey: (key: string): Promise<void> =>
    invoke<void>('set_skills_sh_api_key', { key }),
  /** Whether a skillsmp.com API key is stored (status only; the key is optional). */
  getSkillsmpApiKeySet: (): Promise<boolean> => invoke<boolean>('get_skillsmp_api_key_set'),
  /** Store (or clear, with `''`) the optional skillsmp.com API key (raises rate limit). */
  setSkillsmpApiKey: (key: string): Promise<void> =>
    invoke<void>('set_skillsmp_api_key', { key }),
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

  // ── Active-skill manifest IPC (was `listActiveManifestSkills` in tauri-bridge.ts).
  // Powers the ToolSettings 活动技能 debug panel. ──
  /** Snapshot the active skill manifest the agent loop injects right now. */
  listActiveManifestSkills: (
    opts: { spaceId?: string; strategy?: string; maxEntries?: number } = {},
  ): Promise<ActiveManifestSkill[]> =>
    invoke<ActiveManifestSkill[]>('list_active_manifest_skills', {
      spaceId: opts.spaceId,
      strategy: opts.strategy,
      maxEntries: opts.maxEntries,
    }),

  // ── Role→model assignment IPC (was the role-model fns in tauri-bridge.ts).
  // Powers ModelSettings. Thin wrappers — these never swallowed errors. ──
  /** List configured providers and their model ids as `[providerId, modelIds[]][]`. */
  getAllConfiguredModels: (): Promise<[string, string[]][]> =>
    invoke<[string, string[]][]>('get_all_configured_models'),
  /** Read the per-role model assignments. */
  getRoleModels: (): Promise<ModelRoleConfig[]> => invoke<ModelRoleConfig[]>('get_role_models'),
  /** Assign (or clear, with `null`) the model for a role. */
  setRoleModel: (role: string, modelRef: string | null): Promise<void> =>
    invoke<void>('set_role_model', { role, modelRef }),

  // ── Permission rules + safety-policy IPC (was the permission/safety fns in
  // tauri-bridge.ts). Powers PermissionsSettings. ──
  /** List the V14 session + pattern permission rules. */
  listPermissionRules: (): Promise<PermissionRule[]> =>
    invoke<PermissionRule[]>('list_permission_rules'),
  /** Create a permission rule. */
  createPermissionRule: (input: CreatePermissionRuleInput): Promise<PermissionRule> =>
    invoke<PermissionRule>('create_permission_rule', { input }),
  /** Delete a permission rule by id. */
  deletePermissionRule: (id: string): Promise<boolean> =>
    invoke<boolean>('delete_permission_rule', { id }),
  /** Read the most-recent permission decisions (audit log). */
  listPermissionAudit: (sessionId?: string, limit = 100): Promise<PermissionAuditEntry[]> =>
    invoke<PermissionAuditEntry[]>('list_permission_audit', { sessionId, limit }),
  /** Read the safety policy (global allow/block lists + per-tool overrides). */
  getSafetyPolicy: (): Promise<SafetyPolicyResponse> => invoke<SafetyPolicyResponse>('get_safety_policy'),
  /** Remove a tool from the global auto-approve list; returns the updated policy. */
  removeAutoApprovedTool: (input: ToolNameInput): Promise<SafetyPolicyResponse> =>
    invoke<SafetyPolicyResponse>('remove_auto_approved_tool', { input }),
  /** Unblock a globally-blocked tool; returns the updated policy. */
  unblockTool: (input: ToolNameInput): Promise<SafetyPolicyResponse> =>
    invoke<SafetyPolicyResponse>('unblock_tool', { input }),

  // ── IM channel (机器人 / 渠道) IPC (was raw `invoke` inside the ImChannel* +
  // WechatIlinkBindingPanel components). Powers Settings → 机器人. Thin wrappers —
  // these never swallowed errors (callers own their try/catch + toast). The
  // list/status *atoms* keep their own IPC in `@/atoms/im-channel-atoms`. ──
  /** List the spaces a channel can bind to (id + name only at the call site). */
  listSpaces: (): Promise<SpaceSummary[]> => invoke<SpaceSummary[]>('list_spaces'),
  /** Create a new IM channel instance. */
  createImChannel: (input: ImChannelInput): Promise<void> =>
    invoke<void>('create_im_channel', { input }),
  /** Update an existing IM channel instance. */
  updateImChannel: (id: string, input: ImChannelInput): Promise<void> =>
    invoke<void>('update_im_channel', { id, input }),
  /** Enable/disable an IM channel instance. */
  toggleImChannel: (id: string, enabled: boolean): Promise<void> =>
    invoke<void>('toggle_im_channel', { id, enabled }),
  /** Delete an IM channel instance. */
  deleteImChannel: (id: string): Promise<void> => invoke<void>('delete_im_channel', { id }),

  // ── WeChat iLink QR-binding IPC (was raw `invoke` inside WechatIlinkBindingPanel). ──
  /** Request a fresh login QR code for a WeChat iLink instance. */
  requestWechatIlinkQrcode: (instanceId: string): Promise<WechatIlinkQrcode> =>
    invoke<WechatIlinkQrcode>('request_wechat_ilink_qrcode', { instanceId }),
  /** Poll the scan/confirm status of a pending WeChat iLink QR code. */
  pollWechatIlinkQrcodeStatus: (
    instanceId: string,
    qrcode: string,
  ): Promise<WechatIlinkQrcodeStatus> =>
    invoke<WechatIlinkQrcodeStatus>('poll_wechat_ilink_qrcode_status', { instanceId, qrcode }),
  /** Persist the bot token + account id once a WeChat iLink binding is confirmed. */
  saveWechatIlinkToken: (
    instanceId: string,
    botToken: string,
    accountId: string,
  ): Promise<void> =>
    invoke<void>('save_wechat_ilink_token', { instanceId, botToken, accountId }),
  /** Disconnect (unbind) a WeChat iLink instance. */
  disconnectWechatIlink: (instanceId: string): Promise<void> =>
    invoke<void>('disconnect_wechat_ilink', { instanceId }),

  // ── STT model IPC (was raw `invoke` inside SttSettings). Powers Settings → 语音.
  // Thin wrappers — these never swallowed errors (the caller owns its try/catch). ──
  /** Probe whether the OpenFlow STT model is downloaded + ready. */
  sttModelStatus: (): Promise<SttModelStatusResult> =>
    invoke<SttModelStatusResult>('stt_model_status'),
  /** Download (or force-redownload) the STT model; resolves to the model dir. */
  sttDownloadModel: (request: SttDownloadModelRequest): Promise<string> =>
    invoke<string>('stt_download_model', { request }),

  // ── Local MiniCPM model IPC (S2 smart download). Powers the Intelligence-tab
  // 本地模型 section. Thin wrappers — the caller owns its try/catch. ──
  /** Whether the local MiniCPM GGUF for `quant` (default Q4_K_M) is downloaded. */
  isLocalModelPresent: (quant?: LocalModelQuant): Promise<boolean> =>
    invoke<boolean>('is_local_model_present', { quant }),
  /** Concurrently probe ModelScope/HF latency for `quant`; for the progress bar. */
  probeDownloadSources: (quant?: LocalModelQuant): Promise<ProbeSourcesResult> =>
    invoke<ProbeSourcesResult>('probe_download_sources', { quant }),
  /** Download the local model for `quant` (default Q4_K_M) from `source`
   * (default: probe fastest); resolves to the GGUF path. Progress streams on
   * `local-model:download-progress` — subscribe via `onLocalModelDownloadProgress`. */
  downloadLocalModel: (quant?: LocalModelQuant, source?: LocalModelSource): Promise<string> =>
    invoke<string>('download_local_model', { quant, source }),
  /** Request cancellation of the in-flight local-model download (best-effort). */
  cancelLocalModelDownload: (): Promise<void> => invoke<void>('cancel_download'),
}

// ── Developer-options setup-script event stream (was `@tauri-apps/api/event`
// `listen` directly inside DeveloperOptionsSection). The hook subscribes through
// these wrappers so no settings component imports `@tauri-apps/api`. ──
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { SetupScriptEndEvent, SetupScriptOutputEvent } from '../embedding-endpoint'

/** Subscribe to per-line stdout/stderr from a running setup script. */
export const onSetupScriptOutput = (
  handler: (e: SetupScriptOutputEvent) => void,
): Promise<UnlistenFn> =>
  listen<SetupScriptOutputEvent>('system-setup-script:output', (e) => handler(e.payload))

/** Subscribe to the terminal exit event of a running setup script. */
export const onSetupScriptEnd = (
  handler: (e: SetupScriptEndEvent) => void,
): Promise<UnlistenFn> =>
  listen<SetupScriptEndEvent>('system-setup-script:end', (e) => handler(e.payload))

// ── IM channel realtime status stream (was `@tauri-apps/api/event` `listen`
// directly inside ImChannelsSettings). The hook subscribes through this wrapper
// so no settings component imports `@tauri-apps/api`. ──
/** Subscribe to backend-pushed per-instance IM channel status changes. */
export const onImChannelStatusChanged = (
  handler: (status: ImChannelStatus) => void,
): Promise<UnlistenFn> =>
  listen<ImChannelStatus>('im_channel_status_changed', (e) => handler(e.payload))

// ── Local MiniCPM download progress stream (S2). The hook subscribes through
// this wrapper so no settings component imports `@tauri-apps/api`. ──
/** Subscribe to local-model download progress (probing/downloading/verifying). */
export const onLocalModelDownloadProgress = (
  handler: (p: LocalModelProgress) => void,
): Promise<UnlistenFn> =>
  listen<LocalModelProgress>('local-model:download-progress', (e) => handler(e.payload))
