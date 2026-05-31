/**
 * Compaction indicators — the two context-compaction system-message affordances
 * rendered inline in the agent message stream:
 *  - `CompactBoundaryDivider`: a settled "上下文已压缩" divider (optionally with counts)
 *  - `CompactingIndicator`: the in-progress "正在压缩..." spinner divider
 *
 * Salvaged from the former `components/agent/SDKMessageRenderer.tsx` — the only
 * two live exports of that file. The rest (the SDK-message renderer / turn-grouping
 * machinery) was dead Proma-migration skeleton and was removed; the live agent
 * stream renders through `AgentMessages` + `ContentBlock` directly.
 */

import * as React from 'react'
import { Loader2 } from 'lucide-react'

// ===== system 消息：上下文压缩分割线 =====

export function CompactBoundaryDivider({ removed, remaining }: { removed?: number; remaining?: number }): React.ReactElement {
  const hasInfo = removed != null && remaining != null
  return (
    <div className="flex items-center gap-3 my-4 px-1">
      <div className="flex-1 h-px bg-border/40" />
      <span className="shrink-0 inline-flex items-center gap-1.5 text-[11px] text-muted-foreground/60 px-2 py-0.5 rounded-full border border-border/30 bg-muted/20">
        上下文已压缩
        {hasInfo && removed > 0 && (
          <span className="text-[10px] text-muted-foreground/40">
            · {removed} 条已压缩 · 保留 {remaining} 条
          </span>
        )}
      </span>
      <div className="flex-1 h-px bg-border/40" />
    </div>
  )
}

// ===== system 消息：正在压缩指示器（与 CompactBoundaryDivider 同款横线样式，pill 内带 spinner） =====

export function CompactingIndicator(): React.ReactElement {
  return (
    <div className="flex items-center gap-3 my-4 px-1">
      <div className="flex-1 h-px bg-border/40" />
      <span className="shrink-0 inline-flex items-center gap-1.5 text-[11px] text-muted-foreground/70 px-2 py-0.5 rounded-full border border-border/30 bg-muted/20">
        <Loader2 className="size-3 animate-spin" />
        正在压缩...
      </span>
      <div className="flex-1 h-px bg-border/40" />
    </div>
  )
}
