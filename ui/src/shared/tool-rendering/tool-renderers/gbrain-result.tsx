import * as React from 'react'
import { AlertTriangle, Database, Lightbulb, Search } from 'lucide-react'
import { CollapsibleResult } from './collapsible-result'

interface Props {
  result: string
  isError: boolean
}

interface GbrainErrorPayload {
  ok?: boolean
  source?: string
  tool?: string
  kind?: string
  status?: string
  message?: string
  hint?: string
  nearest_slugs?: string[]
}

const KIND_LABELS: Record<string, string> = {
  page_not_found: '页面不存在',
  process_killed: '进程被系统终止',
  timeout: 'CLI 超时',
  pglite_lock_timeout: 'PGLite 锁超时',
  pglite_not_ready: 'PGLite 未就绪',
  permission_denied: '权限不足',
  path_mismatch: '运行路径不一致',
  launcher_missing_or_unusable: '启动器不可用',
  unknown: '调用失败',
}

function extractJsonObject(raw: string): GbrainErrorPayload | null {
  const trimmed = raw.trim()
  const start = trimmed.indexOf('{')
  const end = trimmed.lastIndexOf('}')
  if (start < 0 || end <= start) return null

  try {
    const value = JSON.parse(trimmed.slice(start, end + 1)) as unknown
    if (
      value &&
      typeof value === 'object' &&
      !Array.isArray(value) &&
      (value as GbrainErrorPayload).source === 'gbrain'
    ) {
      return value as GbrainErrorPayload
    }
  } catch {
    return null
  }
  return null
}

function normalizeLegacyNearestSlugs(raw: string): GbrainErrorPayload | null {
  if (!raw.includes('nearest slugs:')) return null
  const nearest = raw
    .split('nearest slugs:')
    .pop()
    ?.split(',')
    .map((slug) => slug.trim())
    .filter(Boolean)
    .slice(0, 5) ?? []
  return {
    source: 'gbrain',
    ok: false,
    kind: raw.includes('page_not_found') ? 'page_not_found' : 'unknown',
    message: raw.includes('page_not_found') ? 'gbrain page not found' : 'gbrain CLI failed',
    hint: 'Pick an existing slug from the suggestions or retry with fuzzy=true/include_deleted=true.',
    nearest_slugs: nearest,
  }
}

export function GbrainResultRenderer({ result, isError }: Props): React.ReactElement {
  const payload = extractJsonObject(result) ?? normalizeLegacyNearestSlugs(result)

  if (!payload || !isError) {
    return (
      <CollapsibleResult charThreshold={3000} previewLines={15}>
        <pre className="whitespace-pre-wrap break-all text-xs px-3 py-2 rounded-md text-muted-foreground bg-muted/20">
          {result}
        </pre>
      </CollapsibleResult>
    )
  }

  const kind = payload.kind ?? 'unknown'
  const nearest = payload.nearest_slugs ?? []

  return (
    <div className="rounded-md border border-destructive/20 bg-destructive/5 p-3 text-xs">
      <div className="flex items-start gap-2">
        <AlertTriangle className="mt-0.5 size-4 text-destructive flex-shrink-0" />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-medium text-destructive">
              gbrain: {KIND_LABELS[kind] ?? kind}
            </span>
            {payload.tool && (
              <span className="rounded bg-muted px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">
                {payload.tool}
              </span>
            )}
            {payload.status && (
              <span className="rounded bg-background/70 px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">
                {payload.status}
              </span>
            )}
          </div>

          {payload.message && (
            <p className="mt-1 text-muted-foreground">{payload.message}</p>
          )}

          {payload.hint && (
            <div className="mt-2 flex items-start gap-1.5 text-muted-foreground">
              <Lightbulb className="mt-0.5 size-3.5 flex-shrink-0" />
              <span>{payload.hint}</span>
            </div>
          )}

          {nearest.length > 0 && (
            <div className="mt-2">
              <div className="mb-1 flex items-center gap-1 text-muted-foreground">
                <Search className="size-3.5" />
                <span>候选 slug</span>
              </div>
              <div className="flex flex-wrap gap-1.5">
                {nearest.map((slug) => (
                  <span
                    key={slug}
                    className="rounded bg-background px-2 py-1 font-mono text-[11px] text-foreground border border-border/60"
                  >
                    {slug}
                  </span>
                ))}
              </div>
            </div>
          )}

          <div className="mt-2 flex items-center gap-1 text-muted-foreground">
            <Database className="size-3.5" />
            <span>更多运行状态可在系统诊断中查看。</span>
          </div>
        </div>
      </div>
    </div>
  )
}
