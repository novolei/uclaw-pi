import * as React from 'react'
import type { ComponentProps } from 'react'
import Markdown, { defaultUrlTransform } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { Hexagon } from 'lucide-react'

import { cn } from '@/lib/utils'
import { MarkdownCodeBlock } from '@/components/shared/code-block/CodeBlock'
import { markdownFileChipPlugin } from '@/components/preview/chips/markdownFileChipPlugin'
import { FilePathChip } from '@/components/preview/chips/FilePathChip'
import { useFileChipResolver, useChipCacheInvalidator } from '@/components/preview/chips/useFileChipResolver'

// ===== Message 原语组件 =====

interface MessageProps {
  from: 'user' | 'assistant'
  children: React.ReactNode
}

export function Message({ from, children }: MessageProps): React.ReactElement {
  return (
    <div className={cn('px-4 py-3', from === 'user' ? 'bg-transparent' : '')}>
      {children}
    </div>
  )
}

export function MessageHeader({
  name,
  model,
  time,
  logo,
}: {
  /** Display name of the agent (e.g. "uClaw"). Configurable per-session.
   *  When omitted, falls back to "uClaw" → "Assistant" → "" in that order. */
  name?: string
  /** Underlying model id (e.g. "deepseek-v4-flash") — rendered as
   *  caption next to the agent name. Optional. */
  model?: string
  time?: string
  logo?: React.ReactNode
}): React.ReactElement {
  // Resolve the primary label with a 3-tier fallback so existing callers
  // that only pass `model` still render meaningfully (back-compat path).
  const primaryName = name ?? 'uClaw'

  return (
    <div className="flex items-start gap-2.5 mb-2.5">
      {logo}
      <div className="flex flex-col justify-between h-[35px]">
        <div className="flex items-baseline gap-1.5 leading-none">
          <span className="text-sm font-semibold text-foreground/60">
            {primaryName}
          </span>
          {model && (
            <span
              className="text-[11px] font-normal text-foreground/40 tracking-wide lowercase"
              title={model}
            >
              {model}
            </span>
          )}
        </div>
        {time && <span className="text-[10px] text-foreground/[0.38] leading-none">{time}</span>}
      </div>
    </div>
  )
}

export function MessageContent({ children, className }: { children: React.ReactNode; className?: string }): React.ReactElement {
  return <div className={cn('pl-[46px]', className)}>{children}</div>
}

export function MessageActions({
  children,
  className,
}: {
  children: React.ReactNode
  className?: string
}): React.ReactElement {
  return (
    <div className={cn('flex items-center gap-0.5 opacity-0 hover:opacity-100 transition-opacity', className)}>
      {children}
    </div>
  )
}

export function MessageAction({
  children,
  onClick,
  tooltip,
  disabled,
}: {
  children: React.ReactNode
  onClick?: () => void
  tooltip?: string
  disabled?: boolean
}): React.ReactElement {
  return (
    <button
      type="button"
      className="p-1 rounded text-muted-foreground/60 hover:text-foreground transition-colors disabled:opacity-40"
      onClick={onClick}
      title={tooltip}
      disabled={disabled}
    >
      {children}
    </button>
  )
}

// ===== mdast 工具：保留换行（user 消息中的 \n 转为 <br>）=====

interface MdastTextNode { type: 'text'; value: string }
interface MdastBreakNode { type: 'break' }
interface MdastGenericNode { type: string; children?: MdastNode[]; value?: string }
type MdastNode = MdastTextNode | MdastBreakNode | MdastGenericNode
interface MdastParent { type: string; children: MdastNode[] }

function walkMdastText(
  node: MdastParent,
  visitor: (node: MdastTextNode, index: number, parent: MdastParent) => number | void,
): void {
  if (!node.children) return
  for (let i = 0; i < node.children.length; i++) {
    const child = node.children[i]!
    if (child.type === 'text') {
      const result = visitor(child as MdastTextNode, i, node)
      if (typeof result === 'number') i = result - 1
    } else if (child.type !== 'code' && child.type !== 'inlineCode') {
      const asParent = child as MdastParent
      if (asParent.children) walkMdastText(asParent, visitor)
    }
  }
}

/** 将 text 节点中的 \n 转为 break 节点（跳过代码块） */
function remarkPreserveBreaks() {
  return (tree: MdastParent) => {
    walkMdastText(tree, (node, index, parent) => {
      const text = node.value
      if (!text.includes('\n')) return
      const lines = text.split('\n')
      const parts: MdastNode[] = []
      for (let i = 0; i < lines.length; i++) {
        if (i > 0) parts.push({ type: 'break' })
        if (lines[i]) parts.push({ type: 'text', value: lines[i] })
      }
      parent.children.splice(index, 1, ...parts)
      return index + parts.length
    })
  }
}

// ===== Markdown 渲染器组件 =====

// remark-math + rehype-katex removed entirely.
//
// Why: agent responses overwhelmingly contain dollar amounts ($3.65,
// $294.76) and numeric ranges ($580 — 600$) rather than LaTeX math.
// remark-math treats any `$...$` or `$$...$$` pair as math and feeds
// the contents to KaTeX, which then warns about Unicode (CJK) chars in
// math mode and renders the content in a serif italic typeface. The
// result: random spans of agent text show up as math italic — a real
// bug, never a real feature for our users.
//
// Disabling `singleDollarTextMath` only addressed half of it (still
// allowed `$$...$$`). Removing the plugin chain entirely is the only
// robust fix. Users who genuinely need formula rendering can wrap in
// a ```latex``` code block (which our highlighter already handles) or
// paste a rendered image.
import type { PluggableList } from 'unified'
const REMARK_PLUGINS: PluggableList = [remarkGfm, markdownFileChipPlugin]
const REMARK_PLUGINS_WITH_BREAKS: PluggableList = [remarkGfm, remarkPreserveBreaks, markdownFileChipPlugin]
const REHYPE_PLUGINS: PluggableList = []

/** 链接渲染器：外部 URL 用 Tauri openExternal 打开 */
const MarkdownLink = React.memo(function MarkdownLink({
  href,
  children: linkChildren,
  ...linkProps
}: React.AnchorHTMLAttributes<HTMLAnchorElement>): React.ReactElement {
  return (
    <a
      {...linkProps}
      href={href}
      onClick={(e) => {
        if (href && (href.startsWith('http://') || href.startsWith('https://'))) {
          e.preventDefault()
          // 通过 Tauri 后端打开外部链接（避免在 webview 内导航）
          import('@/lib/tauri-bridge').then((m) => m.openExternal(href)).catch(() => {
            window.open(href, '_blank', 'noopener,noreferrer')
          })
        }
      }}
      title={href}
    >
      {linkChildren}
    </a>
  )
})

/** 代码块渲染器 */
const MarkdownPre = React.memo(function MarkdownPre({
  children: preChildren,
}: { children?: React.ReactNode }): React.ReactElement {
  return <MarkdownCodeBlock>{preChildren}</MarkdownCodeBlock>
})

/** 行内代码渲染器（仅在没有 language- 类时生效；有则交给 MarkdownPre 处理） */
const MarkdownInlineCode = React.memo(function MarkdownInlineCode({
  children: codeChildren,
  className: codeClassName,
  ...codeProps
}: React.HTMLAttributes<HTMLElement>): React.ReactElement {
  // 代码块（fenced code）的 <code> 走 MarkdownPre，这里只处理行内代码
  if (codeClassName) {
    return <code className={codeClassName} {...codeProps}>{codeChildren}</code>
  }
  return (
    <code
      className="rounded bg-foreground/10 px-[0.35em] py-[0.15em] text-[0.875em] font-medium"
      {...codeProps}
    >
      {codeChildren}
    </code>
  )
})

/** 标题渲染器：h1/h2 带左侧实心条做视觉锚点；h3 纯文本 */
const MarkdownH1 = React.memo(function MarkdownH1({
  children,
}: React.HTMLAttributes<HTMLHeadingElement>): React.ReactElement {
  return (
    <h1 className="flex items-center gap-2.5 text-[22px] font-semibold tracking-[-0.015em] mt-7 mb-3.5 first:mt-0 leading-[1.3]">
      <span className="w-[3px] h-[18px] bg-foreground rounded-sm shrink-0" aria-hidden />
      <span>{children}</span>
    </h1>
  )
})

const MarkdownH2 = React.memo(function MarkdownH2({
  children,
}: React.HTMLAttributes<HTMLHeadingElement>): React.ReactElement {
  return (
    <h2 className="flex items-center gap-2.5 text-[19px] font-semibold tracking-[-0.012em] mt-[22px] mb-3 first:mt-0 leading-[1.35]">
      <span className="w-[3px] h-4 bg-foreground rounded-sm shrink-0" aria-hidden />
      <span>{children}</span>
    </h2>
  )
})

const MarkdownH3 = React.memo(function MarkdownH3({
  children,
}: React.HTMLAttributes<HTMLHeadingElement>): React.ReactElement {
  return (
    <h3 className="text-[16px] font-semibold mt-[18px] mb-2 first:mt-0 leading-[1.4] tracking-[-0.005em]">
      {children}
    </h3>
  )
})

/** 表格渲染器：包一层 card 容器，让表格有清晰边界 */
const MarkdownTable = React.memo(function MarkdownTable({
  children,
}: React.HTMLAttributes<HTMLTableElement>): React.ReactElement {
  // `not-prose` opts the entire table subtree out of @tailwindcss/typography
  // defaults. Without it, prose injects:
  //   - `table { margin-top: 2em; margin-bottom: 2em }`
  //     → renders as empty cream bands above/below the table inside our
  //       `bg-card` wrapper (user-visible "blank rows" bug).
  //   - `tbody tr { border-bottom: 1px }`
  //     → competes with our `border-border/40` row borders.
  //   - `thead th { vertical-align: bottom }`
  //     → fights our `align-middle`.
  // Once not-prose is set, our MarkdownTr/Th/Td classes have unambiguous
  // control of every visual property.
  return (
    <div className="not-prose my-3 rounded-[10px] border border-border overflow-hidden bg-card">
      <table className="w-full border-collapse text-[14px]">{children}</table>
    </div>
  )
})

const MarkdownThead = React.memo(function MarkdownThead({
  children,
}: React.HTMLAttributes<HTMLTableSectionElement>): React.ReactElement {
  // Theme-agnostic tint via `--foreground` — warm-paper / qingye etc.
  // have such low-contrast `--muted` against their `--card` that
  // bg-muted/* is invisible. A foreground-derived tint is identical
  // visual weight on every theme.
  return <thead className="bg-foreground/[0.05]">{children}</thead>
})

const MarkdownTr = React.memo(function MarkdownTr({
  children,
}: React.HTMLAttributes<HTMLTableRowElement>): React.ReactElement {
  // Polish set, all theme-agnostic via foreground tint:
  //   - Zebra (3.5% foreground tint) — readable on every theme without
  //     competing with status badges or text legibility
  //   - Hover (8% foreground tint) — clearly tactile on light AND dark
  //   - Border on every td except in the last row keeps the card
  //     bottom edge clean
  // Why foreground/<alpha> and not muted/<alpha>: `--muted` is ~95-98%
  // of `--card` on warm themes (paper, sepia), so a 30% overlay of one
  // beige on another beige produces ~1% rendered difference — invisible.
  // Foreground tints are derived from the text color, so contrast is
  // guaranteed on every theme.
  return (
    <tr
      className={cn(
        '[&:not(:last-child)>td]:border-b [&>td]:border-border/40',
        '[&:nth-child(even)]:bg-foreground/[0.035]',
        'hover:bg-foreground/[0.075] transition-colors',
      )}
    >
      {children}
    </tr>
  )
})

const MarkdownTh = React.memo(function MarkdownTh({
  children,
}: React.HTMLAttributes<HTMLTableCellElement>): React.ReactElement {
  return (
    <th className="text-left px-3.5 py-2 text-[11px] font-semibold uppercase tracking-[0.07em] text-muted-foreground/85 border-b border-border align-middle">
      {children}
    </th>
  )
})

/** 递归收集 React 子树里的文本节点（用于 td 内的状态识别） */
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

type StatusVariant = 'success' | 'warning' | 'danger'

const STATUS_PATTERNS: Array<{ re: RegExp; variant: StatusVariant }> = [
  // Order matters: explicit failure / negation wins over completion.
  { re: /❌|✗|未开始|尚未|failed\b|error\b/i, variant: 'danger' },
  { re: /⏳|未完成|in[\s-]?progress\b|pending\b|wip\b/i, variant: 'warning' },
  { re: /✅|✓|已完成/, variant: 'success' },
]

function detectStatus(text: string): StatusVariant | null {
  for (const { re, variant } of STATUS_PATTERNS) {
    if (re.test(text)) return variant
  }
  return null
}

const STATUS_CLASS: Record<StatusVariant, string> = {
  success: 'bg-[hsl(var(--success-bg))] text-[hsl(var(--success))]',
  warning: 'bg-[hsl(var(--warning-bg))] text-[hsl(var(--warning))]',
  danger:  'bg-[hsl(var(--danger-bg))] text-[hsl(var(--danger))]',
}

/** 单元格渲染器：检测状态文本，自动包成 badge */
const MarkdownTd = React.memo(function MarkdownTd({
  children,
}: React.HTMLAttributes<HTMLTableCellElement>): React.ReactElement {
  const text = extractText(children).trim()
  const variant = text ? detectStatus(text) : null
  // Polish: tighter vertical padding + middle align + zero out any
  // `prose-p:my-1.5` margin that would otherwise stretch cells when
  // the content is wrapped in <p> by react-markdown. `align-middle`
  // matters when one cell wraps to two lines and the neighbour doesn't.
  const tdClass =
    'px-3.5 py-2 text-[14px] align-middle leading-[1.55] [&>p]:my-0 [&_p]:my-0'
  if (variant) {
    return (
      <td className={tdClass}>
        <span
          data-status={variant}
          className={cn(
            'inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[12px] font-medium',
            STATUS_CLASS[variant],
          )}
        >
          {text}
        </span>
      </td>
    )
  }
  return <td className={tdClass}>{children}</td>
})

const MarkdownBlockquote = React.memo(function MarkdownBlockquote({
  children,
}: React.HTMLAttributes<HTMLQuoteElement>): React.ReactElement {
  return (
    <blockquote className="my-3 pl-3 border-l-2 border-foreground/20 text-foreground/75 not-italic [&>p]:my-1">
      {children}
    </blockquote>
  )
})

const MarkdownHr = React.memo(function MarkdownHr(): React.ReactElement {
  return <hr className="my-6 border-0 border-t border-border/60" />
})

/**
 * `<strong>` renderer applied uniformly to ALL `**bold**` spans, regardless
 * of whether they live inside a `not-prose` subtree (e.g. our tables).
 *
 * Why a dedicated component instead of `prose-strong:` utilities:
 *   1. `MarkdownTable` wraps the `<table>` in `not-prose` (so prose's
 *      default table styling doesn't fight our card layout). That also
 *      strips `prose-strong:*` styles from `<strong>` inside cells —
 *      browser default `font-weight: bold` (700) takes over, which reads
 *      as a typeface change in mixed CJK + Latin prose.
 *   2. The previous setup also forced `text-foreground` on bold, which
 *      broke `<blockquote>`'s dimmed (`text-foreground/75`) color —
 *      bold spans popped via color contrast even when their weight
 *      matched, again reading as a different font.
 *
 * The fix is purely additive: `font-medium` (500) for a subtle emphasis,
 * inherit color from the parent so bold blends with whatever container
 * it lives in.
 */
const MarkdownStrong = React.memo(function MarkdownStrong({
  children,
}: React.HTMLAttributes<HTMLElement>): React.ReactElement {
  return <strong className="font-medium text-inherit">{children}</strong>
})

/**
 * `<em>` renderer.
 *
 * Markdown `*x*` becomes `<em>` which the browser/prose styles render as
 * `font-style: italic`. In Latin fonts that produces a true italic
 * (slanted serif glyphs); in CJK text there's no italic form, so adjacent
 * Chinese characters stay upright while Latin digits / words inside the
 * `<em>` slant. User-reported symptom: numbers like `570-580` look like
 * "a different font" when emphasized via `*570-580*` between Chinese
 * characters.
 *
 * Fix: drop the italic transform entirely. Apply the same subtle
 * `font-medium` (500) emphasis as `<strong>` so the semantic is still
 * conveyed without typeface-mismatch glyph slanting. Italic is bad
 * typography in mixed CJK + Latin anyway.
 */
const MarkdownEm = React.memo(function MarkdownEm({
  children,
}: React.HTMLAttributes<HTMLElement>): React.ReactElement {
  return <em className="not-italic font-medium text-inherit">{children}</em>
})

// ===== File chip adapter (wires HAST custom element → FilePathChip) =====

interface FileChipHastProps {
  rawPath: string
  label: string
  line?: number
  col?: number
}

function FileChipFromHast(props: FileChipHastProps & { sessionId: string | null }): React.ReactElement {
  const resolution = useFileChipResolver(props.rawPath, props.sessionId)
  return (
    <FilePathChip
      rawPath={props.rawPath}
      label={props.label}
      state={resolution.state}
      mountId={resolution.mountId}
      relPath={resolution.relPath}
      absolutePath={resolution.absolutePath}
      sessionId={props.sessionId}
      line={props.line}
      col={props.col}
    />
  )
}

const MARKDOWN_COMPONENTS = {
  a: MarkdownLink,
  pre: MarkdownPre,
  code: MarkdownInlineCode,
  h1: MarkdownH1,
  h2: MarkdownH2,
  h3: MarkdownH3,
  table: MarkdownTable,
  thead: MarkdownThead,
  tr: MarkdownTr,
  th: MarkdownTh,
  td: MarkdownTd,
  blockquote: MarkdownBlockquote,
  hr: MarkdownHr,
  strong: MarkdownStrong,
  em: MarkdownEm,
} as const

interface MessageResponseProps {
  children: React.ReactNode
  className?: string
  /** 是否在 text 节点中保留换行（user 消息常用） */
  preserveBreaks?: boolean
  /** Session ID for chip workspace resolution. Pass null for workspace-only resolution. */
  sessionId?: string | null
}

/**
 * 使用 react-markdown 渲染 assistant 消息内容。
 * 支持 GFM、数学公式（KaTeX）、代码语法高亮（Shiki）。
 *
 * Agent 回复内容平铺完整显示（thinking block 由上层控制折叠）。
 */
export const MessageResponse = React.memo(
  function MessageResponse({ children, className, preserveBreaks = false, sessionId = null }: MessageResponseProps): React.ReactElement {
    useChipCacheInvalidator()
    const remarkPlugins = preserveBreaks ? REMARK_PLUGINS_WITH_BREAKS : REMARK_PLUGINS
    const content = typeof children === 'string' ? children : ''

    const components = React.useMemo(
      () => ({
        ...MARKDOWN_COMPONENTS,
        'file-path-chip': (chipProps: FileChipHastProps) => (
          <FileChipFromHast {...chipProps} sessionId={sessionId} />
        ),
      }),
      [sessionId],
    )

    return (
      <div
        className={cn(
          'chat-content prose prose-sm dark:prose-invert max-w-none',
          'prose-p:my-1.5 prose-p:leading-[1.65]',
          'prose-pre:my-0 prose-pre:bg-transparent prose-pre:p-0',
          'prose-a:text-primary prose-a:no-underline hover:prose-a:underline',
          // `<strong>` styling moved to the MarkdownStrong component
          // override (font-medium + inherit color), so it applies inside
          // `not-prose` table cells too. Don't restore prose-strong:*
          // here — they would re-introduce color contrast inside
          // blockquotes.
          '[&>*:first-child]:mt-0 [&>*:last-child]:mb-0',
          className,
        )}
      >
        {content ? (
          <Markdown
            remarkPlugins={remarkPlugins}
            rehypePlugins={REHYPE_PLUGINS}
            urlTransform={defaultUrlTransform}
            components={components as ComponentProps<typeof Markdown>['components']}
          >
            {content}
          </Markdown>
        ) : (
          typeof children !== 'string' ? children : null
        )}
      </div>
    )
  },
  (prev, next) => prev.children === next.children && prev.preserveBreaks === next.preserveBreaks && prev.className === next.className && prev.sessionId === next.sessionId,
)

/** 用户消息：通过 MessageResponse 渲染 markdown，但保留换行 */
/** A leading `/<command>` rendered as the same "command pill" the composer shows
 *  (hexagon + bare name), so a sent slash-command message reads consistently with
 *  what the user typed. The wire text keeps the `/`. */
function SlashCommandPill({ name }: { name: string }): React.ReactElement {
  return (
    <span className="inline-flex items-center gap-1 px-2 py-[1px] rounded-full text-[12px] leading-[1.5] align-baseline font-medium bg-muted text-blue-600 dark:text-blue-400 border border-border/50">
      <Hexagon size={12} className="shrink-0" aria-hidden="true" />
      {name}
    </span>
  )
}

/** Split a leading `/<command>` (a single word at the very start) from the rest,
 *  mirroring the backend's slash intercept. No leading slash ⇒ command is null. */
function splitLeadingSlashCommand(text: string): { command: string | null; rest: string } {
  const m = /^\s*\/([A-Za-z][\w-]*)(?:\s+([\s\S]*))?$/.exec(text)
  if (!m) return { command: null, rest: text }
  return { command: m[1], rest: m[2] ?? '' }
}

export function UserMessageContent({ children }: { children: React.ReactNode }): React.ReactElement {
  if (typeof children !== 'string') {
    return <div className="chat-content text-[15px] leading-relaxed whitespace-pre-wrap break-words">{children}</div>
  }
  // A sent `/<command> …` message renders the command as a pill + the rest as
  // plain text (slash-command messages are plain — markdown there is rare).
  const { command, rest } = splitLeadingSlashCommand(children)
  if (command) {
    return (
      <div className="chat-content text-[15px] leading-relaxed whitespace-pre-wrap break-words">
        <SlashCommandPill name={command} />
        {rest ? <span>{' '}{rest}</span> : null}
      </div>
    )
  }
  return (
    <MessageResponse preserveBreaks className="break-words">
      {children}
    </MessageResponse>
  )
}

// ===== 流式辅助组件 =====

export function MessageLoading({ startedAt }: { startedAt?: number }): React.ReactElement {
  return (
    <div className="flex items-center gap-1.5 py-1">
      <div className="flex gap-0.5">
        <div className="size-1.5 rounded-full bg-foreground/30 animate-pulse" />
        <div className="size-1.5 rounded-full bg-foreground/30 animate-pulse" style={{ animationDelay: '150ms' }} />
        <div className="size-1.5 rounded-full bg-foreground/30 animate-pulse" style={{ animationDelay: '300ms' }} />
      </div>
    </div>
  )
}

export function StreamingIndicator(): React.ReactElement {
  return (
    <span className="inline-block ml-0.5 w-2 h-4 bg-foreground/40 animate-pulse rounded-sm" />
  )
}

export function MessageStopped(): React.ReactElement {
  return (
    <div className="text-sm text-foreground/40 italic">
      已中止生成
    </div>
  )
}

export function MessageAttachments({ attachments }: { attachments: Array<{ filename: string; mediaType: string; localPath: string }> }): React.ReactElement {
  return (
    <div className="flex flex-wrap gap-2 mt-2">
      {attachments.map((att, i) => (
        <div key={i} className="text-xs text-muted-foreground bg-muted/50 rounded px-2 py-1">
          {att.filename}
        </div>
      ))}
    </div>
  )
}
