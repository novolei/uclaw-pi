// PersonaBondTimeline: 纪念物 list. Extracted verbatim out of the 560-line
// legacy settings/PersonaBondTimeline during the features/settings split
// (code-organization ADR 2026-05-31).
import { Check, EyeOff, Loader2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import type { PersonaKeepsake, PersonaKeepsakeStatus } from '@/lib/persona-types'

export function KeepsakeList({
  keepsakes,
  busyId,
  onUpdate,
}: {
  keepsakes: PersonaKeepsake[]
  busyId: string | null
  onUpdate: (id: string, status: PersonaKeepsakeStatus) => void
}) {
  if (keepsakes.length === 0) {
    return (
      <div className="mt-2 text-xs text-muted-foreground">
        成功合作后，UClaw 可以提议一张经历卡，由你确认后保存。
      </div>
    )
  }

  return (
    <div className="mt-3 space-y-2">
      {keepsakes.map((keepsake) => (
        <div key={keepsake.id} className="rounded-md border border-border/40 bg-muted/20 p-2.5">
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <div className="truncate text-xs font-medium text-foreground">{keepsake.title}</div>
              <div className="mt-1 text-xs leading-5 text-muted-foreground">
                {keepsake.narrative}
              </div>
            </div>
            <span className="shrink-0 rounded border border-border/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {statusLabel(keepsake.status)}
            </span>
          </div>
          {keepsake.learnedText && (
            <div className="mt-2 rounded bg-background/50 px-2 py-1.5 text-[11px] text-muted-foreground">
              {keepsake.learnedText}
            </div>
          )}
          {keepsake.status === 'proposed' && (
            <div className="mt-2 flex justify-end gap-1.5">
              <Button
                size="sm"
                variant="ghost"
                className="h-7 px-2 text-xs"
                disabled={busyId === keepsake.id}
                onClick={() => onUpdate(keepsake.id, 'hidden')}
              >
                {busyId === keepsake.id ? (
                  <Loader2 className="mr-1 size-3 animate-spin" />
                ) : (
                  <EyeOff className="mr-1 size-3" />
                )}
                隐藏
              </Button>
              <Button
                size="sm"
                className="h-7 px-2 text-xs"
                disabled={busyId === keepsake.id}
                onClick={() => onUpdate(keepsake.id, 'accepted')}
              >
                {busyId === keepsake.id ? (
                  <Loader2 className="mr-1 size-3 animate-spin" />
                ) : (
                  <Check className="mr-1 size-3" />
                )}
                接受
              </Button>
            </div>
          )}
        </div>
      ))}
    </div>
  )
}

function statusLabel(status: PersonaKeepsakeStatus): string {
  switch (status) {
    case 'accepted':
      return '已确认'
    case 'hidden':
      return '已隐藏'
    case 'discarded':
      return '已丢弃'
    case 'proposed':
    default:
      return '待确认'
  }
}
