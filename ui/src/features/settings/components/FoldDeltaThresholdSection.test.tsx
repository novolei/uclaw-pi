import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen, waitFor } from '@/test-utils/render'
import { FoldDeltaThresholdSection } from './FoldDeltaThresholdSection'

// The section loads its threshold on mount via @/lib/fold-delta-threshold (a
// Bundle-17-B domain helper — not settings-domain IPC, so it stays there rather
// than routing through settingsBridge). Mock it so the field populates.
vi.mock('@/lib/fold-delta-threshold', () => ({
  FOLD_DELTA_THRESHOLD_DEFAULT: 50,
  FOLD_DELTA_THRESHOLD_MIN: 1,
  FOLD_DELTA_THRESHOLD_MAX: 200,
  getFoldDeltaThreshold: vi.fn().mockResolvedValue(42),
  setFoldDeltaThreshold: vi.fn().mockResolvedValue(undefined),
}))

describe('FoldDeltaThresholdSection', () => {
  it('renders the /compact 折叠 delta 阈值 card + loads the threshold from the helper', async () => {
    renderWithProviders(<FoldDeltaThresholdSection />)
    expect(screen.getByText(/折叠 delta 阈值/)).toBeTruthy()
    // After the mount load resolves, the loaded value (42) populates the field.
    await waitFor(() => expect(screen.getByDisplayValue('42')).toBeTruthy())
  })
})
