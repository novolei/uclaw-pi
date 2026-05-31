/**
 * EmptyState — the whole-tab placeholder shown when no preferences have been
 * learned yet. Split out of legacy settings/LearnedProfileTab.tsx during the
 * features/settings migration (code-organization ADR 2026-05-31). Behavior verbatim.
 */
import * as React from 'react'
import { UserCircle2 } from 'lucide-react'

export function EmptyState(): React.ReactElement {
  return (
    <div className="flex flex-col items-center gap-2 py-10 text-center px-4 border border-dashed border-border/50 rounded-md">
      <UserCircle2 className="size-8 text-muted-foreground/40" />
      <p className="text-xs text-muted-foreground">还没有学到任何偏好。</p>
      <p className="text-[10px] text-muted-foreground/60 max-w-prose">
        随着你和 uClaw 对话，提取器会从消息中提取候选事实（如 "我用 helix"、"我叫 Alice"），
        每 30 分钟根据证据稳定性把候选晋级为 active。
      </p>
    </div>
  )
}
