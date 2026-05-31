// Owns the SkillEvolutionTab version-history load + the selected-version state.
// Extracted out of the component during the features/settings migration
// (code-organization ADR 2026-05-31). IPC stays in the typed `@/lib/tauri-bridge`
// getSkillVersions helper (precedent: useChannelSettings keeps provider IPC there
// too). The cancelled-flag guard + default-select-first behavior are preserved
// verbatim from the pre-migration component.
import * as React from 'react'
import { getSkillVersions, type SkillVersionInfo } from '@/lib/tauri-bridge'

export function useSkillVersions(skillId: string) {
  const [versions, setVersions] = React.useState<SkillVersionInfo[]>([])
  const [loading, setLoading] = React.useState(true)
  const [selectedId, setSelectedId] = React.useState<string | null>(null)

  React.useEffect(() => {
    let cancelled = false
    setLoading(true)
    getSkillVersions(skillId).then((v) => {
      if (cancelled) return
      setVersions(v)
      // Default: show active vs previous
      const first = v[0]
      if (first) setSelectedId(first.id)
      setLoading(false)
    })
    return () => {
      cancelled = true
    }
  }, [skillId])

  return { versions, loading, selectedId, setSelectedId }
}
