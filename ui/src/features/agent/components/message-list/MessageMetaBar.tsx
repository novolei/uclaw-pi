/**
 * MessageMetaBar / DurationBadge — the per-message token-cost-duration badge.
 *
 * ⚠️ USER-VERIFIED SURFACE. This is the badge the user has personally checked:
 * the `$USD (¥CNY)` cost caption, the `CNY_PER_USD` rate (which MUST match the
 * backend `agent::types::CNY_PER_USD`), the thinking-aware single-line layout,
 * and the hover tooltip with the token breakdown. Extracted verbatim from
 * AgentMessages.tsx during the features/agent migration split — formatting is
 * byte-for-byte identical to the original. Do NOT "improve" it.
 */

import * as React from 'react'
import { Zap } from 'lucide-react'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { AgentEventUsage } from '@/lib/agent-types'

/** 格式化耗时（毫秒 → 可读字符串） */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`
  const seconds = ms / 1000
  if (seconds < 60) return `${seconds.toFixed(1)}s`
  const m = Math.floor(seconds / 60)
  const s = seconds % 60
  return `${m}m ${s.toFixed(0)}s`
}

/** 构建 usage tooltip 多行文本 */
export function buildUsageTooltip(durationMs: number, usage?: AgentEventUsage): string {
  const lines: string[] = []
  lines.push(`耗时: ${formatDuration(durationMs)}`)
  if (usage) {
    const pureInput = usage.inputTokens - (usage.cacheReadTokens ?? 0) - (usage.cacheCreationTokens ?? 0)
    if (pureInput > 0) lines.push(`输入: ${pureInput.toLocaleString()}`)
    if (usage.outputTokens) lines.push(`输出: ${usage.outputTokens.toLocaleString()}`)
    if (usage.cacheCreationTokens) lines.push(`缓存写入: ${usage.cacheCreationTokens.toLocaleString()}`)
    if (usage.cacheReadTokens) lines.push(`缓存读取: ${usage.cacheReadTokens.toLocaleString()}`)
  }
  return lines.join('\n')
}

/** 耗时徽章 — 悬浮显示 token 用量明细 */
export function DurationBadge({ durationMs, usage }: { durationMs: number; usage?: AgentEventUsage }): React.ReactElement {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="text-[12px] text-muted-foreground/50 tabular-nums cursor-default hover:text-muted-foreground/70 transition-colors">
          {formatDuration(durationMs)}
        </span>
      </TooltipTrigger>
      <TooltipContent side="top">
        <p className="whitespace-pre-line text-left">{buildUsageTooltip(durationMs, usage)}</p>
      </TooltipContent>
    </Tooltip>
  )
}

/** 统一消息元信息栏 — 耗时 + token 用量合并为单行，单一 tooltip */
/** CNY per USD — MUST match the backend (`agent::types::CNY_PER_USD`) so the ¥
 *  caption is the provider's native amount, not a second estimate. */
const CNY_PER_USD = 7.15

export function MessageMetaBar({ durationMs, usage }: { durationMs?: number; usage?: AgentEventUsage }): React.ReactElement | null {
  if (durationMs == null && usage == null) return null

  const parts: string[] = []
  if (durationMs != null) parts.push(formatDuration(durationMs))
  if (usage) {
    const { inputTokens, outputTokens, costUsd } = usage
    parts.push(`${inputTokens.toLocaleString()} 输入`)
    parts.push(`${(outputTokens ?? 0).toLocaleString()} 输出`)
    if (costUsd != null && costUsd > 0) {
      // USD primary + a ¥ caption (DeepSeek bills in 元). Uses the SAME rate as the
      // backend (agent::types::CNY_PER_USD), so ¥ is the provider's native amount
      // rather than a second estimate.
      const cny = costUsd * CNY_PER_USD
      parts.push(`$${costUsd.toFixed(4)} (¥${cny < 0.1 ? cny.toFixed(4) : cny.toFixed(2)})`)
    }
  }

  const tooltipText = durationMs != null ? buildUsageTooltip(durationMs, usage) : null

  const content = (
    <span className="inline-flex items-center gap-1 text-[12px] text-muted-foreground/50 tabular-nums cursor-default hover:text-muted-foreground/70 transition-colors animate-in fade-in duration-300">
      <Zap size={11} strokeWidth={2} className="shrink-0" />
      {parts.map((p, i) => (
        <React.Fragment key={i}>
          {i > 0 && <span className="opacity-40">·</span>}
          <span>{p}</span>
        </React.Fragment>
      ))}
    </span>
  )

  if (!tooltipText) return content

  return (
    <Tooltip>
      <TooltipTrigger asChild>{content}</TooltipTrigger>
      <TooltipContent side="top">
        <p className="whitespace-pre-line text-left">{tooltipText}</p>
      </TooltipContent>
    </Tooltip>
  )
}
