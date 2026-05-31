/**
 * SkillConsolidationDialog — preview & apply a LLM-proposed merge plan
 * for learned skills. Opened from the Kaleidoscope Skills module.
 */

import * as React from 'react'
import Markdown from 'react-markdown'
import { ChevronDown, ChevronRight, ArrowRight } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import type {
  SkillConsolidationCluster,
  SkillConsolidationProposal,
} from '@/lib/tauri-bridge'
import { cn } from '@/lib/utils'
import { useSkillConsolidation } from '../hooks/useSkillConsolidation'

interface SkillConsolidationDialogProps {
  open: boolean
  proposal: SkillConsolidationProposal | null
  onOpenChange: (open: boolean) => void
  onApplied: () => void
}

function MarkdownBlock({ text }: { text: string }): React.ReactElement {
  return (
    <div className="prose prose-sm max-w-none text-[12px] text-foreground/90
                    prose-p:my-1 prose-ul:my-1 prose-ol:my-1 prose-li:my-0
                    prose-headings:text-foreground prose-strong:text-foreground
                    prose-code:text-foreground prose-code:bg-muted prose-code:px-1 prose-code:rounded">
      <Markdown>{text}</Markdown>
    </div>
  )
}

function Section({ label, children }: { label: string; children: React.ReactNode }): React.ReactElement {
  return (
    <div>
      <div className="mb-1 text-[10.5px] font-semibold uppercase tracking-wider text-muted-foreground/70">
        {label}
      </div>
      {children}
    </div>
  )
}

interface ClusterCardProps {
  cluster: SkillConsolidationCluster
  expanded: boolean
  onToggle: () => void
}

function ClusterCard({ cluster, expanded, onToggle }: ClusterCardProps): React.ReactElement {
  const mergedCount = cluster.duplicateIds.length + 1
  return (
    <div className="rounded-lg border border-border/60 bg-card">
      <button
        type="button"
        onClick={onToggle}
        className="w-full flex items-start gap-2 px-3 py-2.5 text-left hover:bg-muted/30 rounded-t-lg"
      >
        <div className="flex-shrink-0 mt-0.5 text-muted-foreground/60">
          {expanded ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <div className="text-[13px] font-semibold text-foreground truncate">
              {cluster.mergedTitle}
            </div>
            <span className="flex-shrink-0 text-[10.5px] text-muted-foreground/70 tabular-nums px-1.5 py-0.5 rounded bg-muted/60">
              {mergedCount} 合 1
            </span>
          </div>
          {cluster.reason && (
            <div className="mt-1 text-[11.5px] leading-relaxed text-muted-foreground/80 line-clamp-2">
              {cluster.reason}
            </div>
          )}
        </div>
      </button>

      {expanded && (
        <div className="border-t border-border/40 px-4 py-3 space-y-3">
          {/* Mapping */}
          <Section label="合并映射">
            <div className="space-y-1.5">
              <div className="flex items-center gap-2 text-[12px]">
                <span className="flex-shrink-0 text-[10px] font-semibold px-1.5 py-0.5 rounded bg-emerald-500/15 text-emerald-700 dark:text-emerald-400">
                  保留
                </span>
                <span className="text-foreground/90 truncate">{cluster.canonicalTitle}</span>
              </div>
              {cluster.duplicateTitles.map((title, i) => (
                <div key={cluster.duplicateIds[i] ?? i} className="flex items-center gap-2 text-[12px]">
                  <span className="flex-shrink-0 text-[10px] font-semibold px-1.5 py-0.5 rounded bg-destructive/15 text-destructive">
                    将弃用
                  </span>
                  <span className="text-muted-foreground line-through truncate">{title}</span>
                  <ArrowRight className="size-3 text-muted-foreground/50 flex-shrink-0" />
                  <span className="text-foreground/80 truncate">{cluster.mergedTitle}</span>
                </div>
              ))}
            </div>
          </Section>

          {cluster.reason && (
            <Section label="合并理由">
              <p className="text-[12px] leading-relaxed text-muted-foreground">{cluster.reason}</p>
            </Section>
          )}

          {/* 保留后内容（直接使用 canonical 的现有内容；后续可在卡片里手动编辑） */}
          {cluster.mergedContext && (
            <Section label="保留后场景">
              <p className="text-[12px] leading-relaxed text-muted-foreground">{cluster.mergedContext}</p>
            </Section>
          )}
          {cluster.mergedPrinciples && (
            <Section label="保留后原则">
              <MarkdownBlock text={cluster.mergedPrinciples} />
            </Section>
          )}
          {cluster.mergedSteps && (
            <Section label="保留后步骤">
              <MarkdownBlock text={cluster.mergedSteps} />
            </Section>
          )}
          {cluster.mergedPitfalls && (
            <Section label="保留后陷阱">
              <MarkdownBlock text={cluster.mergedPitfalls} />
            </Section>
          )}
        </div>
      )}
    </div>
  )
}

export function SkillConsolidationDialog({
  open,
  proposal,
  onOpenChange,
  onApplied,
}: SkillConsolidationDialogProps): React.ReactElement | null {
  // State machine (expanded set, applying flag, reset/auto-close effects, apply
  // action) lives in the hook; the component stays pure presentation.
  const { expanded, applying, onToggle, onConfirm } = useSkillConsolidation({
    open,
    proposal,
    onOpenChange,
    onApplied,
  })

  if (!proposal) return null

  const clusterCount = proposal.clusters.length

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (applying) return
        onOpenChange(next)
      }}
    >
      <DialogContent className={cn('max-w-3xl max-h-[80vh] overflow-hidden flex flex-col gap-0 p-0')}>
        <DialogHeader className="px-6 pt-6 pb-4 border-b border-border/40">
          <DialogTitle>整合现有技能</DialogTitle>
          <DialogDescription>
            从 <span className="text-foreground font-medium tabular-nums">{proposal.totalSkills}</span> 条技能整合为{' '}
            <span className="text-foreground font-medium tabular-nums">{proposal.proposedCanonicalCount}</span> 条 ·
            共 <span className="text-foreground font-medium tabular-nums">{clusterCount}</span> 组合并
          </DialogDescription>
        </DialogHeader>

        <div className="flex-1 overflow-auto px-6 py-4 space-y-2.5">
          {proposal.clusters.map((cluster) => (
            <ClusterCard
              key={cluster.canonicalId}
              cluster={cluster}
              expanded={expanded.has(cluster.canonicalId)}
              onToggle={() => onToggle(cluster.canonicalId)}
            />
          ))}
        </div>

        <DialogFooter className="px-6 py-4 border-t border-border/40">
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={applying}
          >
            取消
          </Button>
          <Button onClick={() => void onConfirm()} disabled={applying}>
            {applying ? '整合中…' : `确认整合（${proposal.totalSkills} → ${proposal.proposedCanonicalCount}）`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
