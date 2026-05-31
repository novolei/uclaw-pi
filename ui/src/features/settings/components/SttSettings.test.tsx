import { describe, it, expect, vi, beforeEach } from 'vitest'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { SttSettings } from './SttSettings'
import { createStore } from 'jotai'
import { modelStatusAtom, sttSettingsAtom } from '@/atoms/stt-atoms'

// The model-status probe + download now go through settingsBridge (via the
// useSttModel hook), so mock the bridge instead of raw @tauri-apps/api.
const statusMock = vi.fn()
const downloadMock = vi.fn()
vi.mock('../../../lib/bridge/settings', () => ({
  settingsBridge: {
    sttModelStatus: (...a: unknown[]) => statusMock(...a),
    sttDownloadModel: (...a: unknown[]) => downloadMock(...a),
  },
}))

beforeEach(() => {
  statusMock.mockReset()
  downloadMock.mockReset()
  // Default: stt_model_status never resolves (so we can control modelStatusAtom via store)
  statusMock.mockReturnValue(new Promise(() => {}))
})

describe('SttSettings', () => {
  it('renders model status section with "未下载" when not downloaded', () => {
    const store = createStore()
    store.set(modelStatusAtom, { kind: 'not-downloaded', expectedDir: '/tmp/x' })
    renderWithProviders(<SttSettings />, { store })
    expect(screen.getByText('未下载')).not.toBeNull()
  })

  it('renders default language select with "auto" selected', () => {
    const store = createStore()
    renderWithProviders(<SttSettings />, { store })
    // "自动" appears as the selected language option value
    expect(screen.getAllByText(/自动/).length).toBeGreaterThan(0)
  })

  it('shows shortcut hint linking to keyboard settings', () => {
    const store = createStore()
    renderWithProviders(<SttSettings />, { store })
    expect(screen.getByText(/Alt\+S|⌥S/)).not.toBeNull()
  })

  it('renders silence threshold select with default value', () => {
    const store = createStore()
    renderWithProviders(<SttSettings />, { store })
    expect(screen.getByText('1.8 秒（默认）')).not.toBeNull()
  })
})
