import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { PermissionBanner } from './PermissionBanner'
import { allPendingPermissionRequestsAtom } from '@/atoms/agent-atoms'
import type { PermissionRequest } from '@/lib/agent-types'

// IPC routes through the agent bridge — mock it so the component never hits
// `@tauri-apps/api`.
vi.mock('@/lib/bridge/agent', () => ({
  stopAgent: vi.fn().mockResolvedValue(undefined),
  respondPermission: vi.fn().mockResolvedValue(undefined),
}))

const REQ: PermissionRequest = {
  requestId: 'perm-1',
  sessionId: 's1',
  toolName: 'Bash',
  toolInput: { command: 'rm -rf /tmp/x' },
  dangerLevel: 'dangerous',
  command: 'rm -rf /tmp/x',
}

function seeded() {
  const store = createStore()
  store.set(allPendingPermissionRequestsAtom, new Map([['s1', [REQ]]]))
  return store
}

describe('PermissionBanner', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('renders nothing when no pending request exists', () => {
    const { container } = renderWithProviders(<PermissionBanner sessionId="s1" />)
    expect(container.firstChild).toBeNull()
  })

  it('renders the dangerous-confirmation banner + command when a request is pending', () => {
    renderWithProviders(<PermissionBanner sessionId="s1" />, { store: seeded() })
    expect(screen.getByText('危险操作需要确认')).toBeInTheDocument()
    expect(screen.getByText('rm -rf /tmp/x')).toBeInTheDocument()
    expect(screen.getByText('允许')).toBeInTheDocument()
  })

  it('允许 routes through respondPermission(allow) on the bridge', async () => {
    const bridge = await import('@/lib/bridge/agent')
    const { user } = renderWithProviders(<PermissionBanner sessionId="s1" />, { store: seeded() })
    await user.click(screen.getByText('允许'))
    await waitFor(() => {
      expect(bridge.respondPermission).toHaveBeenCalledWith({
        requestId: 'perm-1',
        behavior: 'allow',
        alwaysAllow: false,
      })
    })
  })
})
