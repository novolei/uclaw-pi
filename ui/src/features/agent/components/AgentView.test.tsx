/**
 * AgentView.test.tsx — smoke test for the top-level agent view shell after the
 * features/agent migration split (the final, largest agent leaf).
 *
 * The shell composes the header, the message list, the approval/status banners,
 * and the composer (RichTextInput + toolbar selectors) under the session
 * provider. This test mounts it with a fresh Jotai store + mocked agent bridge
 * (so nothing reaches the native Tauri layer) and asserts:
 *   1. it renders without crashing (the whole layout tree mounts).
 *   2. the agent composer's placeholder text is present (the model-not-selected
 *      hint), proving the composer sub-tree wired in.
 *
 * IPC is mocked at both the agent-bridge facade and the raw @tauri-apps layer
 * (some leaf children subscribe through the raw API directly).
 */

import { describe, it, expect, vi } from 'vitest'
import * as React from 'react'
import { renderWithProviders } from '@/test-utils/render'

// ── Module mocks ────────────────────────────────────────────────────────
// Raw Tauri layer (leaf children that still listen/invoke directly).
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }))

// The agent bridge — every command the shell, its hooks, and its children import
// is stubbed. Listener-style wrappers return an unlisten/cleanup fn.
vi.mock('@/lib/bridge/agent', () => {
  const noop = () => {}
  const unlisten = () => noop
  const asyncUnlisten = async () => noop
  return {
    // commands used by the shell + hooks
    sendAgentMessage: vi.fn(async () => {}),
    getAgentSessionMessages: vi.fn(async () => []),
    getAgentSessionPath: vi.fn(async () => ''),
    estimateSessionContext: vi.fn(async () => null),
    stopAgent: vi.fn(async () => {}),
    createAgentSession: vi.fn(async () => ({ id: 'new', title: '' })),
    forkAgentSession: vi.fn(async () => ({ id: 'fork', title: '' })),
    rewindSession: vi.fn(async () => ({})),
    saveFilesToAgentSession: vi.fn(async () => []),
    attachSessionDirectory: vi.fn(async () => []),
    agentSteer: vi.fn(async () => {}),
    agentFollowUp: vi.fn(async () => {}),
    openFileDialog: vi.fn(async () => ({ files: [] })),
    getPathForFile: vi.fn(() => null),
    checkPathsType: vi.fn(async () => ({ directories: [], files: [] })),
    updateSettings: vi.fn(async () => {}),
    getSttModelStatus: vi.fn(async () => ({ openflow_ready: false, openflow_model_dir: '' })),
    // listener wrappers (CleanupFn-returning, sync)
    onStreamComplete: vi.fn(unlisten),
    onStreamError: vi.fn(unlisten),
    onQueuedConsumed: vi.fn(unlisten),
    // listener wrappers used by children (Promise<UnlistenFn>)
    onAgentHeartbeat: vi.fn(asyncUnlisten),
    onAgentStalled: vi.fn(asyncUnlisten),
    onAgentStallRecovered: vi.fn(asyncUnlisten),
    onAgentInterruptedRecovered: vi.fn(asyncUnlisten),
    onChatStreamComplete: vi.fn(asyncUnlisten),
    onPlanModeSuggest: vi.fn(asyncUnlisten),
    consumePendingRecovery: vi.fn(async () => null),
    dismissPendingRecovery: vi.fn(async () => {}),
    interruptCurrentAgentRun: vi.fn(async () => ({ partialText: '', iteration: 0, stage: '', stalledForMs: 0, startedAt: 0 })),
    // safety + banner response commands used by composer-side children
    getSafetyPolicy: vi.fn(async () => ({ mode: 'ask' })),
    setSafetyMode: vi.fn(async () => {}),
    respondPermission: vi.fn(async () => {}),
    respondAskUser: vi.fn(async () => {}),
    respondExitPlanMode: vi.fn(async () => {}),
    respondPlanModeSuggest: vi.fn(async () => {}),
    // skill catalog (SkillSuggestionBar etc.)
    listSkills: vi.fn(async () => []),
    listLearnedSkills: vi.fn(async () => []),
    recordSkillCited: vi.fn(async () => {}),
    openExternal: vi.fn(async () => {}),
    readAttachment: vi.fn(async () => ''),
    saveImageAs: vi.fn(async () => {}),
    listAgentSessions: vi.fn(async () => []),
    updateAgentSessionTitle: vi.fn(async () => {}),
    moveAgentSessionToWorkspace: vi.fn(async () => {}),
    getSessionTrajectory: vi.fn(async () => []),
  }
})

import { AgentView } from './AgentView'

describe('AgentView (shell)', () => {
  it('mounts the agent view layout without crashing', () => {
    const { container } = renderWithProviders(<AgentView sessionId="s1" />)
    expect(container.firstChild).toBeTruthy()
  })

  it('renders the composer with the model-not-selected hint', () => {
    const { container } = renderWithProviders(<AgentView sessionId="s1" />)
    // With no active provider model, the composer shows the 请在下方工具栏选择模型 hint
    // and the RichTextInput placeholder for the unconfigured state.
    expect(container.textContent ?? '').toContain('请在下方工具栏选择模型')
  })
})
