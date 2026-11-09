/**
 * Browser-mock for the MemoryCore engine (Settings › 记忆). Used by
 * `dev-tauri-mock.ts` and by component tests as fixtures.
 *
 * `?memoryCore=ready|downloading|failed|off|offondisk|empty|loading|loadfailed`
 * picks the initial state (default `ready`). Toggling / retrying transitions
 * plausibly: off→on streams a fake ~1.14 GB download over `memory-core:status`
 * (or just loads, when the models are already on disk), then "loads" the models
 * and fills the vector index. `offondisk` is "off" with the models already
 * downloaded; `loadfailed` is models on disk whose load failed (重试 reloads).
 */
import {
  MEMORY_MODELS,
  MEMORY_MODELS_TOTAL_BYTES,
  type MemoryCoreStatus,
  type MemoryDownloadStatus,
} from './bridge/memory-core'

const [EMBEDDER, RERANKER] = MEMORY_MODELS
const EMBEDDER_BYTES = EMBEDDER.bytes
const EMBEDDER_ID = EMBEDDER.id
const RERANKER_ID = RERANKER.id
const TOTAL_MODEL_BYTES = MEMORY_MODELS_TOTAL_BYTES

export const MEMORY_CORE_MOCK_STATES = [
  'ready',
  'downloading',
  'failed',
  'off',
  'offondisk',
  'empty',
  'loading',
  'loadfailed',
] as const
export type MemoryCoreMockState = (typeof MEMORY_CORE_MOCK_STATES)[number]

const minutesAgo = (m: number): string => new Date(Date.now() - m * 60_000).toISOString()

function download(
  phase: MemoryDownloadStatus['phase'],
  downloadedBytes: number,
  extra: Partial<MemoryDownloadStatus> = {},
): MemoryDownloadStatus {
  return {
    phase,
    model: downloadedBytes >= EMBEDDER_BYTES ? RERANKER_ID : EMBEDDER_ID,
    file: 'onnx/model_int8.onnx',
    source: phase === 'downloading' ? 'hf-mirror.com' : null,
    downloadedBytes,
    totalBytes: TOTAL_MODEL_BYTES,
    error: null,
    ...extra,
  }
}

/** A plausible status for each mock state. */
export function memoryCoreFixture(state: MemoryCoreMockState, seq = 1): MemoryCoreStatus {
  const index = { docs: 1284, vectors: 1284, pendingEmbeddings: 0, lastSyncAt: minutesAgo(3) }
  const base: MemoryCoreStatus = {
    seq,
    modelsEnabled: true,
    mode: 'hybrid',
    embedderLoaded: true,
    rerankerLoaded: true,
    download: null,
    loadError: null,
    modelsOnDisk: true,
    index,
  }
  switch (state) {
    case 'ready':
      return base
    case 'empty':
      return { ...base, index: { docs: 0, vectors: 0, pendingEmbeddings: 0, lastSyncAt: null } }
    case 'off':
      return {
        ...base,
        modelsEnabled: false,
        mode: 'keyword',
        embedderLoaded: false,
        rerankerLoaded: false,
        modelsOnDisk: false,
        index: { ...index, vectors: 0, pendingEmbeddings: 1284 },
      }
    case 'offondisk':
      // Models downloaded earlier, then turned off: vectors stay on disk and,
      // with no embedder loaded, the backend counts every vector.
      return {
        ...base,
        modelsEnabled: false,
        mode: 'keyword',
        embedderLoaded: false,
        rerankerLoaded: false,
      }
    case 'loadfailed':
      return {
        ...base,
        mode: 'keyword',
        embedderLoaded: false,
        rerankerLoaded: false,
        loadError:
          'load bge-m3: Failed to create session: Load model from onnx/model_int8.onnx failed: Protobuf parsing failed.',
        index: { ...index, vectors: 0, pendingEmbeddings: 1284 },
      }
    case 'loading':
      return {
        ...base,
        mode: 'keyword',
        embedderLoaded: false,
        rerankerLoaded: false,
        download: download('done', TOTAL_MODEL_BYTES),
        index: { ...index, vectors: 0, pendingEmbeddings: 1284 },
      }
    case 'downloading':
      return {
        ...base,
        mode: 'keyword',
        embedderLoaded: false,
        rerankerLoaded: false,
        download: download('downloading', 412_000_000),
        modelsOnDisk: false,
        index: { ...index, vectors: 0, pendingEmbeddings: 1284 },
      }
    case 'failed':
      return {
        ...base,
        mode: 'keyword',
        embedderLoaded: false,
        rerankerLoaded: false,
        modelsOnDisk: false,
        download: download('failed', 0, {
          error:
            'all sources failed for onnx/model_int8.onnx: error sending request for url (https://hf-mirror.com/Xenova/bge-m3/resolve/main/onnx/model_int8.onnx): operation timed out',
        }),
        index: { ...index, vectors: 0, pendingEmbeddings: 1284 },
      }
  }
}

export function parseMemoryCoreMockState(search: string): MemoryCoreMockState {
  const v = new URLSearchParams(search).get('memoryCore')
  return (MEMORY_CORE_MOCK_STATES as readonly string[]).includes(v ?? '')
    ? (v as MemoryCoreMockState)
    : 'ready'
}

type Emit = (status: MemoryCoreStatus) => Promise<void> | void
const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms))

/** Stateful mock of the three `memory_core_*` commands + the status event stream. */
export function createMemoryCoreMock(initial: MemoryCoreMockState, emit: Emit) {
  let state = memoryCoreFixture(initial)
  let run = 0 // bumped to cancel an in-flight simulation
  let started = false

  const snapshot = (): MemoryCoreStatus => structuredClone(state)
  const update = async (f: (s: MemoryCoreStatus) => void, myRun: number): Promise<boolean> => {
    if (myRun !== run) return false
    f(state)
    state.seq += 1
    await emit(snapshot())
    return true
  }

  async function loadModelsAndIndex(myRun: number): Promise<void> {
    await sleep(2500)
    if (!(await update((s) => {
      s.embedderLoaded = true
      s.rerankerLoaded = true
      s.mode = 'hybrid'
    }, myRun))) return
    // Embed the backlog in a few bursts.
    while (state.index.vectors < state.index.docs) {
      await sleep(400)
      const ok = await update((s) => {
        s.index.vectors = Math.min(s.index.docs, s.index.vectors + 220)
        s.index.pendingEmbeddings = s.index.docs - s.index.vectors
        s.index.lastSyncAt = new Date().toISOString()
      }, myRun)
      if (!ok) return
    }
  }

  async function downloadModels(myRun: number): Promise<void> {
    const from = state.download?.phase === 'downloading' ? state.download.downloadedBytes : 0
    if (from === 0) {
      if (!(await update((s) => { s.download = download('queued', 0) }, myRun))) return
      await sleep(700)
    }
    const step = 40_000_000
    for (let b = from + step; ; b += step) {
      const bytes = Math.min(b, TOTAL_MODEL_BYTES)
      if (!(await update((s) => { s.download = download('downloading', bytes) }, myRun))) return
      if (bytes >= TOTAL_MODEL_BYTES) break
      await sleep(300)
    }
    if (!(await update((s) => {
      s.download = download('done', TOTAL_MODEL_BYTES)
      s.modelsOnDisk = true
    }, myRun))) return
    await loadModelsAndIndex(myRun)
  }

  function startLifecycle(expectedRun: number): void {
    if (run !== expectedRun) return // the user already toggled/retried; their run owns the state
    const myRun = ++run
    if (state.download?.phase === 'downloading') void downloadModels(myRun)
    else if (state.modelsEnabled && !state.embedderLoaded && state.modelsOnDisk && !state.loadError) {
      void loadModelsAndIndex(myRun)
    }
  }

  return {
    getStatus(): MemoryCoreStatus {
      // Lazily resume an in-progress initial state (downloading / loading) on first read.
      if (!started) {
        started = true
        const at = run
        setTimeout(() => startLifecycle(at), 600)
      }
      return snapshot()
    },
    setModelsEnabled(enabled: boolean): MemoryCoreStatus {
      const myRun = ++run
      state.seq += 1
      if (!enabled) {
        Object.assign(state, {
          modelsEnabled: false,
          mode: 'keyword',
          embedderLoaded: false,
          rerankerLoaded: false,
          download: null,
          loadError: null,
        })
        // Vectors stay on disk; with no model loaded the backend counts them all.
        return snapshot()
      }
      state.modelsEnabled = true
      if (state.modelsOnDisk) {
        void loadModelsAndIndex(myRun)
      } else {
        state.download = download('queued', 0)
        void sleep(500).then(() => downloadModels(myRun))
      }
      return snapshot()
    },
    retryDownload(): MemoryCoreStatus {
      if (state.modelsEnabled && state.loadError && !state.embedderLoaded) {
        const myRun = ++run
        state.seq += 1
        state.loadError = null // back to "loading"; the load reports its outcome
        void loadModelsAndIndex(myRun)
        return snapshot()
      }
      const myRun = ++run
      state.seq += 1
      state.download = download('queued', 0)
      void sleep(500).then(() => downloadModels(myRun))
      return snapshot()
    },
  }
}
