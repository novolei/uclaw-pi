import * as React from 'react'
import { useLivePlanContent } from '../hooks/useLivePlanContent'

interface PlanStep {
  text: string
  done: boolean
}

interface ParsedPlan {
  title: string
  goal: string
  steps: PlanStep[]
  notes: string
}

export function parsePlanMarkdown(content: string): ParsedPlan {
  let title = 'Unnamed Plan'
  let goal = ''
  let steps: PlanStep[] = []
  let notes = ''

  // Parse YAML frontmatter. Accept both `title:` (canonical) and `task:`
  // (what plan_write actually emits — see src-tauri/src/agent/tools/builtin/plan.rs).
  // Also strip surrounding double-quotes because plan_write always quotes the value.
  const fmMatch = content.match(/^---\n([\s\S]+?)\n---\n?/m)
  if (fmMatch) {
    const fm = fmMatch[1]
    const titleMatch = fm.match(/^(?:title|task):\s*(.+)$/m)
    if (titleMatch) {
      title = titleMatch[1].trim().replace(/^"(.*)"$/, '$1').replace(/^'(.*)'$/, '$1')
    }
  }

  // Strip frontmatter, parse body sections
  const bodyStart = fmMatch ? content.indexOf(fmMatch[0]) + fmMatch[0].length : 0
  const body = content.slice(bodyStart)

  // Collect each ## heading: store both headingStart (index of ##) and contentStart (after heading line)
  const sectionRegex = /^##\s+(.+)$/gm
  const sections: Array<{ name: string; headingStart: number; contentStart: number }> = []
  let match: RegExpExecArray | null

  while ((match = sectionRegex.exec(body)) !== null) {
    sections.push({
      name: match[1].trim(),
      headingStart: match.index,
      contentStart: match.index + match[0].length,
    })
  }

  function getSectionText(name: string): string {
    const idx = sections.findIndex((s) => s.name.toLowerCase() === name.toLowerCase())
    if (idx === -1) return ''
    const start = sections[idx].contentStart
    // End is the start of the next heading's `##`, not its content
    const end = idx + 1 < sections.length ? sections[idx + 1].headingStart : body.length
    return body.slice(start, end).trim()
  }

  const goalText = getSectionText('Goal')
  if (goalText) goal = goalText

  const stepsText = getSectionText('Steps')
  if (stepsText) {
    steps = stepsText
      .split('\n')
      .filter((line) => /^- \[[ x]\]/i.test(line))
      .map((line) => {
        const done = /^- \[x\]/i.test(line)
        const text = line.replace(/^- \[[ x]\]\s*/i, '').trim()
        return { text, done }
      })
  }

  const notesText = getSectionText('Notes')
  if (notesText) notes = notesText

  return { title, goal, steps, notes }
}

interface PlanViewerProps {
  planContent: string
  planFilename: string
}

export function PlanViewer({ planContent, planFilename }: PlanViewerProps): React.ReactElement {
  // Live plan content — seeds from props, resets on file switch, and follows
  // `plan:updated` events (subscription + reset logic in the hook).
  const liveContent = useLivePlanContent(planContent, planFilename)

  const plan = parsePlanMarkdown(liveContent)
  const total = plan.steps.length
  const done = plan.steps.filter((s) => s.done).length
  const progressPct = total > 0 ? (done / total) * 100 : 0

  return (
    <div className="flex flex-col gap-3 p-3">
      {/* Header */}
      <div>
        <p className="text-[12px] font-semibold text-foreground truncate">{plan.title}</p>
        <p className="text-[10px] text-muted-foreground mt-0.5 truncate">{planFilename}</p>
      </div>

      {/* Progress bar */}
      {total > 0 && (
        <div>
          <div className="flex items-center justify-between mb-1">
            <span className="text-[10px] text-muted-foreground">Progress</span>
            <span className="text-[10px] text-muted-foreground">
              {done}/{total} steps done
            </span>
          </div>
          <div
            role="progressbar"
            aria-valuenow={done}
            aria-valuemin={0}
            aria-valuemax={total}
            className="h-1.5 rounded-full bg-muted/30 overflow-hidden"
          >
            <div
              className="h-full rounded-full bg-blue-500 transition-all duration-300"
              style={{ width: `${progressPct}%` }}
            />
          </div>
        </div>
      )}

      {/* Goal */}
      {plan.goal && (
        <div>
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground mb-1">Goal</p>
          <p className="text-[11px] text-foreground whitespace-pre-wrap">{plan.goal}</p>
        </div>
      )}

      {/* Steps */}
      {plan.steps.length > 0 && (
        <div>
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground mb-1.5">Steps</p>
          <div className="flex flex-col gap-1">
            {plan.steps.map((step, i) => (
              <div key={`${i}-${step.text}`} className="flex items-start gap-1.5">
                <span className="shrink-0 text-[13px] leading-tight mt-px">
                  {step.done ? '✅' : '◻️'}
                </span>
                <span
                  className={`text-[11px] leading-snug ${
                    step.done ? 'text-muted-foreground line-through' : 'text-foreground'
                  }`}
                >
                  {step.text}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Notes */}
      {plan.notes && (
        <div>
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground mb-1">Notes</p>
          <p className="text-[11px] text-muted-foreground whitespace-pre-wrap">{plan.notes}</p>
        </div>
      )}
    </div>
  )
}
