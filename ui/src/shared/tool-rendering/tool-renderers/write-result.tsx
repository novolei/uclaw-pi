import * as React from 'react'
import { MultiFileDiff } from '@pierre/diffs/react'
import { usePierreTheme, detectLang } from './pierre-theme'

interface Props {
  input: Record<string, unknown>
  result: string
  isError: boolean
}

export function WriteResultRenderer({ input, result, isError }: Props): React.ReactElement {
  const path = (input.path as string | undefined) ?? ''
  const content = (input.content as string | undefined) ?? ''
  const theme = usePierreTheme()

  if (isError) {
    return (
      <div className="rounded-md bg-destructive/5 text-destructive text-xs px-3 py-2 whitespace-pre-wrap break-all">
        {result || '写入失败'}
      </div>
    )
  }

  if (!path) {
    return (
      <div className="rounded-md bg-muted/30 text-muted-foreground text-xs px-3 py-2 italic">
        missing path
      </div>
    )
  }

  const lang = detectLang(path) as string | undefined

  return (
    <div className="rounded-md border border-border bg-content-area overflow-auto max-h-[400px]">
      <MultiFileDiff
        oldFile={{ name: path, contents: '', lang }}
        newFile={{ name: path, contents: content, lang }}
        options={{ theme }}
      />
    </div>
  )
}
