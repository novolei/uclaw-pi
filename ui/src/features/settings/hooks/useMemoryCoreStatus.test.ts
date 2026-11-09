import { describe, it, expect } from 'vitest'
import {
  createMemoryCoreMock,
  memoryCoreFixture,
  parseMemoryCoreMockState,
} from '@/lib/dev-memory-core-mock'
import {
  describeDownloadError,
  formatBytes,
  formatSince,
} from '../components/memory/MemoryEngineCard'
import {
  acceptMemoryCoreStatus,
  memoryCoreAttention,
  memoryEngineState,
} from './useMemoryCoreStatus'

describe('useMemoryCoreStatus helpers', () => {
  it('seq guard: events must be strictly newer; command responses may be equal', () => {
    const a = memoryCoreFixture('ready', 5)
    expect(acceptMemoryCoreStatus(null, a)).toBe(a)
    expect(acceptMemoryCoreStatus(a, memoryCoreFixture('off', 4))).toBe(a)
    expect(acceptMemoryCoreStatus(a, memoryCoreFixture('off', 5))).toBe(a)
    const eq = memoryCoreFixture('off', 5)
    expect(acceptMemoryCoreStatus(a, eq, true)).toBe(eq)
    const newer = memoryCoreFixture('off', 6)
    expect(acceptMemoryCoreStatus(a, newer)).toBe(newer)
  })

  it('derives the engine state and the nav attention', () => {
    expect(memoryEngineState(memoryCoreFixture('ready'))).toBe('ready')
    expect(memoryEngineState(memoryCoreFixture('off'))).toBe('off')
    expect(memoryEngineState(memoryCoreFixture('failed'))).toBe('failed')
    expect(memoryEngineState(memoryCoreFixture('downloading'))).toBe('downloading')
    expect(memoryEngineState(memoryCoreFixture('loading'))).toBe('loading')
    expect(memoryEngineState(memoryCoreFixture('loadfailed'))).toBe('loadFailed')
    expect(memoryEngineState(memoryCoreFixture('offondisk'))).toBe('off')
    // "loading" claims the files are on disk: before the first download status
    // (models not on disk yet) it reads as "preparing" instead.
    expect(memoryEngineState({ ...memoryCoreFixture('loading'), download: null, modelsOnDisk: false })).toBe('queued')
    // a stale load error never hides a loaded embedder
    expect(memoryEngineState({ ...memoryCoreFixture('ready'), loadError: 'x' })).toBe('ready')
    expect(memoryCoreAttention(memoryCoreFixture('loadfailed'))).toBe('failed')
    expect(memoryCoreAttention(null)).toBe('none')
    expect(memoryCoreAttention(memoryCoreFixture('ready'))).toBe('none')
    expect(memoryCoreAttention(memoryCoreFixture('loading'))).toBe('none') // phase done
    expect(memoryCoreAttention(memoryCoreFixture('downloading'))).toBe('active')
    expect(memoryCoreAttention(memoryCoreFixture('failed'))).toBe('failed')
    // a stale download record never lights the dot once models are off
    expect(memoryCoreAttention({ ...memoryCoreFixture('failed'), modelsEnabled: false })).toBe('none')
  })

  it('formats bytes and relative sync time', () => {
    expect(formatBytes(568_456_694)).toBe('568 MB')
    expect(formatBytes(1_139_183_788)).toBe('1.14 GB')
    const now = Date.parse('2026-11-09T10:00:00Z')
    expect(formatSince(null, now)).toBe('尚未同步')
    expect(formatSince('2026-11-09T09:59:30Z', now)).toBe('刚刚同步')
    expect(formatSince('2026-11-09T09:57:00Z', now)).toBe('上次同步 3 分钟前')
    expect(formatSince('2026-11-09T07:00:00Z', now)).toBe('上次同步 3 小时前')
    expect(formatSince('2026-11-07T10:00:00Z', now)).toBe('上次同步 2 天前')
  })

  it('classifies download errors into an actionable cause', () => {
    expect(describeDownloadError('error sending request for url (...): operation timed out')).toMatch(/检查网络/)
    expect(describeDownloadError('write onnx: No space left on device (os error 28)')).toMatch(/磁盘空间不足/)
    expect(describeDownloadError('Permission denied (os error 13)')).toMatch(/权限/)
    expect(describeDownloadError('checksum mismatch')).toBe('下载未完成，请稍后重试。')
    expect(describeDownloadError(null)).toBe('下载未完成，请稍后重试。')
  })

  it('mock URL flag picks the state, defaulting to ready', () => {
    expect(parseMemoryCoreMockState('?memoryCore=failed')).toBe('failed')
    expect(parseMemoryCoreMockState('?memoryCore=loadfailed')).toBe('loadfailed')
    expect(parseMemoryCoreMockState('?memoryCore=nope')).toBe('ready')
    expect(parseMemoryCoreMockState('')).toBe('ready')
  })

  it('mock retry acts only on a failed download or a failed load', async () => {
    const emitted: number[] = []
    const ready = createMemoryCoreMock('ready', (st) => void emitted.push(st.seq))
    const before = ready.getStatus()
    expect(ready.retryDownload()).toEqual(before) // nothing failed: unchanged
    const lf = createMemoryCoreMock('loadfailed', () => {})
    lf.getStatus()
    const after = lf.retryDownload()
    expect(after.loadError).toBeNull()
    expect(memoryEngineState(after)).toBe('loading')
  })

  it('mock off recomputes pending like the backend (docs lacking any vector)', () => {
    const m = createMemoryCoreMock('ready', () => {})
    m.getStatus()
    const off = m.setModelsEnabled(false)
    expect(off.index.pendingEmbeddings).toBe(off.index.docs - off.index.vectors)
    expect(off.modelsOnDisk).toBe(true)
    expect(memoryCoreFixture('off').modelsOnDisk).toBe(false)
  })
})
