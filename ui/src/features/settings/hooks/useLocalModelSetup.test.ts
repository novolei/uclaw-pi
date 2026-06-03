import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act, waitFor } from '@testing-library/react'
import type { EnvReport } from '../../../lib/bridge/settings'
import { useLocalModelSetup } from './useLocalModelSetup'

// useLocalModelSetup drives the first-launch state machine through the bridge.
// Mock the bridge so we control the env report + record the call ORDER
// (check → download → warmup → assign), and assert the disk-fail → blocked and
// skip → flag transitions.
const calls: string[] = []
const checkLocalModelEnvironment = vi.fn()
const downloadLocalModel = vi.fn()
const warmupLocalModel = vi.fn()
const assignLocalModelToRoles = vi.fn()

const okReport: EnvReport = {
  diskFreeBytes: 50_000_000_000,
  diskOk: true,
  diskRequiredBytes: 1_000_000_000,
  ramTotalBytes: 16_000_000_000,
  ramAvailableBytes: 8_000_000_000,
  ramOk: true,
  metalAvailable: true,
  network: {
    modelscopeReachable: true,
    huggingfaceReachable: true,
    fastest: 'modelscope',
    anyReachable: true,
  },
}

vi.mock('../../../lib/bridge/settings', () => ({
  settingsBridge: {
    checkLocalModelEnvironment: (...a: unknown[]) => {
      calls.push('check')
      return checkLocalModelEnvironment(...a)
    },
    downloadLocalModel: (...a: unknown[]) => {
      calls.push('download')
      return downloadLocalModel(...a)
    },
    warmupLocalModel: () => {
      calls.push('warmup')
      return warmupLocalModel()
    },
    assignLocalModelToRoles: () => {
      calls.push('assign')
      return assignLocalModelToRoles()
    },
  },
  onLocalModelDownloadProgress: () => Promise.resolve(() => {}),
}))

describe('useLocalModelSetup', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    calls.length = 0
    checkLocalModelEnvironment.mockResolvedValue(okReport)
    downloadLocalModel.mockResolvedValue('/data/MiniCPM5-1B-Q4_K_M.gguf')
    warmupLocalModel.mockResolvedValue(undefined)
    assignLocalModelToRoles.mockResolvedValue(undefined)
    localStorage.clear()
  })

  it('starts in intro', () => {
    const { result } = renderHook(() => useLocalModelSetup())
    expect(result.current.phase).toBe('intro')
    expect(result.current.report).toBeNull()
  })

  it('runChecks → report when disk is ok', async () => {
    const { result } = renderHook(() => useLocalModelSetup())
    await act(async () => {
      await result.current.runChecks()
    })
    expect(result.current.phase).toBe('report')
    expect(result.current.report?.diskOk).toBe(true)
  })

  it('disk-fail → blocked', async () => {
    checkLocalModelEnvironment.mockResolvedValue({ ...okReport, diskOk: false })
    const { result } = renderHook(() => useLocalModelSetup())
    await act(async () => {
      await result.current.runChecks()
    })
    expect(result.current.phase).toBe('blocked')
  })

  it('downloadAndEnable runs check→download→warmup→assign in order and ends done', async () => {
    const { result } = renderHook(() => useLocalModelSetup())
    await act(async () => {
      await result.current.runChecks()
    })
    await act(async () => {
      await result.current.downloadAndEnable()
    })
    expect(calls).toEqual(['check', 'download', 'warmup', 'assign'])
    expect(result.current.phase).toBe('done')
  })

  it('does not download when blocked', async () => {
    checkLocalModelEnvironment.mockResolvedValue({ ...okReport, diskOk: false })
    const { result } = renderHook(() => useLocalModelSetup())
    await act(async () => {
      await result.current.runChecks()
    })
    await act(async () => {
      await result.current.downloadAndEnable()
    })
    expect(downloadLocalModel).not.toHaveBeenCalled()
    expect(result.current.phase).toBe('blocked')
  })

  it('skip → skipped', () => {
    const { result } = renderHook(() => useLocalModelSetup())
    act(() => {
      result.current.skip()
    })
    expect(result.current.phase).toBe('skipped')
  })

  it('surfaces a download error back to report', async () => {
    downloadLocalModel.mockRejectedValue(new Error('network down'))
    const { result } = renderHook(() => useLocalModelSetup())
    await act(async () => {
      await result.current.runChecks()
    })
    await act(async () => {
      await result.current.downloadAndEnable()
    })
    await waitFor(() => expect(result.current.phase).toBe('report'))
    expect(result.current.error).toContain('network down')
  })
})
