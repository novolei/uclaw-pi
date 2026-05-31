/**
 * useRecordSkillCitations — fires the best-effort `record_skill_cited`
 * observability bump exactly once per (messageKey, citation title) pair.
 * Extracted from SkillCitationChips during the features/agent migration so the
 * chips stay presentational.
 *
 * The dedup key set is module-level (not per-component state) on purpose:
 * streaming + finalized renders of the same message must not double-count, and
 * a re-render must not re-fire. The bump routes through the agent bridge — no
 * `@tauri-apps/api` here — and is fire-and-forget (failures are logged in the
 * bridge layer; the UI never blocks on it).
 */

import * as React from 'react'
import { recordSkillCited } from '@/lib/bridge/agent'
import type { SkillCitation } from '@/lib/skill-citation'

// Module-level dedup. Streaming + finalized message can both render the same
// citation; we only want one `recordSkillCited` per logical citation per page
// lifetime.
const recordedKeys = new Set<string>()

export function useRecordSkillCitations(
  citations: SkillCitation[],
  messageKey: string,
): void {
  React.useEffect(() => {
    if (citations.length === 0) return
    for (const c of citations) {
      const key = `${messageKey}::${c.title}`
      if (recordedKeys.has(key)) continue
      recordedKeys.add(key)
      // Fire-and-forget: bumping cited_count is best-effort observability,
      // never block UI on it. Failures get logged in the bridge layer.
      recordSkillCited(c.title).catch(() => {
        // Swallow — backend logs the actual error. UI shouldn't surface
        // a transient bump failure.
      })
    }
  }, [citations, messageKey])
}
