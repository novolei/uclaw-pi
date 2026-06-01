// skills.sh API key — used by the marketplace client to search / install skills
// from skills.sh (in-chat `skill_marketplace_search` + the 技能市场 page). The key
// is write-only from the UI (never read back); the card shows only whether one
// is stored. Mirrors `PiEngineToggleCard`; side effects live in `useSkillsApiKey`.
// Never calls IPC directly.
import * as React from 'react'
import { cn } from '@/lib/utils'
import { useSkillsApiKey } from '../../hooks/useSkillsApiKey'

export function SkillsApiKeyCard({ onError }: { onError?: (m: string) => void }) {
  const { isSet, saving, save } = useSkillsApiKey(onError)
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
          <div className="text-sm font-medium text-foreground">skills.sh API key</div>
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
          用于从 <span className="text-foreground/70">skills.sh</span> 搜索 / 安装技能（聊天内{' '}
          <span className="font-mono">skill_marketplace_search</span> 与技能市场）。向{' '}
          <span className="text-foreground/70">skills-api@vercel.com</span> 申请{' '}
          <span className="font-mono">sk_live_</span> key。仅写入本地，不回显。
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
