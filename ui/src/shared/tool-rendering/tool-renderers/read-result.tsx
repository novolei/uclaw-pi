import * as React from 'react'
import { File as PierreFile } from '@pierre/diffs/react'
import { usePierreTheme, detectLang } from './pierre-theme'
import { CollapsibleResult } from './collapsible-result'

interface Props {
  input: Record<string, unknown>
  result: string
  isError: boolean
}

/**
 * If the SDK injects line-number prefixes like "    1\tcontent",
 * strip them so Pierre renders clean source. Tolerant of files
 * that don't have this convention (pass-through).
 *
 * Handles both real tab characters (\t) and the literal two-character
 * sequence \t that some SDK serializers emit as escaped strings.
 * Also normalizes literal \n sequences to real newlines when stripping.
 */
function stripLinePrefixes(text: string): string {
  // Normalize literal \n sequences to real newlines to simplify splitting
  const normalized = text.includes('\n') ? text : text.replace(/\\n/g, '\n')
  const lines = normalized.split('\n')
  // Pattern matches spaces + digits + real tab OR literal \t
  const pattern = /^\s*\d+(\t|\\t)/
  if (lines.every((l) => l === '' || pattern.test(l))) {
    return lines.map((l) => l.replace(pattern, '')).join('\n')
  }
  return normalized
}

/**
 * read_file output (Dirac-B1) prepends a `[File Hash: 0x...]` header line and
 * prefixes every content line with a stable anchor token + the § delimiter,
 * e.g. `Apple§    def foo():` / `AppleCedar§...` / `Apple-1§...`. The anchor
 * tokens are an LLM-targeting aid, not source — strip them (and the header)
 * before handing clean source to the syntax highlighter.
 *
 * Conservative: only strips when the FIRST line is the `[File Hash:]` header,
 * so plain content (or other tools' output) passes through untouched.
 */
function stripAnchorPrefixes(text: string): string {
  const normalized = text.includes('\n') ? text : text.replace(/\\n/g, '\n')
  const lines = normalized.split('\n')
  if (lines.length === 0 || !/^\[File Hash: 0x[0-9a-fA-F]+\]/.test(lines[0])) {
    return text
  }
  // Drop the header line; strip `<token>§` from each remaining line. Token
  // shape mirrors the Rust validator: ^[A-Za-z]+(-\d+)? (1- or 2-word combo,
  // optional legacy `-N` collision suffix).
  const anchorPattern = /^[A-Za-z]+(?:-\d+)?§/
  return lines
    .slice(1)
    .map((l) => l.replace(anchorPattern, ''))
    .join('\n')
}

export function ReadResultRenderer({ input, result, isError }: Props): React.ReactElement {
  const path = (input.path as string | undefined) ?? ''
  const theme = usePierreTheme()

  if (isError) {
    return (
      <div className="rounded-md bg-destructive/5 text-destructive text-xs px-3 py-2 whitespace-pre-wrap break-all">
        {result || '读取失败'}
      </div>
    )
  }

  const content = stripLinePrefixes(stripAnchorPrefixes(result))
  return (
    <CollapsibleResult charThreshold={3000} previewLines={15}>
      <div className="rounded-md border border-border bg-content-area overflow-auto max-h-[400px]">
        <PierreFile
          file={{ name: path, contents: content, lang: detectLang(path) }}
          options={{ theme }}
        />
      </div>
    </CollapsibleResult>
  )
}
