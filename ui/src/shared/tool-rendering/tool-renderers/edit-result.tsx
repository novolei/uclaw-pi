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
  const path = (input.path as string | undefined) ?? ''
  const rawEdits = input.edits
  // uClaw's edit tool uses batch edits; defensive: also accept single edit object
  const edits: EditEntry[] = Array.isArray(rawEdits)
    ? (rawEdits as EditEntry[])
    : rawEdits && typeof rawEdits === 'object'
      ? [rawEdits as EditEntry]
      : []
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
