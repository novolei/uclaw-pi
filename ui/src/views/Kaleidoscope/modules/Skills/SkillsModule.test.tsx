import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor, within } from '@/test-utils/render'
import userEvent from '@testing-library/user-event'
import { SkillsModule } from './SkillsModule'

const learnedFixture = [
  {
    id: 'L1', name: 'systematic-debugging', context: '修复流式 bug 的场景',
    principles: '先复现', steps: '1. 复现', pitfalls: '别猜',
    enabled: true, usageCount: 3, createdAt: '2026-05-12T10:00:00Z',
  },
]
const builtinFixture = [
  {
    name: 'brainstorming', version: '1.0.0', description: '把想法变成设计',
    author: 'uclaw', enabled: true, category: 'design', provenance: 'bundled' as const,
  },
]

const listLearnedSkills = vi.fn()
const listSkills = vi.fn()
const deleteLearnedSkill = vi.fn()

vi.mock('@/lib/tauri-bridge', () => ({
  listLearnedSkills: (...a: unknown[]) => listLearnedSkills(...a),
  toggleLearnedSkill: vi.fn().mockResolvedValue(undefined),
  deleteLearnedSkill: (...a: unknown[]) => deleteLearnedSkill(...a),
  proposeSkillConsolidation: vi.fn().mockResolvedValue({ clusters: [] }),
  backfillSkillKeywords: vi.fn().mockResolvedValue({ backfilledSkills: 0, totalLearnedSkills: 0, keywordsInserted: 0 }),
  listSkills: (...a: unknown[]) => listSkills(...a),
  toggleSkill: vi.fn().mockResolvedValue(true),
  forkSkillToUser: vi.fn().mockResolvedValue('~/.uclaw/skills/x'),
  reloadSkills: vi.fn().mockResolvedValue([]),
}))

// 重子树 —— stub 掉,本测试只关心 SkillsModule 的 merge / 分组 / 选中。
vi.mock('@/features/settings', () => ({
  SkillEvolutionTab: () => <div data-testid="skill-evolution-tab" />,
  SkillConsolidationDialog: () => null,
}))
vi.mock('react-markdown', () => ({ default: ({ children }: { children: string }) => <span>{children}</span> }))

// Mock Tauri event listener to avoid jsdom unhandled rejections
vi.mock('@tauri-apps/api/event', () => ({
  listen: () => Promise.resolve(() => {}),
}))

describe('SkillsModule', () => {
  beforeEach(() => {
    listLearnedSkills.mockReset().mockResolvedValue(learnedFixture)
    listSkills.mockReset().mockResolvedValue(builtinFixture)
    deleteLearnedSkill.mockReset().mockResolvedValue(undefined)
  })

  it('merges learned + builtin into the two groups with counts', async () => {
    renderWithProviders(<SkillsModule />)
    await waitFor(() => expect(screen.getAllByText('systematic-debugging').length).toBeGreaterThan(0))
    expect(screen.getByText('brainstorming')).toBeInTheDocument()
    // 分组标签使用 getAllByText 因为可能被截断或出现在 filter tabs
    expect(screen.getByText(/学得/)).toBeInTheDocument()
    expect(screen.getAllByText(/内建技能/).length).toBeGreaterThan(0)
  })

  it('collapses a group when its header is clicked', async () => {
    const user = userEvent.setup()
    renderWithProviders(<SkillsModule />)
    await waitFor(() => expect(screen.getAllByText('systematic-debugging').length).toBeGreaterThan(0))
    // 点击学得分组标题折叠
    const learnedHeaders = screen.getAllByText(/学得/)
    await user.click(learnedHeaders[0])
    // 折叠后列表中的技能名应消失（但"最近使用"区域的仍然存在）
    expect(screen.getAllByText('systematic-debugging').length).toBe(1)
  })

  it('shows the detail pane when a skill is selected', async () => {
    const user = userEvent.setup()
    renderWithProviders(<SkillsModule />)
    await waitFor(() => expect(screen.getAllByText('systematic-debugging').length).toBeGreaterThan(0))
    // 点击学得分组内的技能行
    const skillRows = screen.getAllByText('systematic-debugging')
    await user.click(skillRows[skillRows.length - 1])
    // 详情面板渲染了「场景」段(此 label 出现在分区导航和 Section 标题中)
    expect(screen.getAllByText('场景').length).toBeGreaterThanOrEqual(1)
  })

  it('renders the empty state when both sources are empty', async () => {
    listLearnedSkills.mockResolvedValue([])
    listSkills.mockResolvedValue([])
    renderWithProviders(<SkillsModule />)
    await waitFor(() =>
      expect(screen.getByText(/还没学到技能/)).toBeInTheDocument(),
    )
  })

  it('refetches and restores the skill when delete fails', async () => {
    const user = userEvent.setup()
    deleteLearnedSkill.mockRejectedValueOnce(new Error('backend offline'))
    renderWithProviders(<SkillsModule />)
    await waitFor(() => expect(screen.getAllByText('systematic-debugging').length).toBeGreaterThan(0))
    // 选中 → 点击学得分组内的技能行
    const skillRows = screen.getAllByText('systematic-debugging')
    await user.click(skillRows[skillRows.length - 1])
    await user.click(screen.getByRole('button', { name: '删除' }))
    await user.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: '删除' }))
    // 删除失败 → onConfirmDelete 走 refetch,fixture 仍返回该技能 → 它重新出现
    await waitFor(() =>
      expect(screen.getAllByText('systematic-debugging').length).toBeGreaterThan(0),
    )
  })
})
