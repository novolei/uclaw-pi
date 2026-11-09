import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act } from '@testing-library/react'
import { renderWithProviders, screen, waitFor, within } from '@/test-utils/render'
import type { MemoryCoreStatus } from '@/lib/bridge/memory-core'
import { memoryCoreFixture, type MemoryCoreMockState } from '@/lib/dev-memory-core-mock'
import { MemoryEngineCard } from './MemoryEngineCard'

const h = vi.hoisted(() => ({
  invoke: vi.fn(),
  listeners: new Map<string, (e: { payload: unknown }) => void>(),
  toastError: vi.fn(),
}))

vi.mock('@tauri-apps/api/core', () => ({ invoke: h.invoke }))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, handler: (e: { payload: unknown }) => void) => {
    h.listeners.set(event, handler)
    return () => h.listeners.delete(event)
  }),
}))
vi.mock('sonner', () => ({ toast: { error: h.toastError, success: vi.fn() } }))

function serve(status: MemoryCoreStatus, overrides: Record<string, unknown> = {}) {
  h.invoke.mockImplementation(async (cmd: string, args?: { enabled?: boolean }) => {
    if (cmd in overrides) {
      const o = overrides[cmd]
      return typeof o === 'function' ? (o as (a: unknown) => unknown)(args) : o
    }
    if (cmd === 'memory_core_get_status') return status
    throw new Error(`unstubbed ${cmd}`)
  })
}

async function renderState(state: MemoryCoreMockState, seq = 5) {
  serve(memoryCoreFixture(state, seq))
  const r = renderWithProviders(<MemoryEngineCard />)
  await screen.findByRole('switch', { name: '使用本地检索模型' })
  return r
}

function pushEvent(status: MemoryCoreStatus) {
  const fn = h.listeners.get('memory-core:status')
  if (!fn) throw new Error('no memory-core:status listener')
  act(() => fn({ payload: status }))
}

beforeEach(() => {
  h.invoke.mockReset()
  h.listeners.clear()
  h.toastError.mockReset()
})

describe('MemoryEngineCard', () => {
  it('shows a skeleton (aria-busy, no text) while the first status loads', async () => {
    h.invoke.mockImplementation(() => new Promise(() => {}))
    renderWithProviders(<MemoryEngineCard />)
    const sk = await screen.findByTestId('memory-engine-skeleton', {}, { timeout: 1000 })
    expect(sk).toHaveAttribute('aria-busy', 'true')
    expect(sk).toHaveAttribute('role', 'status')
    expect(screen.queryByText(/加载中/)).toBeNull()
  })

  it('ready: hybrid mode, both models ready, index stats + relative sync time', async () => {
    await renderState('ready')
    expect(screen.getByText('混合检索')).toBeInTheDocument()
    expect(screen.getByRole('switch', { name: '使用本地检索模型' })).toBeChecked()
    expect(screen.getByText('语义理解模型')).toBeInTheDocument()
    expect(screen.getByText('精排模型')).toBeInTheDocument()
    expect(screen.getAllByText('已就绪').length).toBeGreaterThanOrEqual(2)
    expect(screen.getByRole('progressbar', { name: '语义覆盖' })).toHaveAttribute('aria-valuenow', '100')
    expect(screen.getByText(/上次同步 3 分钟前/)).toBeInTheDocument()
    // the coverage row carries the count; no duplicate 记忆 pill
    expect(screen.getByText(/1,284 \/ 1,284 条/)).toBeInTheDocument()
    expect(screen.queryByText('记忆', { selector: 'span' })).toBeNull()
  })

  it('downloading: overall progress with bytes and friendly source', async () => {
    await renderState('downloading')
    const bar = screen.getByRole('progressbar', { name: '下载记忆模型' })
    expect(Number(bar.getAttribute('aria-valuenow'))).toBe(36)
    expect(screen.getByText(/412 MB \/ 1\.14 GB/)).toBeInTheDocument()
    expect(screen.getByText(/来自 镜像站/)).toBeInTheDocument()
    expect(screen.getByText('仅关键词')).toBeInTheDocument()
    // per-model rows
    expect(screen.getByText('语义理解模型')).toBeInTheDocument()
    expect(screen.getByText('等待中')).toBeInTheDocument()
  })

  it('failed: danger callout with the reason and a working 重试', async () => {
    const { user } = await renderState('failed')
    const alert = screen.getByRole('alert')
    expect(within(alert).getByText('模型下载失败')).toBeInTheDocument()
    expect(within(alert).getByText(/请检查网络后重试。目前仍可按关键词检索/)).toBeInTheDocument()
    // raw error sits behind 详情, untruncated
    expect(within(alert).queryByText(/operation timed out/)).toBeNull()
    await user.click(within(alert).getByRole('button', { name: '详情' }))
    expect(within(alert).getByText(/https:\/\/hf-mirror\.com\/Xenova\/bge-m3.*operation timed out/)).toBeInTheDocument()
    h.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'memory_core_retry_download') return memoryCoreFixture('downloading', 9)
      throw new Error(cmd)
    })
    await user.click(within(alert).getByRole('button', { name: '重试' }))
    expect(h.invoke).toHaveBeenCalledWith('memory_core_retry_download')
    expect(await screen.findByRole('progressbar', { name: '下载记忆模型' })).toBeInTheDocument()
  })

  it('load failed: calm callout, raw error behind 详情, 重试 re-runs the load', async () => {
    const { user } = await renderState('loadfailed', 5)
    expect(screen.queryByText('正在加载模型')).toBeNull()
    const alert = screen.getByRole('alert')
    expect(within(alert).getByText('模型加载失败')).toBeInTheDocument()
    expect(within(alert).getByText('本地模型未能加载，目前仍可按关键词检索。')).toBeInTheDocument()
    expect(within(alert).queryByText(/Protobuf parsing failed/)).toBeNull()
    await user.click(within(alert).getByRole('button', { name: '详情' }))
    expect(within(alert).getByText(/Protobuf parsing failed/)).toBeInTheDocument()
    h.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'memory_core_retry_download') return { ...memoryCoreFixture('loading', 6) }
      throw new Error(cmd)
    })
    await user.click(within(alert).getByRole('button', { name: '重试' }))
    expect(h.invoke).toHaveBeenCalledWith('memory_core_retry_download')
    expect(await screen.findByText('正在加载模型')).toBeInTheDocument()
  })

  it('off with the models already on disk: no download promise', async () => {
    await renderState('offondisk')
    expect(screen.getByText(/开启后立即可用，模型已在本机/)).toBeInTheDocument()
    expect(screen.queryByText(/首次开启会下载/)).toBeNull()
  })

  it('off: keyword-only explanation and the first-download size', async () => {
    await renderState('off')
    expect(screen.getByRole('switch', { name: '使用本地检索模型' })).not.toBeChecked()
    expect(screen.getByText('仅关键词')).toBeInTheDocument()
    expect(screen.getByText(/首次开启会下载约 1\.1 GB/)).toBeInTheDocument()
    expect(screen.queryByRole('progressbar', { name: '语义覆盖' })).toBeNull()
    expect(screen.getByText('开启本地模型后，将为 1,284 条记忆建立语义索引')).toBeInTheDocument()
  })

  it('empty index: first-run empty state', async () => {
    await renderState('empty')
    expect(screen.getByText('还没有可检索的记忆')).toBeInTheDocument()
    expect(screen.getByText(/尚未同步/)).toBeInTheDocument()
  })

  it('loading models: files ready but embedder not yet loaded', async () => {
    await renderState('loading')
    expect(screen.getByText('正在加载模型')).toBeInTheDocument()
    expect(screen.getByRole('progressbar', { name: '加载记忆模型' })).toHaveAttribute(
      'data-state',
      'indeterminate',
    )
  })

  it('toggling calls memory_core_set_models_enabled with the new value (optimistic)', async () => {
    const { user } = await renderState('off')
    let resolve!: (s: MemoryCoreStatus) => void
    h.invoke.mockImplementation((cmd: string) =>
      cmd === 'memory_core_set_models_enabled'
        ? new Promise((r) => { resolve = r })
        : Promise.reject(new Error(cmd)),
    )
    const sw = screen.getByRole('switch', { name: '使用本地检索模型' })
    await user.click(sw)
    expect(h.invoke).toHaveBeenCalledWith('memory_core_set_models_enabled', { enabled: true })
    expect(sw).toBeChecked() // optimistic
    await act(async () => resolve(memoryCoreFixture('downloading', 9)))
    expect(await screen.findByRole('progressbar', { name: '下载记忆模型' })).toBeInTheDocument()
  })

  it('keeps the optimistic toggle while older-state progress events arrive mid-call', async () => {
    const { user } = await renderState('off', 5)
    let resolve!: (s: MemoryCoreStatus) => void
    h.invoke.mockImplementation((cmd: string) =>
      cmd === 'memory_core_set_models_enabled'
        ? new Promise((r) => { resolve = r })
        : Promise.reject(new Error(cmd)),
    )
    const sw = screen.getByRole('switch', { name: '使用本地检索模型' })
    await user.click(sw)
    // backend still reports modelsEnabled=false (persist not done) on a newer seq
    pushEvent(memoryCoreFixture('off', 6))
    expect(sw).toBeChecked()
    await act(async () => resolve(memoryCoreFixture('downloading', 7)))
    expect(sw).toBeChecked()
    // once settled, pushed events are taken at face value again
    pushEvent(memoryCoreFixture('off', 8))
    expect(sw).not.toBeChecked()
  })

  it('shares one get_status call and one listener across consumers', async () => {
    serve(memoryCoreFixture('ready', 3))
    const { listen } = await import('@tauri-apps/api/event')
    vi.mocked(listen).mockClear()
    renderWithProviders(
      <>
        <MemoryEngineCard />
        <MemoryEngineCard />
      </>,
    )
    await screen.findAllByRole('switch', { name: '使用本地检索模型' })
    expect(h.invoke.mock.calls.filter(([c]) => c === 'memory_core_get_status')).toHaveLength(1)
    expect(vi.mocked(listen)).toHaveBeenCalledTimes(1)
  })

  it('rolls back the toggle and toasts when the command fails', async () => {
    const { user } = await renderState('ready')
    h.invoke.mockImplementation(async () => {
      throw new Error('boom')
    })
    const sw = screen.getByRole('switch', { name: '使用本地检索模型' })
    await user.click(sw)
    await waitFor(() => expect(h.toastError).toHaveBeenCalled())
    expect(sw).toBeChecked()
  })

  it('applies newer events and ignores stale ones (seq ≤ last applied)', async () => {
    await renderState('ready', 5)
    pushEvent(memoryCoreFixture('off', 4)) // stale
    expect(screen.getByRole('switch', { name: '使用本地检索模型' })).toBeChecked()
    pushEvent(memoryCoreFixture('off', 5)) // equal → stale too
    expect(screen.getByRole('switch', { name: '使用本地检索模型' })).toBeChecked()
    pushEvent(memoryCoreFixture('off', 6))
    expect(screen.getByRole('switch', { name: '使用本地检索模型' })).not.toBeChecked()
  })

  it('initial load failure: inline error with a retry that re-fetches', async () => {
    h.invoke.mockRejectedValueOnce(new Error('ipc down'))
    const { user } = renderWithProviders(<MemoryEngineCard />)
    const alert = await screen.findByRole('alert')
    expect(within(alert).getByText('无法读取检索引擎状态')).toBeInTheDocument()
    serve(memoryCoreFixture('ready', 2))
    await user.click(within(alert).getByRole('button', { name: '重试' }))
    expect(await screen.findByText('混合检索')).toBeInTheDocument()
  })
})
