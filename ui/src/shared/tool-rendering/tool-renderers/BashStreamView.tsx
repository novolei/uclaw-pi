import * as React from 'react'
import { cn } from '@/lib/utils'
import { invoke } from '@tauri-apps/api/core'
import type { LiveOutput } from '@/atoms/agent-atoms'

interface Props {
  command: string
  live: LiveOutput
  /** 从 tool_result 截断头注解析出的 temp 路径;有值才显示「加载完整日志」 */
  logPath?: string
}

/**
 * Bash 实时流式输出视图。stdout 默认色、stderr 用 text-red-400;
 * 流式期间自动滚动到底部;droppedHead 时提供「加载完整日志」(读 temp 文件)。
 */
export function BashStreamView({ command, live, logPath }: Props): React.ReactElement {
  const scrollRef = React.useRef<HTMLPreElement>(null)
  const [fullLog, setFullLog] = React.useState<string | null>(null)
  const [loading, setLoading] = React.useState(false)

  React.useEffect(() => {
    const el = scrollRef.current
    if (!el) return
    const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40
    if (nearBottom) el.scrollTop = el.scrollHeight
  }, [live.bytes])

  const handleLoadFull = React.useCallback(async () => {
    if (!logPath) return
    setLoading(true)
    try {
      const content = await invoke<string>('read_bash_log', { path: logPath })
      setFullLog(content)
    } catch (e) {
      setFullLog(`加载失败: ${String(e)}`)
    } finally {
      setLoading(false)
    }
  }, [logPath])

  return (
    <div className="rounded-md bg-zinc-950 text-zinc-100 font-mono text-xs p-3 overflow-x-auto">
      <div className="text-emerald-400 mb-1.5">$ {command}</div>
      {live.droppedHead && (
        <div className="text-amber-400/80 mb-1 flex items-center gap-2">
          <span>⋯ 早期输出已截断</span>
          {logPath && (
            <button
              type="button"
              onClick={handleLoadFull}
              disabled={loading}
              className="px-1.5 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800 disabled:opacity-50"
            >
              {loading ? '加载中…' : '加载完整日志'}
            </button>
          )}
        </div>
      )}
      <pre ref={scrollRef} className="whitespace-pre-wrap break-all max-h-[320px] overflow-y-auto">
        {fullLog !== null
          ? fullLog
          : live.segments.map((seg, i) => (
              <span key={i} className={cn(seg.stream === 'stderr' && 'text-red-400')}>
                {seg.text}
              </span>
            ))}
      </pre>
    </div>
  )
}
