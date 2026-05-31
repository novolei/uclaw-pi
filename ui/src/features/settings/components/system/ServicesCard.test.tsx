import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { ServicesCard } from './ServicesCard'

vi.mock('../../../../lib/bridge/settings', () => ({
  settingsBridge: {
    bridgeAction: vi.fn().mockResolvedValue(undefined),
  },
}))

describe('ServicesCard', () => {
  it('renders the 恢复操作 actions', () => {
    const { getByText } = renderWithProviders(<ServicesCard />)
    expect(getByText('恢复操作')).toBeTruthy()
    expect(getByText('重置 AI 引擎')).toBeTruthy()
    expect(getByText('重启 memU')).toBeTruthy()
    expect(getByText('重启 gbrain')).toBeTruthy()
  })
})
