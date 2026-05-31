import { describe, it, expect, vi, beforeEach } from 'vitest'
import { screen, waitFor } from '@testing-library/react'
import { renderWithProviders } from '@/test-utils/render'
import { WorkspaceSandboxSettings } from './WorkspaceSandboxSettings'

vi.mock('@/lib/tauri-bridge', () => ({
  listAlwaysAllowedPaths: vi.fn().mockResolvedValue(['/tmp', '/Users/me/notes']),
  addAlwaysAllowedPath: vi.fn().mockResolvedValue(undefined),
  removeAlwaysAllowedPath: vi.fn().mockResolvedValue(undefined),
  listSessionAllowedPaths: vi.fn().mockResolvedValue([]),
  promoteSessionPathToGlobal: vi.fn().mockResolvedValue(undefined),
  openFolderDialog: vi.fn().mockResolvedValue({ path: '/new/path', name: 'path' }),
}))

describe('WorkspaceSandboxSettings', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('renders the global allowed list from IPC', async () => {
    renderWithProviders(<WorkspaceSandboxSettings />)
    await waitFor(() => {
      expect(screen.getByText('/tmp')).toBeInTheDocument()
      expect(screen.getByText('/Users/me/notes')).toBeInTheDocument()
    })
  })

  it('shows empty state when global list is empty', async () => {
    const { listAlwaysAllowedPaths } = await import('@/lib/tauri-bridge')
    vi.mocked(listAlwaysAllowedPaths).mockResolvedValueOnce([])
    renderWithProviders(<WorkspaceSandboxSettings />)
    await waitFor(() => {
      expect(screen.getByText('尚未添加任何路径。')).toBeInTheDocument()
    })
  })

  it('shows "no active session" when sessionId is null', async () => {
    renderWithProviders(<WorkspaceSandboxSettings />)
    await waitFor(() => {
      expect(screen.getByText('没有活动会话。')).toBeInTheDocument()
    })
  })
})
