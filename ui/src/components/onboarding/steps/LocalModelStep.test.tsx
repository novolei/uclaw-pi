import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders, screen, fireEvent } from '@/test-utils/render'
import type { EnvReport } from '@/lib/bridge/settings'
import type { UseLocalModelSetup } from '@/features/settings/hooks/useLocalModelSetup'
import { LocalModelStep } from './LocalModelStep'

// The component still calls the real `useLocalModelSetup` (hooks can't be
// conditional), which subscribes to the progress event on mount. Stub the
// bridge module so that subscription is a no-op — every assertion drives the
// component through an injected `setup` prop instead.
vi.mock('@/lib/bridge/settings', () => ({
  settingsBridge: {
    checkLocalModelEnvironment: vi.fn().mockResolvedValue(undefined),
    downloadLocalModel: vi.fn().mockResolvedValue(''),
    warmupLocalModel: vi.fn().mockResolvedValue(undefined),
    assignLocalModelToRoles: vi.fn().mockResolvedValue(undefined),
  },
  onLocalModelDownloadProgress: () => Promise.resolve(() => {}),
}))

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

function makeSetup(overrides: Partial<UseLocalModelSetup> = {}): UseLocalModelSetup {
  return {
    phase: 'report',
    report: okReport,
    progress: null,
    error: null,
    runChecks: vi.fn().mockResolvedValue(undefined),
    downloadAndEnable: vi.fn().mockResolvedValue(undefined),
    skip: vi.fn(),
    reset: vi.fn(),
    ...overrides,
  }
}

describe('LocalModelStep', () => {
  it('renders the env checklist from the report', () => {
    renderWithProviders(<LocalModelStep setup={makeSetup()} />)
    expect(screen.getByText('磁盘空间')).toBeTruthy()
    expect(screen.getByText('内存')).toBeTruthy()
    expect(screen.getByText('GPU 加速')).toBeTruthy()
    expect(screen.getByText('网络')).toBeTruthy()
    expect(screen.getByRole('button', { name: '下载并启用' })).toBeTruthy()
    expect(screen.getByRole('button', { name: '跳过' })).toBeTruthy()
  })

  it('shows CPU guidance when Metal is unavailable', () => {
    renderWithProviders(<LocalModelStep setup={makeSetup({ report: { ...okReport, metalAvailable: false } })} />)
    expect(screen.getByText('未检测到 Metal，将用 CPU（较慢）')).toBeTruthy()
  })

  it('blocked state disables download', () => {
    renderWithProviders(
      <LocalModelStep setup={makeSetup({ phase: 'blocked', report: { ...okReport, diskOk: false } })} />,
    )
    const download = screen.getByRole('button', { name: '下载并启用' }) as HTMLButtonElement
    expect(download.disabled).toBe(true)
    expect(screen.getByText(/磁盘空间不足/)).toBeTruthy()
  })

  it('happy path: clicking 下载并启用 calls the orchestrator', () => {
    const setup = makeSetup()
    renderWithProviders(<LocalModelStep setup={setup} />)
    fireEvent.click(screen.getByRole('button', { name: '下载并启用' }))
    expect(setup.downloadAndEnable).toHaveBeenCalled()
  })

  it('skip calls the orchestrator', () => {
    const setup = makeSetup()
    renderWithProviders(<LocalModelStep setup={setup} />)
    fireEvent.click(screen.getByRole('button', { name: '跳过' }))
    expect(setup.skip).toHaveBeenCalled()
  })

  it('done state notifies onSettled and hides actions', () => {
    const onSettled = vi.fn()
    renderWithProviders(<LocalModelStep setup={makeSetup({ phase: 'done' })} onSettled={onSettled} />)
    expect(onSettled).toHaveBeenCalledWith('done')
    expect(screen.getByText('已下载并启用本地模型')).toBeTruthy()
    expect(screen.queryByRole('button', { name: '下载并启用' })).toBeNull()
  })

  it('renders a progress bar while downloading', () => {
    renderWithProviders(
      <LocalModelStep
        setup={makeSetup({
          phase: 'downloading',
          progress: { phase: 'downloading', source: 'huggingface', downloaded: 50, total: 100, percent: 50 },
        })}
      />,
    )
    expect(screen.getByText('HuggingFace')).toBeTruthy()
    expect(screen.getByText('50%')).toBeTruthy()
  })
})
