// PersonaBondTimeline: 内心层 composer + list. Extracted verbatim out of the
// 560-line legacy settings/PersonaBondTimeline during the features/settings
// split (code-organization ADR 2026-05-31).
import { Loader2, Plus, Trash2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import type { PersonaBondField, PersonaJournalEntry } from '@/lib/persona-types'

export function JournalComposer({
  observation,
  interpretation,
  busy,
  onObservationChange,
  onInterpretationChange,
  onCreate,
}: {
  observation: string
  interpretation: string
  busy: boolean
  onObservationChange: (value: string) => void
  onInterpretationChange: (value: string) => void
  onCreate: () => void
}) {
  return (
    <div className="mt-3 grid gap-2 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
      <Textarea
        className="min-h-20 resize-none text-xs"
        value={observation}
        onChange={(event) => onObservationChange(event.target.value)}
        placeholder="记录一次合作中的观察"
      />
      <Textarea
        className="min-h-20 resize-none text-xs"
        value={interpretation}
        onChange={(event) => onInterpretationChange(event.target.value)}
        placeholder="可选：它可能说明的关系偏好"
      />
      <Button
        size="sm"
        className="h-9 self-start px-2 text-xs"
        disabled={busy || observation.trim().length === 0}
        onClick={onCreate}
      >
        {busy ? <Loader2 className="mr-1 size-3 animate-spin" /> : <Plus className="mr-1 size-3" />}
        记录
      </Button>
    </div>
  )
}

export function JournalList({
  entries,
  busyId,
  onPromote,
  onDelete,
}: {
  entries: PersonaJournalEntry[]
  busyId: string | null
  onPromote: (id: string, field: PersonaBondField) => void
  onDelete: (id: string) => void
}) {
  if (entries.length === 0) {
    return <div className="mt-3 text-xs text-muted-foreground">还没有内心层日志。</div>
  }

  return (
    <div className="mt-3 space-y-2">
      {entries.map((entry) => (
        <div key={entry.id} className="rounded-md border border-border/40 bg-muted/20 p-2.5">
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <div className="text-xs font-medium leading-5 text-foreground">
                {entry.observation}
              </div>
              {entry.interpretation && (
                <div className="mt-1 text-xs leading-5 text-muted-foreground">
                  {entry.interpretation}
                </div>
              )}
            </div>
            <span className="shrink-0 rounded border border-border/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {confidenceLabel(entry.confidence)}
            </span>
          </div>
          <div className="mt-2 flex flex-wrap justify-end gap-1.5">
            <Button
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-xs"
              disabled={busyId === `${entry.id}:support_style`}
              onClick={() => onPromote(entry.id, 'support_style')}
            >
              {busyId === `${entry.id}:support_style` && (
                <Loader2 className="mr-1 size-3 animate-spin" />
              )}
              提升为支持风格
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-xs"
              disabled={busyId === `${entry.id}:collaboration_rhythm`}
              onClick={() => onPromote(entry.id, 'collaboration_rhythm')}
            >
              {busyId === `${entry.id}:collaboration_rhythm` && (
                <Loader2 className="mr-1 size-3 animate-spin" />
              )}
              提升为协作节奏
            </Button>
            <Button
              aria-label="删除内心层日志"
              size="sm"
              variant="ghost"
              className="h-7 px-2 text-xs text-muted-foreground"
              disabled={busyId === `${entry.id}:delete`}
              onClick={() => onDelete(entry.id)}
            >
              {busyId === `${entry.id}:delete` ? (
                <Loader2 className="size-3 animate-spin" />
              ) : (
                <Trash2 className="size-3" />
              )}
            </Button>
          </div>
        </div>
      ))}
    </div>
  )
}

function confidenceLabel(confidence: PersonaJournalEntry['confidence']): string {
  switch (confidence) {
    case 'high':
      return '高置信'
    case 'low':
      return '低置信'
    case 'medium':
    default:
      return '中置信'
  }
}
