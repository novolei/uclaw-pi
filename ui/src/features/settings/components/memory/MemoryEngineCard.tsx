/**
 * MemoryEngineCard — Settings › 记忆: the local retrieval engine.
 *
 * One headline + one sentence, the mode pill, the models toggle, then a single
 * state body (ready / downloading / failed / off / loading models) that
 * cross-fades on change, and the index summary. Technical details (model ids,
 * bytes, source host) stay in secondary text. Visual language:
 * docs/design/memory-ui.md.
 */
import * as React from 'react'
import { motion, useReducedMotion } from 'motion/react'
import {
  AlertCircle, CheckCircle2, ChevronRight, Circle, Database, RotateCw, Search,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { Button } from '@/components/ui/button'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { MemorySkeleton, ProgressBar, StatPill } from '@/components/memory/primitives'
import {
  MEMORY_MODELS,
  MEMORY_MODELS_TOTAL_BYTES,
  type MemoryCoreStatus,
} from '@/lib/bridge/memory-core'
import { SettingsToggle } from '../primitives'
import { CARD_CLASS } from '../primitives/SettingsUIConstants'
import {
  memoryEngineState,
  useMemoryCoreStatus,
  type MemoryEngineState,
} from '../../hooks/useMemoryCoreStatus'

// ── formatting ──────────────────────────────────────────────────────────────

const SOURCE_LABEL: Record<string, string> = {
  'huggingface.co': 'Hugging Face',
  'hf-mirror.com': '镜像站',
  'modelscope.cn': 'ModelScope',
}

/** Decimal units, matching how model sizes are usually quoted (568 MB, 1.14 GB). */
export function formatBytes(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(2)} GB`
  return `${Math.round(n / 1e6)} MB`
}

const numberFmt = new Intl.NumberFormat('zh-CN')

export function formatSince(iso: string | null, now: number): string {
  if (!iso) return '尚未同步'
  const t = Date.parse(iso)
  if (Number.isNaN(t)) return '尚未同步'
  const sec = Math.max(0, (now - t) / 1000)
  if (sec < 60) return '刚刚同步'
  const min = Math.floor(sec / 60)
  if (min < 60) return `上次同步 ${min} 分钟前`
  const hr = Math.floor(min / 60)
  if (hr < 24) return `上次同步 ${hr} 小时前`
  return `上次同步 ${Math.floor(hr / 24)} 天前`
}

/** `Date.now()` that refreshes every `ms` (relative timestamps). */
function useNow(ms: number): number {
  const [now, setNow] = React.useState(() => Date.now())
  React.useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), ms)
    return () => clearInterval(id)
  }, [ms])
  return now
}

/** True only once `active` has been true for `delay` ms — avoids skeleton flashes. */
function useDelayed(active: boolean, delay: number): boolean {
  const [shown, setShown] = React.useState(false)
  React.useEffect(() => {
    if (!active) {
      setShown(false)
      return
    }
    const id = setTimeout(() => setShown(true), delay)
    return () => clearTimeout(id)
  }, [active, delay])
  return shown
}

const pct = (v: number): number => Math.round(Math.min(1, Math.max(0, v)) * 100)

// ── shared bits ─────────────────────────────────────────────────────────────

const MODEL_NAMES: Record<string, string> = {
  embedder: '语义理解模型',
  reranker: '精排模型',
}

const EASE_OUT = [0.16, 1, 0.3, 1] as const

function FadeIn({ children, className }: { children: React.ReactNode; className?: string }) {
  const reduce = useReducedMotion()
  return (
    <motion.div
      className={className}
      initial={{ opacity: 0, y: reduce ? 0 : 4 }}
      animate={{ opacity: 1, y: 0, transition: { duration: reduce ? 0.12 : 0.18, ease: EASE_OUT } }}
    >
      {children}
    </motion.div>
  )
}

type RowState = 'done' | 'active' | 'waiting' | 'idle'

function ModelRow({
  name,
  detail,
  state,
  trailing,
}: {
  name: string
  detail: string
  state: RowState
  trailing: React.ReactNode
}) {
  return (
    <li className="flex items-center gap-2.5 py-2">
      <span aria-hidden className="flex size-4 shrink-0 items-center justify-center">
        {state === 'done' ? (
          <CheckCircle2 size={14} className="text-success" />
        ) : state === 'active' ? (
          <span className="relative flex size-2">
            <span className="absolute inset-0 rounded-full bg-progress-fill/40 motion-safe:animate-ping" />
            <span className="relative size-2 rounded-full bg-progress-fill" />
          </span>
        ) : (
          <Circle size={14} className="text-muted-foreground/50" />
        )}
      </span>
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px] font-medium leading-5 text-foreground">{name}</div>
        <div className="truncate text-[11px] leading-4 text-muted-foreground">{detail}</div>
      </div>
      <span
        className={cn(
          'shrink-0 text-[12px] leading-[18px] tabular-nums',
          state === 'active' ? 'text-foreground' : 'text-muted-foreground',
        )}
      >
        {trailing}
      </span>
    </li>
  )
}

const modelDetail = (m: (typeof MEMORY_MODELS)[number]) => `${m.name} · int8 · ${formatBytes(m.bytes)}`

// ── state bodies ────────────────────────────────────────────────────────────

function ReadyBody({ status }: { status: MemoryCoreStatus }) {
  return (
    <ul aria-label="本地模型" className="-my-1">
      {MEMORY_MODELS.map((m) => {
        const loaded = m.role === 'embedder' ? status.embedderLoaded : status.rerankerLoaded
        return (
          <ModelRow
            key={m.id}
            name={MODEL_NAMES[m.role]}
            detail={modelDetail(m)}
            state={loaded ? 'done' : 'idle'}
            trailing={loaded ? '已就绪' : '未加载'}
          />
        )
      })}
    </ul>
  )
}

function DownloadingBody({ status, queued }: { status: MemoryCoreStatus; queued: boolean }) {
  const d = status.download
  const total = d?.totalBytes || MEMORY_MODELS_TOTAL_BYTES
  const downloaded = d?.downloadedBytes ?? 0
  const overall = pct(downloaded / total)
  const source = d?.source ? (SOURCE_LABEL[d.source] ?? d.source) : null

  let offset = 0
  const rows = MEMORY_MODELS.map((m) => {
    const frac = (downloaded - offset) / m.bytes
    offset += m.bytes
    const state: RowState = frac >= 1 ? 'done' : !queued && d?.model === m.id ? 'active' : 'waiting'
    return { m, state, frac }
  })

  return (
    <div>
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-[13px] font-medium leading-5 text-foreground">
          {queued ? '正在准备下载' : '正在下载模型'}
        </span>
        {!queued && (
          <span className="text-[12px] leading-[18px] tabular-nums text-foreground">{overall}%</span>
        )}
      </div>
      <ProgressBar
        label="下载记忆模型"
        value={downloaded / total}
        indeterminate={queued}
        className="mt-2"
      />
      <p className="mt-1.5 text-[12px] leading-[18px] tabular-nums text-muted-foreground">
        {queued
          ? `共 ${formatBytes(total)} · 正在选择最快的下载源`
          : `${formatBytes(downloaded)} / ${formatBytes(total)}${source ? ` · 来自 ${source}` : ''}`}
      </p>
      <ul aria-label="下载进度" className="mt-2">
        {rows.map(({ m, state, frac }) => (
          <ModelRow
            key={m.id}
            name={MODEL_NAMES[m.role]}
            detail={modelDetail(m)}
            state={state}
            trailing={state === 'done' ? '已下载' : state === 'active' ? `${pct(frac)}%` : '等待中'}
          />
        ))}
      </ul>
      <p className="mt-1 text-[11px] leading-4 text-muted-foreground">
        下载期间仍可按关键词检索，完成后自动切换。
      </p>
    </div>
  )
}

/** Turn the backend's error chain into a short cause the user can act on. */
export function describeDownloadError(reason: string | null): string {
  const r = (reason ?? '').toLowerCase()
  if (/no space|disk full|enospc|磁盘/.test(r)) return '磁盘空间不足，请清理出约 1.2 GB 后重试。'
  if (/permission denied|eacces|read-only/.test(r)) return '没有写入模型目录的权限，请检查后重试。'
  if (/timed out|timeout|error sending request|dns|connect|network|unreachable|tls|certificate/.test(r)) {
    return '已依次尝试 Hugging Face、镜像站和 ModelScope，均未连通。请检查网络后重试。'
  }
  return '下载未完成，请稍后重试。'
}

function FailedBody({
  reason,
  onRetry,
  busy,
}: {
  reason: string | null
  onRetry: () => void
  busy: boolean
}) {
  return (
    <div role="alert" className="flex gap-2.5 rounded-lg border border-danger/20 bg-danger-bg/50 px-3 py-3">
      <AlertCircle aria-hidden size={14} className="mt-[3px] shrink-0 text-danger" />
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-medium leading-5 text-danger">模型下载失败</div>
        <p className="mt-0.5 text-[12px] leading-[18px] text-muted-foreground">
          {describeDownloadError(reason)}目前仍可按关键词检索。
        </p>
        <div className="mt-2.5 flex flex-wrap items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={onRetry}
            disabled={busy}
            className="h-7 gap-1.5 bg-background/60 px-2.5 text-[12px]"
          >
            <RotateCw aria-hidden className={cn('!size-3', busy && 'motion-safe:animate-spin')} />
            重试
          </Button>
        </div>
        {reason && <ErrorDetails reason={reason} />}
      </div>
    </div>
  )
}

/** Raw error behind a small disclosure; wraps and scrolls, never truncated. */
function ErrorDetails({ reason }: { reason: string }) {
  return (
    <Collapsible className="mt-2">
      <CollapsibleTrigger
        className={cn(
          'group -ml-1 inline-flex items-center gap-0.5 rounded px-1 py-0.5 text-[11px] leading-4',
          'text-muted-foreground transition-colors duration-[120ms] hover:text-foreground',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
        )}
      >
        <ChevronRight
          aria-hidden
          size={12}
          className="transition-transform duration-[180ms] group-data-[state=open]:rotate-90 motion-reduce:transition-none"
        />
        详情
      </CollapsibleTrigger>
      <CollapsibleContent>
        <pre className="mt-1 max-h-32 overflow-auto whitespace-pre-wrap break-words rounded-md bg-background/60 px-2 py-1.5 font-mono text-[11px] leading-4 text-muted-foreground">
          {reason}
        </pre>
      </CollapsibleContent>
    </Collapsible>
  )
}

function OffBody() {
  return (
    <div className="flex gap-2.5 rounded-lg bg-muted/50 px-3 py-3">
      <Search aria-hidden size={14} className="mt-[3px] shrink-0 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-medium leading-5 text-foreground">当前仅按关键词找回记忆</div>
        <p className="mt-0.5 text-[12px] leading-[18px] text-muted-foreground">
          开启后，助手能理解意思相近的说法：问“出差安排”，也能想起“下周去上海开会”。
          首次开启会下载约 1.1 GB 模型，之后完全在本机运行。
        </p>
      </div>
    </div>
  )
}

function LoadingModelsBody() {
  return (
    <div>
      <span className="text-[13px] font-medium leading-5 text-foreground">正在加载模型</span>
      <ProgressBar label="加载记忆模型" indeterminate className="mt-2" />
      <p className="mt-1.5 text-[12px] leading-[18px] text-muted-foreground">
        模型已在本机，正在载入，通常只需几秒。
      </p>
    </div>
  )
}

// ── index section ───────────────────────────────────────────────────────────

function IndexSection({ status }: { status: MemoryCoreStatus }) {
  const now = useNow(30_000)
  const { docs, vectors, pendingEmbeddings, lastSyncAt } = status.index
  const showCoverage = status.modelsEnabled && (status.embedderLoaded || vectors > 0) && docs > 0
  const coverage = docs > 0 ? vectors / docs : 0

  return (
    <div>
      <div className="flex items-baseline justify-between gap-3">
        <h4 className="text-[13px] font-medium leading-5 text-foreground">记忆索引</h4>
        <span
          className="shrink-0 text-[11px] leading-4 tabular-nums text-muted-foreground"
          title={lastSyncAt ? new Date(lastSyncAt).toLocaleString('zh-CN') : undefined}
        >
          {formatSince(lastSyncAt, now)}
        </span>
      </div>

      {docs === 0 ? (
        <div className="flex flex-col items-center px-4 py-5 text-center">
          <Database aria-hidden size={20} className="text-muted-foreground/60" />
          <div className="mt-2 text-[13px] font-medium leading-5 text-foreground">还没有可检索的记忆</div>
          <p className="mt-0.5 text-[12px] leading-[18px] text-muted-foreground">
            和助手聊几句后，值得记住的内容会自动整理到这里。
          </p>
        </div>
      ) : showCoverage ? (
        <div className="mt-2.5">
          <div className="flex items-baseline justify-between gap-3 text-[12px] leading-[18px]">
            <span className="text-muted-foreground">语义覆盖</span>
            <span className="tabular-nums text-foreground">
              {numberFmt.format(vectors)} / {numberFmt.format(docs)} 条
              <span className="text-muted-foreground"> · {pct(coverage)}%</span>
            </span>
          </div>
          <ProgressBar label="语义覆盖" value={coverage} className="mt-1.5" />
          {pendingEmbeddings > 0 && (
            <p className="mt-1.5 text-[12px] leading-[18px] tabular-nums text-muted-foreground">
              还有 {numberFmt.format(pendingEmbeddings)} 条正在建立语义索引
            </p>
          )}
        </div>
      ) : (
        <>
          <div className="mt-2.5 flex flex-wrap gap-2">
            <StatPill label="记忆" value={numberFmt.format(docs)} />
          </div>
          <p className="mt-2 text-[12px] leading-[18px] tabular-nums text-muted-foreground">
            {pendingEmbeddings === 0
              ? '开启本地模型后即可按意思检索这些记忆'
              : status.modelsEnabled
                ? `模型就绪后将为 ${numberFmt.format(pendingEmbeddings)} 条记忆建立语义索引`
                : `开启本地模型后，将为 ${numberFmt.format(pendingEmbeddings)} 条记忆建立语义索引`}
          </p>
        </>
      )}
    </div>
  )
}

// ── skeleton / load error ───────────────────────────────────────────────────

function EngineSkeleton() {
  return (
    <div data-testid="memory-engine-skeleton" role="status" aria-busy="true">
      <span className="sr-only">正在加载</span>
      <div className="flex items-center justify-between border-t border-border/50 px-4 py-3.5">
        <div className="space-y-1.5">
          <MemorySkeleton className="h-3.5 w-28" />
          <MemorySkeleton className="h-3 w-40" />
        </div>
        <MemorySkeleton className="h-5 w-9 rounded-full" />
      </div>
      <div className="space-y-3 px-4 pb-4 pt-1">
        {[0, 1].map((i) => (
          <div key={i} className="flex items-center gap-2.5">
            <MemorySkeleton className="size-4 rounded-full" />
            <div className="flex-1 space-y-1.5">
              <MemorySkeleton className="h-3 w-24" />
              <MemorySkeleton className="h-2.5 w-36" />
            </div>
            <MemorySkeleton className="h-3 w-10" />
          </div>
        ))}
      </div>
      <div className="space-y-2.5 border-t border-border/50 px-4 py-4">
        <MemorySkeleton className="h-3 w-16" />
        <MemorySkeleton className="h-6 w-24 rounded-full" />
      </div>
    </div>
  )
}

function LoadError({ error, onRetry }: { error: string; onRetry: () => void }) {
  return (
    <div className="border-t border-border/50 px-4 py-4">
      <div role="alert" className="flex gap-2.5 rounded-lg border border-danger/20 bg-danger-bg/50 px-3 py-3">
        <AlertCircle aria-hidden size={14} className="mt-[3px] shrink-0 text-danger" />
        <div className="min-w-0 flex-1">
          <div className="text-[13px] font-medium leading-5 text-danger">无法读取检索引擎状态</div>
          <p className="mt-0.5 text-[12px] leading-[18px] text-muted-foreground">
            请稍后重试；若持续出现，可在系统诊断中查看服务状态。
          </p>
          <Button
            variant="outline"
            size="sm"
            onClick={onRetry}
            className="mt-2.5 h-7 gap-1.5 bg-background/60 px-2.5 text-[12px]"
          >
            <RotateCw aria-hidden className="!size-3" />
            重试
          </Button>
          <ErrorDetails reason={error} />
        </div>
      </div>
    </div>
  )
}

// ── card ────────────────────────────────────────────────────────────────────

export function MemoryEngineCard(): React.ReactElement {
  const { status, loading, error, pending, setModelsEnabled, retry, reload } = useMemoryCoreStatus()
  const showSkeleton = useDelayed(loading, 150)
  const state: MemoryEngineState | null = status ? memoryEngineState(status) : null
  const hybrid = status?.mode === 'hybrid'

  return (
    <section
      aria-labelledby="memory-engine-title"
      className={cn(CARD_CLASS, 'shadow-elevation-1')}
    >
      <header className="flex items-start gap-3 px-4 pb-3.5 pt-4">
        <div className="min-w-0 flex-1">
          <h3
            id="memory-engine-title"
            className="text-[15px] font-semibold leading-[22px] text-foreground"
          >
            本地检索引擎
          </h3>
          <p className="mt-0.5 text-[12px] leading-[18px] text-muted-foreground">
            让助手按意思而不只是按字词找回记忆，模型在本机运行，不上传数据
          </p>
        </div>
        {status && (
          <StatPill
            label="模式"
            value={hybrid ? '混合检索' : '仅关键词'}
            tone={hybrid ? 'success' : 'neutral'}
            className="mt-0.5"
            title={hybrid ? '关键词 + 语义向量，再经精排' : '全文关键词检索'}
          />
        )}
      </header>

      {status && state ? (
        <>
          <div className="border-t border-border/50">
            <SettingsToggle
              label="使用本地检索模型"
              checked={status.modelsEnabled}
              onCheckedChange={(v) => void setModelsEnabled(v)}
              disabled={pending}
            />
          </div>

          <div className="px-4 pb-4">
            {/* Keyed remount = enter-only fade (180 ms); no exit, so a swap never exceeds 200 ms. */}
            <FadeIn key={state === 'queued' ? 'downloading' : state}>
              {state === 'ready' && <ReadyBody status={status} />}
              {(state === 'downloading' || state === 'queued') && (
                <DownloadingBody status={status} queued={state === 'queued'} />
              )}
              {state === 'failed' && (
                <FailedBody
                  reason={status.download?.error ?? null}
                  onRetry={() => void retry()}
                  busy={pending}
                />
              )}
              {state === 'off' && <OffBody />}
              {state === 'loading' && <LoadingModelsBody />}
            </FadeIn>
          </div>

          <div className="border-t border-border/50 px-4 py-4">
            <IndexSection status={status} />
          </div>
        </>
      ) : error ? (
        <LoadError error={error} onRetry={() => void reload()} />
      ) : showSkeleton ? (
        <EngineSkeleton />
      ) : (
        // Reserve the skeleton's height during the 150 ms grace period.
        <div aria-busy="true" className="h-[214px]" />
      )}
    </section>
  )
}
