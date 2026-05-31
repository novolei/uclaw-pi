import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { AgentSettings } from './AgentSettings'

// Heavy sibling sections do their own IPC; mock them so this test exercises
// only AgentSettings' own presentation (matches the IntelligenceTab test pattern).
vi.mock('./PersonaStudio', () => ({
  PersonaStudio: () => <div data-testid="persona-studio" />,
}))
vi.mock('./PersonaBondTimeline', () => ({
  PersonaBondTimeline: () => <div data-testid="persona-bond-timeline" />,
}))
vi.mock('../../../lib/bridge/settings', () => ({
  settingsBridge: { setPlanModeSuggestEnabled: vi.fn().mockResolvedValue(undefined) },
}))

describe('AgentSettings', () => {
  it('renders the 行为设置 section with its toggles', () => {
    const { getByText } = renderWithProviders(<AgentSettings />)
    expect(getByText('行为设置')).toBeTruthy()
    expect(getByText('流式响应')).toBeTruthy()
    expect(getByText('为复杂任务建议 Plan 模式')).toBeTruthy()
  })
})
