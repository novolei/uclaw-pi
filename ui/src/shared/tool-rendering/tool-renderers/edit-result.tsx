import * as React from 'react'
import { MultiFileDiff } from '@pierre/diffs/react'
import { usePierreTheme, detectLang } from './pierre-theme'

interface Props {
  input: Record<string, unknown>
  result: string
  isError: boolean
}

interface EditEntry {
  old_text: string
  new_text: string
  insert_line?: number
}

export function EditResultRenderer({ input, result, isError }: Props): React.ReactElement {
  const path = (input.path as string | undefined) ?? (input.file_path as string | undefined) ?? ''
  // Normalize an edit item across shapes: pi (`oldText`/`newText`), Claude
  // (`old_string`/`new_string`), and the batch form (`old_text`/`new_text`).
  const normalizeEdit = (e: unknown): EditEntry => {
    const o = (e ?? {}) as Record<string, unknown>
    return {
      old_text: (o.old_text ?? o.oldText ?? o.old_string ?? '') as string,
      new_text: (o.new_text ?? o.newText ?? o.new_string ?? '') as string,
      insert_line: o.insert_line as number | undefined,
    }
  }
  const rawEdits = input.edits
  // uClaw/hashline_edit use batch edits; defensive: also accept a single object.
  let edits: EditEntry[] = Array.isArray(rawEdits)
    ? (rawEdits as unknown[]).map(normalizeEdit)
    : rawEdits && typeof rawEdits === 'object'
      ? [normalizeEdit(rawEdits)]
      : []
  // pi's plain `edit` tool sends a single oldText/newText pair (no `edits` array).
  if (edits.length === 0) {
    const oldText = input.oldText ?? input.old_string ?? input.old_text
    const newText = input.newText ?? input.new_string ?? input.new_text
    if (typeof oldText === 'string' || typeof newText === 'string') {
      edits = [{ old_text: (oldText as string) ?? '', new_text: (newText as string) ?? '' }]
    }
  }
  const theme = usePierreTheme()
  const lang = detectLang(path) as string | undefined

  if (isError) {
    return (
      <div className="rounded-md bg-destructive/5 text-destructive text-xs px-3 py-2 whitespace-pre-wrap break-all">
        {result || '编辑失败'}
      </div>
    )
  }

  if (!path || edits.length === 0) {
    return (
      <div className="rounded-md bg-muted/30 text-muted-foreground text-xs px-3 py-2 italic">
        no edits to display
      </div>
    )
  }

  return (
    <div className="space-y-2 max-h-[500px] overflow-auto">
      {edits.map((edit, i) => (
        <MultiFileDiff
          key={i}
          oldFile={{ name: path, contents: edit.old_text ?? '', lang }}
          newFile={{ name: path, contents: edit.new_text ?? '', lang }}
          options={{ theme }}
        />
      ))}
    </div>
  )
}
