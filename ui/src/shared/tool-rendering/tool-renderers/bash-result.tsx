import * as React from 'react'
import { invoke } from '@tauri-apps/api/core'
import { cn } from '@/lib/utils'
import { CollapsibleResult } from './collapsible-result'

interface Props {
  input: Record<string, unknown>
  result: string
  isError: boolean
}

const ERROR_PATTERNS = /(error|exception|traceback|failed|fatal|panic|warning)/i
// Backend (shell.rs `to_truncated_string`) prepends this header when bash output
// exceeds the 32KB cap:
//   [输出已截断：共 N 字节，显示最后 M 字节，完整输出已保存至 /path/bash-<uuid>.log]\n\n<tail>
// Surface it as an amber banner + a "加载完整日志" button instead of rendering the
// raw header as plain terminal text. (It was only wired into the live
// BashStreamView before, never the completed-result renderer — that gap is why
// a finished truncated command showed no notice.)
const TRUNCATION_RE = /^\[输出已截断[^\]]*\]\n*/
const LOG_PATH_RE = /保存至 (.+?)\]/

export function BashResultRenderer({ input, result, isError }: Props): React.ReactElement {
  const command = (input.command as string | undefined) ?? ''
  // Normalize both actual newlines and escaped \n sequences (JSX string attrs pass literal \n)
  const normalized = result.replace(/\\n/g, '\n')

  // Detect the backend truncation header and (if present) the temp-log path.
  const headerMatch = TRUNCATION_RE.exec(normalized)
  const truncated = headerMatch !== null
  const logPath = truncated ? LOG_PATH_RE.exec(headerMatch[0])?.[1]?.trim() : undefined
  // Body without the header line — the amber banner conveys truncation instead.
  const body = truncated ? normalized.slice(headerMatch[0].length) : normalized

  const [fullLog, setFullLog] = React.useState<string | null>(null)
  const [loading, setLoading] = React.useState(false)

  const handleLoadFull = React.useCallback(async () => {
    if (!logPath) return
    setLoading(true)
    try {
      setFullLog(await invoke<string>('read_bash_log', { path: logPath }))
    } catch (e) {
      setFullLog(`加载失败: ${String(e)}`)
    } finally {
      setLoading(false)
    }
  }, [logPath])

  const display = fullLog ?? body
  const lines = display.split('\n')

  return (
    <CollapsibleResult charThreshold={2000} previewLines={20}>
      <div className="rounded-md bg-zinc-950 text-zinc-100 font-mono text-xs p-3 overflow-x-auto">
        <div className="text-emerald-400 mb-1.5">$ {command}</div>
        {truncated && (
          <div className="text-amber-400/80 mb-1 flex items-center gap-2">
            <span>⋯ 早期输出已截断</span>
            {logPath && (
              <button
                type="button"
                onClick={handleLoadFull}
                disabled={loading || fullLog !== null}
                className="px-1.5 py-0.5 rounded border border-zinc-700 hover:bg-zinc-800 disabled:opacity-50"
              >
                {loading ? '加载中…' : fullLog !== null ? '已加载完整日志' : '加载完整日志'}
              </button>
            )}
          </div>
        )}
        <pre className="whitespace-pre-wrap break-all">
          {lines.map((line, i) => {
            const isErrorLine = isError || ERROR_PATTERNS.test(line)
            return (
              <div key={i} className={cn(isErrorLine && 'text-red-400')}>
                {line || ' ' /* preserve blank lines */}
              </div>
            )
          })}
        </pre>
      </div>
    </CollapsibleResult>
  )
}
