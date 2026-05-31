/**
 * useContextUsage — the context-usage computation behind ContextUsageBadge.
 * Extracted during the features/agent migration so the badge stays a thin
 * presentational shell + the popover stays purely presentational.
 *
 * What it owns (behavior-preserving — lifted verbatim from the old component):
 *   - The "last valid value" cache (`stableRef`): when fresh usage arrives
 *     (`inputTokens > 0`) it snapshots every field; on a session switch where
 *     the new session hasn't reported usage yet, it falls back to that snapshot
 *     so the badge doesn't flicker away. `skillsTokens` is special-cased — it
 *     arrives via a separate `agent:context_stats` event, so the CURRENT prop
 *     is always preferred (not gated on `hasCurrent`), with the cache as
 *     fallback for session switch.
 *   - All derived metrics: pure-input, cache hit ratio, cache-saved-input,
 *     percent, ring ratio, warning state, compact threshold, total-sent.
 *
 * Returns `null` when there has never been usage data (badge renders nothing);
 * otherwise the full derived metric bundle the badge + popover consume. No IPC
 * here — this is pure derivation from props.
 */

import * as React from 'react'

/** 压缩阈值比例（SDK 在 ~77.5% 窗口大小时自动压缩） */
const COMPACT_THRESHOLD_RATIO = 0.775
/** 显示警告的阈值（压缩阈值的 80%） */
const WARNING_RATIO = 0.80

export interface ContextUsageInput {
  inputTokens?: number
  outputTokens?: number
  cacheReadTokens?: number
  cacheCreationTokens?: number
  costUsd?: number
  contextWindow?: number
  skillsTokens?: number
}

/** Fully-derived usage metrics consumed by the badge ring + the popover. */
export interface ContextUsageMetrics {
  /** Whether occupancy is near the auto-compact threshold (amber styling). */
  isWarning: boolean
  /** Ring fill ratio (displayTokens / displayWindow), 0 when no window known. */
  ratio: number
  /** Total context tokens used (fresh input + cache read + cache write). */
  displayTokens: number
  /** Context window size, if known. */
  displayWindow?: number
  /** Fresh (non-cached) input tokens. */
  pureInput: number
  /** Cache-write tokens, if any. */
  displayCacheCreation?: number
  /** Cache-read tokens, if any. */
  displayCacheRead?: number
  /** Output tokens, if any. */
  displayOutput?: number
  /** Skill manifest token cost (from agent:context_stats), if any. */
  displaySkills?: number
  /** This-turn cost in USD, if any. */
  displayCost?: number
  /** Total tokens sent this turn (fresh input + cache read + cache write). */
  totalSent: number
  /** Cache hit ratio percent (cache_read / total_sent), 0 when no reads. */
  cacheHitRatio: number
  /** Estimated input tokens saved by cache reads (~90% of cache reads). */
  cacheSavedInput: number
  /** Occupancy percent (displayTokens / displayWindow), undefined w/o window. */
  percent?: number
}

export function useContextUsage(input: ContextUsageInput): ContextUsageMetrics | null {
  const {
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheCreationTokens,
    costUsd,
    contextWindow,
    skillsTokens,
  } = input

  // 保留最近一次有效的 token 值，避免切换会话时闪烁消失
  const stableRef = React.useRef<{
    inputTokens: number
    outputTokens?: number
    cacheReadTokens?: number
    cacheCreationTokens?: number
    costUsd?: number
    contextWindow?: number
    skillsTokens?: number
  } | null>(null)
  if (inputTokens && inputTokens > 0) {
    stableRef.current = { inputTokens, outputTokens, cacheReadTokens, cacheCreationTokens, costUsd, contextWindow, skillsTokens }
  }

  // 使用稳定值：优先当前数据，回退到上次有效数据
  const stable = stableRef.current
  const hasCurrent = inputTokens != null && inputTokens > 0
  const displayTokens = hasCurrent ? inputTokens : stable?.inputTokens
  const displayWindow = hasCurrent ? contextWindow : stable?.contextWindow
  const displayOutput = hasCurrent ? outputTokens : stable?.outputTokens
  const displayCacheRead = hasCurrent ? cacheReadTokens : stable?.cacheReadTokens
  const displayCacheCreation = hasCurrent ? cacheCreationTokens : stable?.cacheCreationTokens
  const displayCost = hasCurrent ? costUsd : stable?.costUsd
  // skillsTokens comes in via the agent:context_stats event, which fires
  // independently of usage_update. Always prefer the current prop value
  // (don't gate on hasCurrent); fall back to stable cache for session switch.
  const displaySkills = skillsTokens != null ? skillsTokens : stable?.skillsTokens

  // 从未有过 usage 数据 → 不显示
  if (!displayTokens || displayTokens <= 0) return null

  // 警告阈值：基于压缩阈值（contextWindow × 0.775 × 80%）
  const compactThreshold = displayWindow
    ? Math.floor(displayWindow * COMPACT_THRESHOLD_RATIO)
    : undefined
  const isWarning = compactThreshold
    ? displayTokens / compactThreshold >= WARNING_RATIO
    : false

  const ratio = displayWindow ? displayTokens / displayWindow : 0

  // 纯输入 = 总上下文 - 缓存读取 - 缓存写入
  const pureInput = displayTokens - (displayCacheRead ?? 0) - (displayCacheCreation ?? 0)

  const percent = displayWindow
    ? Math.round((displayTokens / displayWindow) * 100)
    : undefined

  // 总发送 token 数 = 新鲜输入 + 缓存读取 + 缓存写入
  const totalSent = displayTokens + (displayCacheRead ?? 0) + (displayCacheCreation ?? 0)

  // 缓存命中率 — cache_read / total_sent。> 0 才显示。
  const cacheHitRatio = (displayCacheRead ?? 0) > 0
    ? Math.round(((displayCacheRead ?? 0) / totalSent) * 100)
    : 0

  // 等效"省下的"输入 token 估算：缓存读取以 10% 输入价计费（Anthropic 当前
  // 比例），所以省下的等效 input token ≈ cacheRead × 0.9。粗略展示用。
  const cacheSavedInput = Math.round((displayCacheRead ?? 0) * 0.9)

  return {
    isWarning,
    ratio,
    displayTokens,
    displayWindow,
    pureInput,
    displayCacheCreation,
    displayCacheRead,
    displayOutput,
    displaySkills,
    displayCost,
    totalSent,
    cacheHitRatio,
    cacheSavedInput,
    percent,
  }
}
