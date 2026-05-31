import * as React from 'react'
import { cn } from '@/lib/utils'
import { CollapsibleResult } from './collapsible-result'

interface DefaultResultRendererProps {
  toolName: string
  input: Record<string, unknown>
  result: string
  isError: boolean
}

/**
 * Fallback for MCP tools and any unmatched built-in. Tries to parse
 * `result` as JSON and render as a key-value table; otherwise falls
 * back to plain text wrapped in CollapsibleResult.
 */
export function DefaultResultRenderer({
  result,
  isError,
}: DefaultResultRendererProps): React.ReactElement {
  let parsed: Record<string, unknown> | null = null
  try {
    const candidate = JSON.parse(result) as unknown
    if (
      candidate !== null &&
      typeof candidate === 'object' &&
      !Array.isArray(candidate)
    ) {
      parsed = candidate as Record<string, unknown>
    }
  } catch {
    // not JSON — fall through to plain text
  }

  if (parsed) {
    return (
      <div className="rounded-md bg-muted/30 p-2 text-xs font-mono">
        <table className="w-full">
          <tbody>
            {Object.entries(parsed).map(([k, v]) => (
              <tr key={k} className="align-top">
                <td className="font-medium text-foreground pr-2 whitespace-nowrap">{k}</td>
                <td className="text-muted-foreground break-all">
                  {typeof v === 'string' ? v : JSON.stringify(v)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    )
  }

  return (
    <CollapsibleResult charThreshold={3000} previewLines={15}>
      <pre
        className={cn(
          'whitespace-pre-wrap break-all text-xs px-3 py-2 rounded-md',
          isError ? 'text-destructive bg-destructive/5' : 'text-muted-foreground bg-muted/20',
        )}
      >
        {result}
      </pre>
    </CollapsibleResult>
  )
}
