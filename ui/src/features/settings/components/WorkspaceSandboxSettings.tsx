import * as React from 'react'
import { Plus, Trash2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import { useWorkspaceSandbox } from '../hooks/useWorkspaceSandbox'

export function WorkspaceSandboxSettings(): React.ReactElement {
  const { sessionId, global, session, handleAdd, handleRemove, handlePromote } =
    useWorkspaceSandbox()

  return (
    <div className="flex flex-col gap-6">
      <section>
        <h3 className="text-sm font-semibold text-foreground mb-2">始终允许的外部路径</h3>
        <p className="text-xs text-muted-foreground mb-3">Agent 在任何工作区都可以访问这些路径,无需提示。</p>
        <div className="rounded-md border bg-muted/30">
          {global.length === 0 && (
            <div className="px-3 py-2 text-xs italic text-muted-foreground">尚未添加任何路径。</div>
          )}
          {global.map((p) => (
            <div key={p} className="flex items-center gap-2 px-3 py-1.5 border-b last:border-b-0">
              <span className="flex-1 truncate font-mono text-xs" title={p}>{p}</span>
              <button
                type="button"
                onClick={() => handleRemove(p)}
                className={cn('shrink-0 p-1 rounded text-muted-foreground hover:text-destructive hover:bg-destructive/10')}
                title="删除"
              >
                <Trash2 className="size-3.5" />
              </button>
            </div>
          ))}
        </div>
        <button
          type="button"
          onClick={handleAdd}
          className="mt-2 inline-flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-md bg-primary/10 text-primary hover:bg-primary/20"
        >
          <Plus className="size-3.5" />
          添加路径
        </button>
      </section>

      <section>
        <h3 className="text-sm font-semibold text-foreground mb-2">本会话已临时授权的外部路径</h3>
        <p className="text-xs text-muted-foreground mb-3">
          仅本会话有效,重启应用后清除。点"升级为永久"加入上面的列表。
        </p>
        <div className="rounded-md border bg-muted/30">
          {!sessionId && (
            <div className="px-3 py-2 text-xs italic text-muted-foreground">没有活动会话。</div>
          )}
          {sessionId && session.length === 0 && (
            <div className="px-3 py-2 text-xs italic text-muted-foreground">本会话没有触发过外部路径授权。</div>
          )}
          {sessionId && session.map((p) => (
            <div key={p} className="flex items-center gap-2 px-3 py-1.5 border-b last:border-b-0">
              <span className="flex-1 truncate font-mono text-xs" title={p}>{p}</span>
              <button
                type="button"
                onClick={() => handlePromote(p)}
                className="shrink-0 px-2 py-0.5 text-[11px] rounded text-primary hover:bg-primary/10"
              >
                升级为永久
              </button>
            </div>
          ))}
        </div>
      </section>
    </div>
  )
}
