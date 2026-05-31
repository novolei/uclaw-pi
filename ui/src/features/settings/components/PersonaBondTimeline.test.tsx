import { describe, expect, it, vi } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import * as personaApi from '@/lib/persona'
import type { PersonaRelationshipTimeline } from '@/lib/persona-types'
import { PersonaBondTimeline } from './PersonaBondTimeline'

vi.mock('@/lib/persona', () => ({
  createPersonaJournalEntry: vi.fn(),
  deletePersonaJournalEntry: vi.fn(),
  getPersonaRelationshipTimeline: vi.fn(),
  promotePersonaJournalEntry: vi.fn(),
  updatePersonaBadgeVisibility: vi.fn(),
  updatePersonaKeepsakeStatus: vi.fn(),
  updatePersonaRelationshipSettings: vi.fn(),
}))

const timeline: PersonaRelationshipTimeline = {
  affinity: {
    score: 18,
    explanation: ['+2 accepted keepsakes'],
  },
  factors: {
    successfulMinutes: 120,
    acceptedKeepsakes: 2,
    positiveFeedback: 0,
    stableStyleFragments: 0,
    recoveredFailures: 0,
    inactivityDays: 0,
    rejectedCandidates: 0,
    unresolvedFailures: 0,
    correctionCount: 0,
  },
  bond: {
    collaborationRhythm: ['Start with a plan'],
    challengeContract: [],
    supportStyle: ['Be warm'],
    communicationDislikes: ['No hollow praise'],
  },
  journalEntries: [
    {
      id: 'journal_1',
      sessionId: 's1',
      taskId: null,
      observation: 'User likes crisp plans.',
      interpretation: 'Start risky work with a short plan.',
      confidence: 'high',
      promotedAt: null,
      createdAt: '2026-05-27T00:00:00Z',
    },
  ],
  keepsakes: [
    {
      id: 'ks_1',
      title: '第一次顺利合并',
      narrative: '我们一起把人格 MVP 合并进 main。',
      learnedText: '用户偏好先收敛 MVP，再拆后续增强。',
      evidence: ['pr:545'],
      status: 'proposed',
    },
  ],
  badges: [
    {
      id: 'badge_1',
      badgeKey: 'first_keepsake',
      label: '第一张纪念物',
      unlockReason: '确认了第一张共同经历卡。',
      evidence: ['factor:accepted_keepsakes'],
      hidden: false,
      awardedAt: '2026-05-27T00:00:00Z',
    },
  ],
  recentEvents: [],
  settings: {
    gamificationEnabled: true,
  },
}

describe('PersonaBondTimeline', () => {
  it('loads relationship timeline and updates keepsakes, journal, badges, and settings', async () => {
    vi.mocked(personaApi.getPersonaRelationshipTimeline).mockResolvedValueOnce(timeline)
    vi.mocked(personaApi.updatePersonaKeepsakeStatus).mockResolvedValueOnce({
      ...timeline,
      affinity: { score: 24, explanation: ['+3 accepted keepsakes'] },
      factors: { ...timeline.factors, acceptedKeepsakes: 3 },
      keepsakes: [{ ...timeline.keepsakes[0], status: 'accepted' }],
    })
    vi.mocked(personaApi.promotePersonaJournalEntry).mockResolvedValueOnce({
      ...timeline,
      bond: {
        ...timeline.bond,
        supportStyle: [...timeline.bond.supportStyle, 'Start risky work with a short plan.'],
      },
      journalEntries: [
        { ...timeline.journalEntries[0], promotedAt: '2026-05-27T00:01:00Z' },
      ],
    })
    vi.mocked(personaApi.updatePersonaBadgeVisibility).mockResolvedValueOnce({
      ...timeline,
      badges: [{ ...timeline.badges[0], hidden: true }],
    })
    vi.mocked(personaApi.updatePersonaRelationshipSettings).mockResolvedValueOnce({
      ...timeline,
      settings: { gamificationEnabled: false },
    })

    const { user } = renderWithProviders(<PersonaBondTimeline />)

    expect(screen.getByText('关系时间线')).toBeInTheDocument()
    expect(screen.getByText(/不改变 Agent 能力/)).toBeInTheDocument()
    await waitFor(() => expect(screen.getByText('第一次顺利合并')).toBeInTheDocument())
    expect(screen.getByText('18')).toBeInTheDocument()
    expect(screen.getByText('Start with a plan')).toBeInTheDocument()
    expect(screen.getByText('User likes crisp plans.')).toBeInTheDocument()
    expect(screen.getByText('第一张纪念物')).toBeInTheDocument()
    expect(screen.getByText('勋章')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: /接受/ }))

    expect(personaApi.updatePersonaKeepsakeStatus).toHaveBeenCalledWith({
      id: 'ks_1',
      status: 'accepted',
    })
    await waitFor(() => expect(screen.getByText('24')).toBeInTheDocument())

    await user.click(screen.getByRole('button', { name: /提升为支持风格/ }))
    expect(personaApi.promotePersonaJournalEntry).toHaveBeenCalledWith({
      id: 'journal_1',
      field: 'support_style',
    })

    await user.click(screen.getByRole('button', { name: /隐藏勋章/ }))
    expect(personaApi.updatePersonaBadgeVisibility).toHaveBeenCalledWith({
      badgeKey: 'first_keepsake',
      hidden: true,
    })

    await user.click(screen.getByRole('switch'))
    expect(personaApi.updatePersonaRelationshipSettings).toHaveBeenCalledWith({
      gamificationEnabled: false,
    })
  })
})
