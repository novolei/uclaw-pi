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
import type { DefaultPromptsResponse } from '../types'

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
}
