// MemoryCore IPC bridge — the local retrieval engine (embedder + reranker models,
// download progress, index counts). Mirrors `src-tauri/src/commands/memory_core.rs`
// and `memory_core/status.rs` (serde camelCase). Components/hooks never import
// `@tauri-apps/api` directly; they go through here.

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export type MemoryRetrievalMode = 'hybrid' | 'keyword'
export type MemoryDownloadPhase = 'queued' | 'downloading' | 'failed' | 'done'

export interface MemoryDownloadStatus {
  phase: MemoryDownloadPhase
  /** Model spec id, e.g. `bge-m3-int8`. */
  model: string
  file: string
  /** Source host currently used (`huggingface.co` / `hf-mirror.com` / `modelscope.cn`). */
  source: string | null
  /** Overall bytes across both models' onnx files. */
  downloadedBytes: number
  totalBytes: number
  error: string | null
}

export interface MemoryIndexStatus {
  docs: number
  vectors: number
  pendingEmbeddings: number
  /** RFC3339. */
  lastSyncAt: string | null
}

export interface MemoryCoreStatus {
  /** Monotonic per applied change; the UI drops events with an older/equal seq. */
  seq: number
  modelsEnabled: boolean
  mode: MemoryRetrievalMode
  embedderLoaded: boolean
  rerankerLoaded: boolean
  download: MemoryDownloadStatus | null
  /** Last model load error (files on disk, load failed); null once a load succeeds or models are off. */
  loadError: string | null
  index: MemoryIndexStatus
}

export const MEMORY_CORE_STATUS_EVENT = 'memory-core:status'

export const memoryCoreBridge = {
  getStatus: (): Promise<MemoryCoreStatus> => invoke<MemoryCoreStatus>('memory_core_get_status'),
  /** Turn the local models on (downloads them if missing) or off (keyword-only). */
  setModelsEnabled: (enabled: boolean): Promise<MemoryCoreStatus> =>
    invoke<MemoryCoreStatus>('memory_core_set_models_enabled', { enabled }),
  /** Retry a failed download, or a failed model load (`loadError`). */
  retryDownload: (): Promise<MemoryCoreStatus> =>
    invoke<MemoryCoreStatus>('memory_core_retry_download'),
}

/** Subscribe to pushed status changes (download progress, model load, index sync). */
export const onMemoryCoreStatus = (
  handler: (s: MemoryCoreStatus) => void,
): Promise<UnlistenFn> =>
  listen<MemoryCoreStatus>(MEMORY_CORE_STATUS_EVENT, (e) => handler(e.payload))

/** The two int8 onnx models the engine downloads (sizes mirror `memory_core/status.rs`). */
export const MEMORY_MODELS = [
  { id: 'bge-m3-int8', role: 'embedder', name: 'bge-m3', bytes: 568_456_694 },
  { id: 'bge-reranker-v2-m3-int8', role: 'reranker', name: 'bge-reranker-v2-m3', bytes: 570_727_094 },
] as const
export const MEMORY_MODELS_TOTAL_BYTES = MEMORY_MODELS[0].bytes + MEMORY_MODELS[1].bytes
