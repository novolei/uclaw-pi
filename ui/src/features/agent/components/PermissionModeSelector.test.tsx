import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { PermissionModeSelector } from './PermissionModeSelector'

const getSafetyPolicy = vi.fn()
const setSafetyMode = vi.fn()

vi.mock('@/lib/bridge/agent', () => ({
  getSafetyPolicy: () => getSafetyPolicy(),
  setSafetyMode: (input: unknown) => setSafetyMode(input),
}))

describe('PermissionModeSelector', () => {
  beforeEach(() => {
    getSafetyPolicy.mockReset().mockResolvedValue({ globalMode: 'supervised' })
    setSafetyMode.mockReset().mockResolvedValue({ globalMode: 'supervised' })
  })

  it('hydrates the current mode from the agent bridge on mount', async () => {
    renderWithProviders(<PermissionModeSelector sessionId="s-1" />)
    await waitFor(() => expect(getSafetyPolicy).toHaveBeenCalled())
    // Default Auto-mode label from the hydrated 'supervised' wire.
    expect(await screen.findByText('Auto mode')).toBeInTheDocument()
  })

  it('reflects a non-default hydrated mode (Plan)', async () => {
    getSafetyPolicy.mockResolvedValue({ globalMode: 'plan' })
    renderWithProviders(<PermissionModeSelector sessionId="s-1" />)
    expect(await screen.findByText('Plan mode')).toBeInTheDocument()
  })
})
