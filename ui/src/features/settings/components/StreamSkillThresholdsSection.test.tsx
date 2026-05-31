import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { StreamSkillThresholdsSection } from './StreamSkillThresholdsSection'

// The section loads its thresholds on mount via @/lib/stream-skill-thresholds (a
// Bundle-26/27 domain helper — not settings-domain IPC, so it stays there rather
// than routing through settingsBridge). Mock it so the fields populate.
vi.mock('@/lib/stream-skill-thresholds', () => ({
  STREAM_SKILL_DEFAULTS: {
    stream_idle_timeout_secs: 90,
    skill_prune_min_unused_days: 30,
    skill_promote_min_returned_count: 3,
  },
  getStreamSkillThresholds: vi.fn().mockResolvedValue({
    stream_idle_timeout_secs: 75,
    skill_prune_min_unused_days: 30,
    skill_promote_min_returned_count: 3,
  }),
  setStreamIdleTimeoutSecs: vi.fn().mockResolvedValue(undefined),
  setSkillPruneMinUnusedDays: vi.fn().mockResolvedValue(undefined),
  setSkillPromoteMinReturnedCount: vi.fn().mockResolvedValue(undefined),
}))

describe('StreamSkillThresholdsSection', () => {
  it('renders the 流式与技能蒸馏阈值 card + loads thresholds from the helper', async () => {
    renderWithProviders(<StreamSkillThresholdsSection />)
    expect(screen.getByText(/流式与技能蒸馏阈值/)).toBeTruthy()
    // After the mount load resolves, the loaded idle-timeout (75) populates the field.
    await waitFor(() => expect(screen.getByDisplayValue('75')).toBeTruthy())
  })
})
