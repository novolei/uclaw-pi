// Owns the SkillConsolidationDialog state machine: which clusters are expanded,
// the applying flag, the reset-on-open + auto-close-when-empty effects, and the
// confirm/apply action. Extracted out of the component during the
// features/settings migration (code-organization ADR 2026-05-31). IPC stays in
// the typed `@/lib/tauri-bridge` applySkillConsolidation helper (precedent:
// useChannelSettings). The toast copy + error handling are preserved verbatim.
import * as React from 'react'
import { toast } from 'sonner'
import {
  applySkillConsolidation,
  type SkillConsolidationProposal,
} from '@/lib/tauri-bridge'

interface UseSkillConsolidationArgs {
  open: boolean
  proposal: SkillConsolidationProposal | null
  onOpenChange: (open: boolean) => void
  onApplied: () => void
}

export function useSkillConsolidation({
  open,
  proposal,
  onOpenChange,
  onApplied,
}: UseSkillConsolidationArgs) {
  const [expanded, setExpanded] = React.useState<Set<string>>(new Set())
  const [applying, setApplying] = React.useState(false)

  // Reset expanded state whenever a new proposal opens
  React.useEffect(() => {
    if (open) setExpanded(new Set())
  }, [open, proposal])

  // Close ourselves if proposal becomes empty
  React.useEffect(() => {
    if (open && proposal && proposal.clusters.length === 0) {
      onOpenChange(false)
    }
  }, [open, proposal, onOpenChange])

  const onToggle = (id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const onConfirm = async () => {
    if (!proposal) return
    setApplying(true)
    try {
      const result = await applySkillConsolidation(proposal)
      toast.success(
        `已整合 ${result.appliedClusters} 组技能`,
        { description: `更新 ${result.updatedSkills} 条 · 弃用 ${result.deprecatedSkills} 条` },
      )
      onApplied()
      onOpenChange(false)
    } catch (err) {
      console.error('[SkillConsolidationDialog] apply failed', err)
      toast.error('整合失败', { description: String(err) })
    } finally {
      setApplying(false)
    }
  }

  return { expanded, applying, onToggle, onConfirm }
}
