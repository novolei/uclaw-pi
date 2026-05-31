import * as React from 'react'
import { ChevronDown, ChevronUp } from 'lucide-react'
import { cn } from '@/lib/utils'

interface CollapsibleResultProps {
  /** Char-length above which the collapse UI appears. Default 3000. */
  charThreshold?: number
  /** Number of lines visible when collapsed. Default 15. */
  previewLines?: number
  children: React.ReactNode
}

/**
 * Walks a React node tree extracting plain text. Used to measure
 * content length against the collapse threshold. Doesn't try to
 * be perfect — just enough to count chars/lines for the heuristic.
 */
function extractText(node: React.ReactNode): string {
  if (node == null || typeof node === 'boolean') return ''
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(extractText).join('')
  if (React.isValidElement(node)) {
    const props = node.props as { children?: React.ReactNode }
    return extractText(props.children)
  }
  return ''
}

export function CollapsibleResult({
  charThreshold = 3000,
  previewLines = 15,
  children,
}: CollapsibleResultProps): React.ReactElement {
  const text = React.useMemo(() => extractText(children), [children])
  const charCount = text.length
  const lineCount = text.split('\n').length
  const exceedsThreshold = charCount > charThreshold
  const [expanded, setExpanded] = React.useState(false)

  if (!exceedsThreshold) {
    return <>{children}</> as React.ReactElement
  }

  return (
    <div>
      <div
        className={cn('transition-all', !expanded && 'overflow-hidden')}
        style={!expanded ? { maxHeight: `${previewLines * 1.5}em` } : undefined}
      >
        {children}
      </div>
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="mt-1.5 inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
      >
        {expanded ? <ChevronUp className="size-3" /> : <ChevronDown className="size-3" />}
        {expanded ? '收起' : `展开全部 (${charCount} 字符, ${lineCount} 行)`}
      </button>
    </div>
  )
}
