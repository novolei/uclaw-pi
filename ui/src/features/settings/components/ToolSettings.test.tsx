import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { ToolSettings } from './ToolSettings'

// ToolSettings loads the active-skill manifest on mount through `settingsBridge`.
// Mock the bridge so the panel renders the mocked rows (no native IPC). The
// WorkspaceSkillTagsEditor sub-tree pulls its own tauri-bridge calls — mock those
// too so it doesn't throw on mount.
vi.mock('../../../lib/bridge/settings', () => ({
  settingsBridge: {
    listActiveManifestSkills: vi.fn().mockResolvedValue([
      { rank: 1, name: 'skill-alpha', summary: 'first skill', provenance: 'bundled', citedCount: 0 },
    ]),
  },
}))

vi.mock('@/lib/tauri-bridge', () => ({
  getWorkspaceSkillTags: vi.fn().mockResolvedValue([]),
  setWorkspaceSkillTags: vi.fn().mockResolvedValue(undefined),
}))

describe('ToolSettings', () => {
  it('renders the 工具设置 heading + active-manifest panel', async () => {
    renderWithProviders(<ToolSettings />)
    expect(screen.getByText('工具设置')).toBeTruthy()
    expect(screen.getByText('活动技能（调试）')).toBeTruthy()
    await waitFor(() => expect(screen.getByText('skill-alpha')).toBeTruthy())
  })
})
