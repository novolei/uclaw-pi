import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { PermissionsSettings } from './PermissionsSettings'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'

// IPC now flows through settingsBridge (was @/lib/tauri-bridge before the P3a
// migration) — mock the bridge so the cards render the mocked rows.
vi.mock('../../../lib/bridge/settings', () => ({
  settingsBridge: {
    listPermissionRules: vi.fn(async () => [
      { id: 'r1', scope: 'pattern', toolName: 'bash', target: 'git status', mode: 'allow', createdAt: 1715000000000 },
    ]),
    listPermissionAudit: vi.fn(async () => [
      { id: 'a1', sessionId: 'sess-aaa', toolName: 'bash', argsHash: 'abc1', decision: 'auto_approve', createdAt: 1715000000000 },
    ]),
    createPermissionRule: vi.fn(async (i) => ({ ...i, id: 'new', createdAt: Date.now() })),
    deletePermissionRule: vi.fn(async () => true),
    getSafetyPolicy: vi.fn(async () => ({
      globalMode: 'supervised',
      toolOverrides: {},
      autoApprovedTools: ['grep', 'glob', 'read_file'],
      blockedTools: [],
    })),
    removeAutoApprovedTool: vi.fn(async () => ({
      globalMode: 'supervised',
      toolOverrides: {},
      autoApprovedTools: ['glob', 'read_file'],
      blockedTools: [],
    })),
    unblockTool: vi.fn(async () => ({
      globalMode: 'supervised',
      toolOverrides: {},
      autoApprovedTools: ['grep', 'glob', 'read_file'],
      blockedTools: [],
    })),
  },
}))

// The WorkspaceSandboxSettings sub-tree (still under components/settings/) pulls
// its own sandbox IPC from @/lib/tauri-bridge on mount — stub those so it renders
// quietly inside the shell.
vi.mock('@/lib/tauri-bridge', () => ({
  listAlwaysAllowedPaths: vi.fn(async () => []),
  listSessionAllowedPaths: vi.fn(async () => []),
  addAlwaysAllowedPath: vi.fn(async () => undefined),
  removeAlwaysAllowedPath: vi.fn(async () => undefined),
  promoteSessionPathToGlobal: vi.fn(async () => undefined),
  openFolderDialog: vi.fn(async () => null),
}))

describe('PermissionsSettings', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('renders the rules table row', async () => {
    renderWithProviders(<PermissionsSettings />)
    await waitFor(() => {
      expect(screen.getByText('git status')).toBeInTheDocument()
      expect(screen.getAllByText('bash').length).toBeGreaterThan(0)
    })
  })

  it('renders the audit log row', async () => {
    renderWithProviders(<PermissionsSettings />)
    await waitFor(() => {
      expect(screen.getByText('自动允许')).toBeInTheDocument()
      expect(screen.getByText('abc1')).toBeInTheDocument()
    })
  })

  it('renders empty states when both lists are empty', async () => {
    const { settingsBridge } = await import('../../../lib/bridge/settings')
    vi.mocked(settingsBridge.listPermissionRules).mockResolvedValueOnce([])
    vi.mocked(settingsBridge.listPermissionAudit).mockResolvedValueOnce([])
    renderWithProviders(<PermissionsSettings />)
    await waitFor(() => {
      expect(screen.getByText('暂无规则')).toBeInTheDocument()
      expect(screen.getByText('暂无审计记录')).toBeInTheDocument()
    })
  })

  it('renders the global allow-list with each whitelisted tool', async () => {
    renderWithProviders(<PermissionsSettings />)
    await waitFor(() => {
      // Allow-list section header
      expect(screen.getByText('全局放行 / 阻止')).toBeInTheDocument()
      // Each whitelisted tool from the mock
      expect(screen.getByText('grep')).toBeInTheDocument()
      expect(screen.getByText('glob')).toBeInTheDocument()
      expect(screen.getByText('read_file')).toBeInTheDocument()
    })
  })

  it('removeAutoApprovedTool is called when whitelist row trash button clicked', async () => {
    const { settingsBridge } = await import('../../../lib/bridge/settings')
    const { user } = renderWithProviders(<PermissionsSettings />)
    await waitFor(() => expect(screen.getByText('grep')).toBeInTheDocument())
    // Find the row containing 'grep' and click its (hover-revealed) Trash button
    const row = screen.getByText('grep').closest('li')!
    const trashButton = row.querySelector('button[title="移除"]') as HTMLButtonElement
    expect(trashButton).not.toBeNull()
    await user.click(trashButton)
    expect(settingsBridge.removeAutoApprovedTool).toHaveBeenCalledWith({ toolName: 'grep' })
  })
})
