/**
 * PersonaBondTimeline — 关系时间线 (Settings → Agent 人格).
 *
 * Thin shell: all state + persona IPC live in usePersonaBondTimeline; the section
 * lists are split into persona-bond/ presentation components. Split out of the
 * 560-line legacy settings/PersonaBondTimeline during the features/settings
 * migration (code-organization ADR 2026-05-31). Behavior preserved verbatim.
 */
import * as React from 'react'
import { Award, BookOpen, Loader2, ScrollText } from 'lucide-react'
import { SettingsSection } from './primitives/SettingsSection'
import { SettingsCard } from './primitives/SettingsCard'
import { SettingsToggle } from './primitives/SettingsToggle'
import { usePersonaBondTimeline } from '../hooks/usePersonaBondTimeline'
import { Panel } from './persona-bond/Panel'
import { BondProfileList } from './persona-bond/BondProfileList'
import { JournalComposer, JournalList } from './persona-bond/JournalSection'
import { KeepsakeList } from './persona-bond/KeepsakeList'
import { BadgeList } from './persona-bond/BadgeList'

export function PersonaBondTimeline(): React.ReactElement {
  const {
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
  } = usePersonaBondTimeline()

  const gamificationEnabled = timeline?.settings.gamificationEnabled ?? true
  const score = timeline?.affinity.score ?? 0
  const scoreWidth = `${Math.max(0, Math.min(100, score))}%`

  return (
    <SettingsSection
      title="关系时间线"
      description="纪念物、亲密度和勋章只记录共同工作的经历，不改变 Agent 能力。"
    >
      <SettingsCard>
        <div className="space-y-4 p-3 text-sm">
          <SettingsToggle
            label="关系奖励"
            description="开启后显示亲密度和勋章，关闭后仍保留经历与内心层。"
            checked={gamificationEnabled}
            disabled={!timeline || busyId === 'settings:gamification'}
            onCheckedChange={(checked) => void toggleGamification(checked)}
          />

          {gamificationEnabled ? (
            <div>
              <div className="text-xs text-muted-foreground">亲密度</div>
              <div className="mt-1 flex items-end gap-2">
                <div className="text-2xl font-semibold leading-none text-foreground">
                  {timeline ? score : '加载中'}
                </div>
                <div className="text-xs text-muted-foreground">共同经历分</div>
              </div>
              <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-muted">
                <div
                  className="h-full bg-primary transition-[width]"
                  style={{ width: scoreWidth }}
                />
              </div>
              {timeline ? (
                <div className="mt-2 space-y-1 text-xs text-muted-foreground">
                  {timeline.affinity.explanation.length > 0 ? (
                    timeline.affinity.explanation.map((line) => <div key={line}>{line}</div>)
                  ) : (
                    <div>还没有足够的共同经历沉淀。</div>
                  )}
                </div>
              ) : (
                <div className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
                  <Loader2 className="size-3 animate-spin" />
                  读取中…
                </div>
              )}
            </div>
          ) : (
            <div className="rounded-md border border-border/50 bg-muted/20 p-3 text-xs text-muted-foreground">
              关系奖励已关闭。经历卡、内心层和关系档案仍会保留，界面不显示分数和勋章。
            </div>
          )}

          <div className="grid gap-3 lg:grid-cols-2">
            <Panel title="关系档案" icon={<BookOpen size={14} className="text-muted-foreground" />}>
              <BondProfileList bond={timeline?.bond} />
            </Panel>

            <Panel title="纪念物" icon={<ScrollText size={14} className="text-muted-foreground" />}>
              <KeepsakeList
                keepsakes={timeline?.keepsakes ?? []}
                busyId={busyId}
                onUpdate={(id, status) => void updateKeepsake(id, status)}
              />
            </Panel>
          </div>

          <Panel title="内心层日志" icon={<BookOpen size={14} className="text-muted-foreground" />}>
            <JournalComposer
              observation={journalObservation}
              interpretation={journalInterpretation}
              busy={busyId === 'journal:create'}
              onObservationChange={setJournalObservation}
              onInterpretationChange={setJournalInterpretation}
              onCreate={() => void createJournal()}
            />
            <JournalList
              entries={timeline?.journalEntries ?? []}
              busyId={busyId}
              onPromote={(id, field) => void promoteJournal(id, field)}
              onDelete={(id) => void deleteJournal(id)}
            />
          </Panel>

          {gamificationEnabled && (
            <Panel title="勋章" icon={<Award size={14} className="text-muted-foreground" />}>
              <BadgeList
                badges={timeline?.badges ?? []}
                busyId={busyId}
                onHide={(badgeKey) => void hideBadge(badgeKey)}
              />
            </Panel>
          )}
        </div>
      </SettingsCard>
    </SettingsSection>
  )
}
