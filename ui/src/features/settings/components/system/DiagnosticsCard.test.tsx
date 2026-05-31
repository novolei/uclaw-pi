import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { DiagnosticsCard } from './DiagnosticsCard'

vi.mock('../../../../lib/bridge/settings', () => ({
  settingsBridge: {
    getSystemDiagnostics: vi.fn().mockResolvedValue(null),
  },
}))

describe('DiagnosticsCard', () => {
  it('renders the 系统诊断 header + 运行诊断 control + empty state', () => {
    const { container, getByText } = renderWithProviders(<DiagnosticsCard />)
    expect(getByText('系统诊断')).toBeTruthy()
    expect(container.querySelector('button')?.textContent).toContain('运行诊断')
    // No report fetched yet → empty-state prompt.
    expect(getByText('点击「运行诊断」开始检查系统状态')).toBeTruthy()
  })
})
