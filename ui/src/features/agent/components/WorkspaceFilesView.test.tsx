import { describe, it, expect, vi } from 'vitest'
import * as React from 'react'
import { render, screen } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import { TooltipProvider } from '@/components/ui/tooltip'
import { WorkspaceFilesView } from './SidePanel'
import { currentAgentWorkspaceIdAtom } from '@/atoms/agent-atoms'

// FilesRail depends on many Tauri IPC calls — stub the whole component.
vi.mock('@/components/files-rail', () => ({
  FilesRail: () => <div data-testid="files-rail" />,
}))

vi.mock('@/lib/tauri-bridge', () => ({
  showInFinder: vi.fn(),
  openFile: vi.fn(),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

describe('WorkspaceFilesView', () => {
  it('renders FilesRail when a workspace is selected', () => {
    const store = createStore()
    store.set(currentAgentWorkspaceIdAtom, 'ws-x')

    render(
      <Provider store={store}>
        <TooltipProvider>
          <WorkspaceFilesView sessionId="s-1" sessionPath="/some/path" />
        </TooltipProvider>
      </Provider>
    )
    expect(screen.getByTestId('files-rail')).toBeTruthy()
  })

  it('shows workspace-selection prompt when no workspace is set', () => {
    const store = createStore()
    // currentAgentWorkspaceIdAtom defaults to null

    render(
      <Provider store={store}>
        <TooltipProvider>
          <WorkspaceFilesView sessionId="s-1" sessionPath={null} />
        </TooltipProvider>
      </Provider>
    )
    expect(screen.getByText(/请选择工作区/)).toBeTruthy()
  })
})
