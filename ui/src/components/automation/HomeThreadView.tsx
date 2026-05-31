import { useEffect, useRef, useState } from 'react'
import { getOrCreateSpecHomeThread, getAgentSessionMessages, sendAgentMessage } from '@/lib/tauri-bridge'
import { AgentMessages } from '@/features/agent'
import type { AgentMessage } from '@/lib/agent-types'

interface Props {
  specId: string
}

export function HomeThreadView({ specId }: Props) {
  const [sessionId, setSessionId] = useState<string | null>(null)
  const [messages, setMessages] = useState<AgentMessage[]>([])
  const [loaded, setLoaded] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [retryKey, setRetryKey] = useState(0)
  const [input, setInput] = useState('')
  const [sending, setSending] = useState(false)
  const inputRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    let cancelled = false
    setLoaded(false)
    setMessages([])
    setSessionId(null)
    setError(null)
    getOrCreateSpecHomeThread(specId)
      .then((session) => {
        if (cancelled) return null
        setSessionId(session.id)
        return getAgentSessionMessages(session.id)
      })
      .then((msgs) => {
        if (!cancelled && msgs) {
          setMessages(msgs as AgentMessage[])
          setLoaded(true)
        }
      })
      .catch((err) => {
        if (!cancelled) setError(err?.message ?? '加载失败')
      })
    return () => { cancelled = true }
  }, [specId, retryKey])

  async function handleSend() {
    if (!sessionId || !input.trim() || sending) return
    const text = input.trim()
    setInput('')
    setSending(true)
    try {
      await sendAgentMessage({ sessionId, userMessage: text })
      const updated = await getAgentSessionMessages(sessionId)
      setMessages(updated as AgentMessage[])
    } finally {
      setSending(false)
      inputRef.current?.focus()
    }
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  if (error) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center gap-2 text-sm text-muted-foreground">
        <span>加载失败：{error}</span>
        <button
          onClick={() => { setError(null); setRetryKey(k => k + 1) }}
          className="titlebar-no-drag text-primary underline"
        >
          重试
        </button>
      </div>
    )
  }

  if (!sessionId) {
    return (
      <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
        加载中…
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
        <AgentMessages
          sessionId={sessionId}
          messages={messages}
          messagesLoaded={loaded}
          streaming={false}
        />
      </div>

      {/* composer */}
      <div className="shrink-0 border-t border-border/50 p-2">
        <div className="flex gap-2 items-end rounded-lg border border-border bg-background px-3 py-2">
          <textarea
            ref={inputRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="发消息…"
            rows={1}
            disabled={sending}
            className="titlebar-no-drag flex-1 resize-none bg-transparent text-sm outline-none min-h-[24px] max-h-[120px] disabled:opacity-50"
          />
          <button
            onClick={handleSend}
            disabled={!input.trim() || sending}
            className="titlebar-no-drag shrink-0 text-primary disabled:opacity-40 text-sm"
          >
            ➤
          </button>
        </div>
      </div>
    </div>
  )
}
