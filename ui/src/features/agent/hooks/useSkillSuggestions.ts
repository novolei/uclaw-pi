/**
 * useSkillSuggestions — debounced local skill search for SkillSuggestionBar.
 * Extracted during the features/agent migration so the bar stays presentational.
 *
 * Behavior (unchanged from the inline version):
 *   - Lazily loads + caches the enabled builtin + learned skills (one fetch per
 *     mount lifetime, via the agent bridge — no `@tauri-apps/api` here).
 *   - Debounces the input by 500ms; only searches once the trimmed query is ≥ 5
 *     chars, otherwise clears suggestions.
 *   - Fuzzy-matches on name/description (case-insensitive substring) and returns
 *     the top 3 hits.
 */

import * as React from 'react'
import { listSkills, listLearnedSkills } from '@/lib/bridge/agent'

export interface SkillCandidate {
  name: string
  description: string
  provenance: 'learned' | 'builtin'
}

export function useSkillSuggestions(inputText: string): SkillCandidate[] {
  const [suggestions, setSuggestions] = React.useState<SkillCandidate[]>([])
  const cacheRef = React.useRef<SkillCandidate[] | null>(null)

  // Load skills once (lazy, cached).
  const loadSkills = React.useCallback(async (): Promise<SkillCandidate[]> => {
    if (cacheRef.current) return cacheRef.current

    const [builtinResult, learnedResult] = await Promise.allSettled([
      listSkills(),
      listLearnedSkills(),
    ])

    const candidates: SkillCandidate[] = []

    if (builtinResult.status === 'fulfilled') {
      for (const s of builtinResult.value) {
        if (s.enabled) {
          candidates.push({
            name: s.name,
            description: s.description || s.category || '',
            provenance: 'builtin',
          })
        }
      }
    }

    if (learnedResult.status === 'fulfilled') {
      for (const s of learnedResult.value) {
        if (s.enabled) {
          candidates.push({
            name: s.name,
            description: s.context?.split('\n')[0]?.slice(0, 80) || '',
            provenance: 'learned',
          })
        }
      }
    }

    cacheRef.current = candidates
    return candidates
  }, [])

  // Debounced search.
  React.useEffect(() => {
    const q = inputText.trim().toLowerCase()
    if (q.length < 5) {
      setSuggestions([])
      return
    }

    const timer = setTimeout(async () => {
      try {
        const all = await loadSkills()
        const matched = all
          .filter((s) => {
            return (
              s.name.toLowerCase().includes(q) ||
              s.description.toLowerCase().includes(q)
            )
          })
          .slice(0, 3)
        setSuggestions(matched)
      } catch {
        setSuggestions([])
      }
    }, 500)

    return () => clearTimeout(timer)
  }, [inputText, loadSkills])

  return suggestions
}
