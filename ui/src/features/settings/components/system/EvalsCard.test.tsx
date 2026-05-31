import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { EvalsCard } from './EvalsCard'

vi.mock('../../../../lib/bridge/settings', () => ({
  settingsBridge: {
    runEval: vi.fn().mockResolvedValue({ passed: true, averageScore: 1, runIds: [], scorecards: [] }),
  },
}))

describe('EvalsCard', () => {
  it('renders the 评估套件 controls + not-yet-run prompt', () => {
    const { getByText, getByRole } = renderWithProviders(<EvalsCard />)
    expect(getByText('评估套件')).toBeTruthy()
    expect(getByText('自治回归套件')).toBeTruthy()
    expect(getByRole('button', { name: /All/ })).toBeTruthy()
    expect(getByText(/尚未运行/)).toBeTruthy()
  })
})
