// Owns the PersonaBondTimeline state: the timeline, the per-action busy id, the
// journal composer draft, the load effect, and the six mutating actions
// (keepsake / journal create-promote-delete / gamification / badge). Extracted
// out of the 560-line component during the features/settings split
// (code-organization ADR 2026-05-31). IPC stays in the typed `@/lib/persona`
// domain helpers (precedent: useChannelSettings). The busy-id keying, optimistic
// re-set from each call's return, toast copy, and cancelled-flag load guard are
// preserved verbatim.
import * as React from 'react'
import { toast } from 'sonner'
import {
  createPersonaJournalEntry,
  deletePersonaJournalEntry,
  getPersonaRelationshipTimeline,
  promotePersonaJournalEntry,
  updatePersonaBadgeVisibility,
  updatePersonaKeepsakeStatus,
  updatePersonaRelationshipSettings,
} from '@/lib/persona'
import type {
  PersonaBondField,
  PersonaKeepsakeStatus,
  PersonaRelationshipTimeline,
} from '@/lib/persona-types'

export function usePersonaBondTimeline() {
  const [timeline, setTimeline] = React.useState<PersonaRelationshipTimeline | null>(null)
  const [busyId, setBusyId] = React.useState<string | null>(null)
  const [journalObservation, setJournalObservation] = React.useState('')
  const [journalInterpretation, setJournalInterpretation] = React.useState('')

  React.useEffect(() => {
    let cancelled = false
    getPersonaRelationshipTimeline()
      .then((next) => {
        if (!cancelled) setTimeline(next)
      })
      .catch((error) => {
        console.error('[PersonaBondTimeline] load failed', error)
        toast.error('加载关系时间线失败')
      })
    return () => {
      cancelled = true
    }
  }, [])

  const updateKeepsake = async (id: string, status: PersonaKeepsakeStatus) => {
    setBusyId(id)
    try {
      const next = await updatePersonaKeepsakeStatus({ id, status })
      setTimeline(next)
    } catch (error) {
      console.error('[PersonaBondTimeline] update keepsake failed', error)
      toast.error('更新纪念物失败')
    } finally {
      setBusyId(null)
    }
  }

  const createJournal = async () => {
    const observation = journalObservation.trim()
    if (!observation) return
    setBusyId('journal:create')
    try {
      const next = await createPersonaJournalEntry({
        sessionId: null,
        taskId: null,
        observation,
        interpretation: journalInterpretation.trim() || null,
        confidence: 'medium',
      })
      setJournalObservation('')
      setJournalInterpretation('')
      setTimeline(next)
    } catch (error) {
      console.error('[PersonaBondTimeline] create journal failed', error)
      toast.error('记录内心层失败')
    } finally {
      setBusyId(null)
    }
  }

  const promoteJournal = async (id: string, field: PersonaBondField) => {
    setBusyId(`${id}:${field}`)
    try {
      const next = await promotePersonaJournalEntry({ id, field })
      setTimeline(next)
    } catch (error) {
      console.error('[PersonaBondTimeline] promote journal failed', error)
      toast.error('沉淀关系档案失败')
    } finally {
      setBusyId(null)
    }
  }

  const deleteJournal = async (id: string) => {
    setBusyId(`${id}:delete`)
    try {
      const next = await deletePersonaJournalEntry(id)
      setTimeline(next)
    } catch (error) {
      console.error('[PersonaBondTimeline] delete journal failed', error)
      toast.error('删除内心层失败')
    } finally {
      setBusyId(null)
    }
  }

  const toggleGamification = async (gamificationEnabled: boolean) => {
    setBusyId('settings:gamification')
    try {
      const next = await updatePersonaRelationshipSettings({ gamificationEnabled })
      setTimeline(next)
    } catch (error) {
      console.error('[PersonaBondTimeline] update settings failed', error)
      toast.error('更新关系奖励失败')
    } finally {
      setBusyId(null)
    }
  }

  const hideBadge = async (badgeKey: string) => {
    setBusyId(`badge:${badgeKey}`)
    try {
      const next = await updatePersonaBadgeVisibility({ badgeKey, hidden: true })
      setTimeline(next)
    } catch (error) {
      console.error('[PersonaBondTimeline] update badge failed', error)
      toast.error('隐藏勋章失败')
    } finally {
      setBusyId(null)
    }
  }

  return {
    timeline,
    busyId,
    journalObservation,
    journalInterpretation,
    setJournalObservation,
    setJournalInterpretation,
    updateKeepsake,
    createJournal,
    promoteJournal,
    deleteJournal,
    toggleGamification,
    hideBadge,
  }
}
