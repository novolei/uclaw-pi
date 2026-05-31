import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import type { SkillConsolidationProposal } from '@/lib/tauri-bridge'
import { SkillConsolidationDialog } from './SkillConsolidationDialog'

// The apply action flows through useSkillConsolidation → the typed
// @/lib/tauri-bridge applySkillConsolidation helper. Mock the bridge so the
// dialog renders without invoke(); react-markdown is stubbed to a plain span.
vi.mock('@/lib/tauri-bridge', () => ({
  applySkillConsolidation: vi.fn(async () => ({
    appliedClusters: 1,
    updatedSkills: 1,
    deprecatedSkills: 2,
  })),
}))
vi.mock('react-markdown', () => ({
  default: ({ children }: { children: string }) => <span>{children}</span>,
}))

const proposal: SkillConsolidationProposal = {
  totalSkills: 3,
  proposedCanonicalCount: 1,
  clusters: [
    {
      canonicalId: 'c1',
      canonicalTitle: '保留技能',
      mergedTitle: '合并后技能',
      duplicateIds: ['d1', 'd2'],
      duplicateTitles: ['弃用甲', '弃用乙'],
      reason: '高度重复',
      mergedContext: null,
      mergedPrinciples: null,
      mergedSteps: null,
      mergedPitfalls: null,
    },
  ],
} as unknown as SkillConsolidationProposal

describe('SkillConsolidationDialog', () => {
  it('renders the merge-plan header and the cluster row when open', () => {
    renderWithProviders(
      <SkillConsolidationDialog
        open
        proposal={proposal}
        onOpenChange={() => {}}
        onApplied={() => {}}
      />,
    )
    expect(screen.getByText('整合现有技能')).toBeInTheDocument()
    expect(screen.getByText('合并后技能')).toBeInTheDocument()
  })

  it('renders nothing when proposal is null', () => {
    const { container } = renderWithProviders(
      <SkillConsolidationDialog
        open
        proposal={null}
        onOpenChange={() => {}}
        onApplied={() => {}}
      />,
    )
    expect(container.querySelector('[role="dialog"]')).toBeNull()
  })
})
