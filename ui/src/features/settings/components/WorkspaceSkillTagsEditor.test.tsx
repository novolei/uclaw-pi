import { describe, it, expect, vi } from 'vitest'
import { createStore } from 'jotai'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { activeWorkspaceIdAtom, workspacesAtom } from '@/atoms/workspace'
import { WorkspaceSkillTagsEditor } from './WorkspaceSkillTagsEditor'

// Tag IPC flows through useWorkspaceSkillTags → the typed @/lib/tauri-bridge
// get/setWorkspaceSkillTags helpers. Mock the bridge; seed the workspace atoms
// via a pre-seeded Jotai store (the hook reads them with useAtomValue).
vi.mock('@/lib/tauri-bridge', () => ({
  getWorkspaceSkillTags: vi.fn(async () => ['rust', 'tauri']),
  setWorkspaceSkillTags: vi.fn(async (_id: string, tags: string[]) =>
    tags.map((t) => t.toLowerCase()),
  ),
}))

function seededStore() {
  const store = createStore()
  store.set(workspacesAtom, [{ id: 'ws-1', name: '主工作区' }] as never)
  store.set(activeWorkspaceIdAtom, 'ws-1')
  return store
}

describe('WorkspaceSkillTagsEditor', () => {
  it('prompts to pick a workspace when none is active', () => {
    renderWithProviders(<WorkspaceSkillTagsEditor />)
    expect(screen.getByText('请先选择一个工作区。')).toBeInTheDocument()
  })

  it('renders the active workspace name and its loaded tags', async () => {
    renderWithProviders(<WorkspaceSkillTagsEditor />, { store: seededStore() })
    expect(screen.getByText('主工作区')).toBeInTheDocument()
    await waitFor(() => expect(screen.getByText('rust')).toBeInTheDocument())
    expect(screen.getByText('tauri')).toBeInTheDocument()
  })

  it('persists a new tag through the bridge when 添加 is clicked', async () => {
    const { user } = renderWithProviders(<WorkspaceSkillTagsEditor />, { store: seededStore() })
    await waitFor(() => expect(screen.getByText('rust')).toBeInTheDocument())
    const input = screen.getByPlaceholderText('输入标签，回车或逗号添加')
    await user.type(input, 'wasm')
    await user.click(screen.getByRole('button', { name: '添加' }))
    const { setWorkspaceSkillTags } = await import('@/lib/tauri-bridge')
    await waitFor(() =>
      expect(setWorkspaceSkillTags).toHaveBeenCalledWith('ws-1', ['rust', 'tauri', 'wasm']),
    )
  })
})
