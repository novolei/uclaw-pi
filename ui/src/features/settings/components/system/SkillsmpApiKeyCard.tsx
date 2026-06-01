// skillsmp.com API key — OPTIONAL. The marketplace client can search skillsmp.com
// keyless (anonymous rate limit); a stored key only raises that limit. Write-only
// from the UI (never read back); the card shows only whether one is stored.
// Mirrors `SkillsApiKeyCard`; side effects live in `useSkillsmpApiKey`.
// Never calls IPC directly.
import * as React from 'react'
import { cn } from '@/lib/utils'
import { useSkillsmpApiKey } from '../../hooks/useSkillsmpApiKey'

export function SkillsmpApiKeyCard({ onError }: { onError?: (m: string) => void }) {
  const { isSet, saving, save } = useSkillsmpApiKey(onError)
  const [draft, setDraft] = React.useState('')
  const onSave = async () => {
    if (draft.trim().length === 0) return
    const ok = await save(draft)
    if (ok) setDraft('')
  }
  return (
    <div className="rounded-xl border border-border px-4 py-3">
      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-3">
          <div className="text-sm font-medium text-foreground">skillsmp.com API key</div>
          <span
            className={cn(
              'shrink-0 text-xs px-2 py-0.5 rounded-full',
              isSet
                ? 'bg-emerald-500/15 text-emerald-400'
                : 'bg-muted text-muted-foreground',
            )}
          >
            {isSet === null ? '…' : isSet ? '已设置' : '未设置'}
          </span>
        </div>
        <p className="text-xs text-muted-foreground">
          可选 —— 免 key 也能搜 <span className="text-foreground/70">skillsmp.com</span>，填入仅提高限流（匿名{' '}
          <span className="font-mono">50/天</span> →{' '}
          <span className="font-mono">500/天</span>）。仅写入本地，不回显。
        </p>
        <div className="flex items-center gap-2">
          <input
            type="password"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={isSet ? '已存储（输入以覆盖）' : 'sk_live_…'}
            autoComplete="off"
            spellCheck={false}
            className="min-w-0 flex-1 text-xs px-2 py-1.5 rounded-lg bg-muted/50 border border-border text-foreground placeholder:text-muted-foreground/60 focus:outline-none focus:ring-1 focus:ring-ring"
          />
          <button
            onClick={onSave}
            disabled={saving || isSet === null || draft.trim().length === 0}
            className="shrink-0 text-xs px-3 py-1.5 rounded-lg bg-muted text-muted-foreground hover:bg-muted/70 transition-colors disabled:opacity-50"
          >
            {saving ? '…' : '保存'}
          </button>
          {isSet && (
            <button
              onClick={() => save('')}
              disabled={saving}
              className="shrink-0 text-xs px-3 py-1.5 rounded-lg text-red-400/80 hover:bg-red-400/10 transition-colors disabled:opacity-50"
            >
              清除
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
