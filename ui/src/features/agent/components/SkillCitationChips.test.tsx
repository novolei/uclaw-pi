import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { SkillCitationChips } from './SkillCitationChips'
import type { SkillCitation } from '@/lib/skill-citation'

// The dedup hook fires record_skill_cited through the agent bridge; stub it so
// the render test never reaches `@tauri-apps/api`.
const recordSkillCited = vi.fn().mockResolvedValue(null)
vi.mock('@/lib/bridge/agent', () => ({
  recordSkillCited: (...args: unknown[]) => recordSkillCited(...args),
}))

const CITATIONS: SkillCitation[] = [
  { title: 'stock-research', reason: 'user asked about Apple', raw: '> 应用技能：stock-research — user asked about Apple' },
]

describe('SkillCitationChips', () => {
  it('renders nothing when there are no citations', () => {
    const { container } = renderWithProviders(
      <SkillCitationChips citations={[]} messageKey="m1" />,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders a chip per citation', () => {
    const { getByText } = renderWithProviders(
      <SkillCitationChips citations={CITATIONS} messageKey="m1" />,
    )
    expect(getByText('stock-research')).toBeTruthy()
  })

  it('fires the best-effort record bump for each citation', () => {
    renderWithProviders(
      <SkillCitationChips citations={CITATIONS} messageKey="m-record" />,
    )
    expect(recordSkillCited).toHaveBeenCalledWith('stock-research')
  })
})
