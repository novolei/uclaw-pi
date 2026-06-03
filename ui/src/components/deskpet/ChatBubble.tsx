/**
 * Desk-pet chat balloon (S4).
 *
 * A compact composer + a streamed reply bubble. On send it calls the pet bridge
 * `petChat(text, personaId)` and subscribes to the streaming events:
 *   - `pet:reply-delta` → append text   (pet state: typing)
 *   - `pet:reply-done`  → finalize       (pet state: idle)
 *   - `pet:reply-error` → show "本地模型未就绪…" (pet state: error)
 *
 * Pet state is driven directly via `petPrimaryStateAtom` (the desk-pet window is
 * a light root that does NOT mount `usePetStateSync`, which keys off the agent
 * chat-stream events; here the source of truth is the pet's own reply stream).
 */
import { useAtomValue, useSetAtom } from 'jotai'
import { useCallback, useEffect, useRef, useState } from 'react'
import { petPersonaIdAtom, petPrimaryStateAtom } from '@/atoms/pet-atoms'
import {
  onPetReplyDelta,
  onPetReplyDone,
  onPetReplyError,
  petChat,
} from '@/lib/bridge/pet'

export function ChatBubble() {
  const personaId = useAtomValue(petPersonaIdAtom)
  const setPetState = useSetAtom(petPrimaryStateAtom)

  const [input, setInput] = useState('')
  const [reply, setReply] = useState('')
  const [errorMsg, setErrorMsg] = useState<string | null>(null)
  const [streaming, setStreaming] = useState(false)
  const streamingRef = useRef(false)

  // Subscribe once to the streaming events. Deltas append; done finalizes;
  // error surfaces a user-facing message. Guarded by streamingRef so late
  // events after an aborted send don't mutate stale UI.
  useEffect(() => {
    const unlistens: Array<() => void> = []
    let cancelled = false

    const bind = (p: Promise<() => void>) => {
      p.then((u) => {
        if (cancelled) u()
        else unlistens.push(u)
      }).catch((e) => console.warn('[ChatBubble] listen failed:', e))
    }

    bind(
      onPetReplyDelta(({ text }) => {
        if (!streamingRef.current) return
        setReply((prev) => prev + text)
        setPetState('typing')
      }),
    )
    bind(
      onPetReplyDone(() => {
        if (!streamingRef.current) return
        streamingRef.current = false
        setStreaming(false)
        setPetState('idle')
      }),
    )
    bind(
      onPetReplyError(({ message }) => {
        streamingRef.current = false
        setStreaming(false)
        setErrorMsg(message || '本地模型未就绪')
        setPetState('error')
      }),
    )

    return () => {
      cancelled = true
      for (const u of unlistens) {
        try {
          u()
        } catch {
          /* ignore */
        }
      }
    }
  }, [setPetState])

  const send = useCallback(() => {
    const text = input.trim()
    if (!text || streamingRef.current) return
    setInput('')
    setReply('')
    setErrorMsg(null)
    streamingRef.current = true
    setStreaming(true)
    setPetState('thinking')
    petChat(text, personaId).catch((e) => {
      // The backend also emits `pet:reply-error`; this catches transport-level
      // failures (e.g. the command itself rejecting).
      console.warn('[ChatBubble] petChat failed:', e)
      if (!streamingRef.current) return
      streamingRef.current = false
      setStreaming(false)
      setErrorMsg('本地模型未就绪')
      setPetState('error')
    })
  }, [input, personaId, setPetState])

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      send()
    }
  }

  return (
    <div className="deskpet-bubble" role="group" aria-label="桌面伙伴对话">
      {(reply || errorMsg || streaming) && (
        <div
          className={`deskpet-balloon${errorMsg ? ' is-error' : ''}`}
          role="status"
          aria-live="polite"
        >
          {errorMsg ?? (reply || '…')}
        </div>
      )}
      <div className="deskpet-composer">
        <textarea
          className="deskpet-input"
          rows={1}
          value={input}
          placeholder="和我聊聊…"
          aria-label="消息"
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={onKeyDown}
        />
        <button
          type="button"
          className="deskpet-send"
          aria-label="发送"
          disabled={streaming || input.trim().length === 0}
          onClick={send}
        >
          发送
        </button>
      </div>
    </div>
  )
}
