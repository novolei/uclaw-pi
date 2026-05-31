import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { render, screen, waitFor } from '@testing-library/react'
import { SkillEvolutionTab } from './SkillEvolutionTab'
import type { SkillVersionInfo } from '@/lib/tauri-bridge'

// Mock the entire tauri-bridge so the component never calls invoke()
vi.mock('@/lib/tauri-bridge', () => ({
  getSkillVersions: vi.fn(),
}))

import { getSkillVersions } from '@/lib/tauri-bridge'
const mockGetSkillVersions = vi.mocked(getSkillVersions)

beforeEach(() => {
  vi.clearAllMocks()
})

function makeVersion(overrides: Partial<SkillVersionInfo> = {}): SkillVersionInfo {
  return {
    id: 'v-001',
    status: 'active',
    content: '这是当前版本的内容',
    createdAt: '2026-04-01T10:00:00Z',
    ...overrides,
  }
}

describe('SkillEvolutionTab', () => {
  it('shows empty state when version list is empty', async () => {
    mockGetSkillVersions.mockResolvedValue([])

    render(<SkillEvolutionTab skillId="skill-abc" />)

    await waitFor(() => {
      expect(screen.getByText('尚无版本记录')).toBeInTheDocument()
    })
  })

  it('renders both active and previous version in side-by-side panes', async () => {
    const activeVer = makeVersion({
      id: 'v-002',
      status: 'active',
      content: '当前版本文字',
      createdAt: '2026-04-10T12:00:00Z',
    })
    const prevVer = makeVersion({
      id: 'v-001',
      status: 'deprecated',
      content: '历史版本文字',
      createdAt: '2026-04-01T10:00:00Z',
    })
    mockGetSkillVersions.mockResolvedValue([activeVer, prevVer])

    render(<SkillEvolutionTab skillId="skill-abc" />)

    await waitFor(() => {
      expect(screen.getByText('当前版本文字')).toBeInTheDocument()
    })
    expect(screen.getByText('历史版本文字')).toBeInTheDocument()
    // Both pane headers present
    expect(screen.getByText('当前版本')).toBeInTheDocument()
  })

  it('shows loading state initially before data resolves', async () => {
    // Never resolves during this test — we just check the loading UI
    let resolve!: (v: SkillVersionInfo[]) => void
    mockGetSkillVersions.mockReturnValue(new Promise<SkillVersionInfo[]>((res) => { resolve = res }))

    render(<SkillEvolutionTab skillId="skill-xyz" />)

    // Loading text should appear immediately
    expect(screen.getByText('加载中…')).toBeInTheDocument()

    // Resolve to avoid unhandled promise warning
    resolve([])
  })
})
