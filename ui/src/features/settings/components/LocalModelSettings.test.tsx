import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderWithProviders, screen, waitFor, fireEvent } from '@/test-utils/render'
import type { LocalModelProgress } from '../../../lib/bridge/settings'
import { LocalModelSettings } from './LocalModelSettings'

// LocalModelSettings probes presence on mount + subscribes to the progress
// event. Mock the bridge so we control presence, capture the progress handler
// (to drive the source-aware progress bar), and assert the download/cancel
// wiring. Each test sets isLocalModelPresent before render.
const isPresent = vi.fn().mockResolvedValue(false)
const downloadLocalModel = vi.fn().mockResolvedValue('/data/MiniCPM5-1B-Q4_K_M.gguf')
const cancelLocalModelDownload = vi.fn().mockResolvedValue(undefined)
let progressHandler: ((p: LocalModelProgress) => void) | undefined

vi.mock('../../../lib/bridge/settings', () => ({
  settingsBridge: {
    isLocalModelPresent: (...a: unknown[]) => isPresent(...a),
    downloadLocalModel: (...a: unknown[]) => downloadLocalModel(...a),
    cancelLocalModelDownload: () => cancelLocalModelDownload(),
    probeDownloadSources: vi.fn().mockResolvedValue({ fastest: null, latencies: {} }),
  },
  onLocalModelDownloadProgress: (h: (p: LocalModelProgress) => void) => {
    progressHandler = h
    return Promise.resolve(() => {})
  },
}))

describe('LocalModelSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    isPresent.mockResolvedValue(false)
    progressHandler = undefined
    // Reset persisted quant so the default Q4_K_M is selected each test.
    localStorage.clear()
  })

  it('renders the quant selector and a download button when absent', async () => {
    renderWithProviders(<LocalModelSettings />)
    await waitFor(() => {
      expect(screen.getByText('Q4_K_M')).toBeTruthy()
      expect(screen.getByText('Q8_0')).toBeTruthy()
      expect(screen.getByText('F16')).toBeTruthy()
      expect(screen.getByRole("button", { name: /下载/ })).toBeTruthy()
    })
  })

  it('shows 已就绪 when the model is present', async () => {
    isPresent.mockResolvedValue(true)
    renderWithProviders(<LocalModelSettings />)
    await waitFor(() => expect(screen.getByText('已就绪')).toBeTruthy())
  })

  it('drives a source-aware progress bar + cancel from the progress event', async () => {
    renderWithProviders(<LocalModelSettings />)
    await waitFor(() => expect(screen.getByRole("button", { name: /下载/ })).toBeTruthy())

    // Start the download (registers the downloading status; the event subscription
    // already captured `progressHandler` on mount).
    fireEvent.click(screen.getByRole("button", { name: /下载/ }))
    await waitFor(() => expect(progressHandler).toBeTruthy())

    // Emit a downloading progress tick from "huggingface".
    progressHandler!({
      downloaded: 50_000_000,
      total: 100_000_000,
      source: 'huggingface',
      phase: 'downloading',
    })

    await waitFor(() => {
      expect(screen.getByText('HuggingFace')).toBeTruthy()
      expect(screen.getByText(/50%/)).toBeTruthy()
      expect(screen.getByLabelText('取消下载')).toBeTruthy()
    })

    // Cancel routes through the bridge.
    fireEvent.click(screen.getByLabelText('取消下载'))
    await waitFor(() => expect(cancelLocalModelDownload).toHaveBeenCalled())
  })

  it('shows the probing phase label before bytes flow', async () => {
    renderWithProviders(<LocalModelSettings />)
    await waitFor(() => expect(screen.getByRole("button", { name: /下载/ })).toBeTruthy())
    fireEvent.click(screen.getByRole("button", { name: /下载/ }))
    await waitFor(() => expect(progressHandler).toBeTruthy())
    progressHandler!({ downloaded: 0, total: 0, source: 'modelscope', phase: 'probing' })
    await waitFor(() => {
      expect(screen.getByText('测速中…')).toBeTruthy()
      expect(screen.getByText('ModelScope')).toBeTruthy()
    })
  })
})
