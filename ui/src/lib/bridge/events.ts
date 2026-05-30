/**
 * Typed subscription factory for the engine's streaming events (§2A.3 / §3.3).
 *
 * The event NAMES are the contract — byte-identical to what
 * `crates/uclaw-pi-engine` emits (see `events::event`) and what
 * `useGlobalAgentListeners` currently binds with bare `listen()`. This module
 * is the single typed entry point hooks should migrate to.
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

export const STREAM_EVENT = {
  chunk: 'chat:stream-chunk',
  reasoning: 'chat:stream-reasoning',
  toolActivity: 'chat:stream-tool-activity',
  complete: 'chat:stream-complete',
  error: 'chat:stream-error',
} as const

/** `chat:stream-chunk` / `chat:stream-reasoning` payload. */
export interface StreamDelta {
  conversationId: string
  delta: string
  seq: number
}

/** `chat:stream-complete` payload. */
export interface StreamComplete {
  conversationId: string
  text: string
  truncated: boolean
}

/** `chat:stream-error` payload. */
export interface StreamError {
  conversationId: string
  error: string
}

/** `chat:stream-tool-activity` payload. */
export interface StreamToolActivity {
  conversationId: string
  activity: {
    type: 'tool_start' | 'tool_result'
    toolName: string
    toolCallId: string
    input?: unknown
    result?: unknown
    isError?: boolean
    durationMs?: number | null
    timestamp?: number
  }
}

const sub =
  <T>(name: string) =>
  (cb: (payload: T) => void): Promise<UnlistenFn> =>
    listen<T>(name, (e) => cb(e.payload))

export const onStreamChunk = sub<StreamDelta>(STREAM_EVENT.chunk)
export const onStreamReasoning = sub<StreamDelta>(STREAM_EVENT.reasoning)
export const onStreamToolActivity = sub<StreamToolActivity>(STREAM_EVENT.toolActivity)
export const onStreamComplete = sub<StreamComplete>(STREAM_EVENT.complete)
export const onStreamError = sub<StreamError>(STREAM_EVENT.error)
