import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { SystemTab } from './SystemTab'

vi.mock('../../../lib/bridge/settings', () => ({
  settingsBridge: {
    getHttpApiEnabled: vi.fn().mockResolvedValue(false),
    setHttpApiEnabled: vi.fn().mockResolvedValue(undefined),
    getSystemDiagnostics: vi.fn().mockResolvedValue(null),
    runEval: vi.fn(),
    bridgeAction: vi.fn(),
  },
}))

describe('SystemTab', () => {
  it('renders the 系统诊断 shell with its cards', () => {
    const { container, getByText } = renderWithProviders(<SystemTab />)
    expect(container.querySelector('[data-settings-section="系统诊断"]')).toBeTruthy()
    // Each of the four cards' headline marker renders.
    expect(getByText('系统诊断')).toBeTruthy()
    expect(getByText(/本地 HTTP API 服务/)).toBeTruthy()
    expect(getByText('评估套件')).toBeTruthy()
    expect(getByText('恢复操作')).toBeTruthy()
  })
})
