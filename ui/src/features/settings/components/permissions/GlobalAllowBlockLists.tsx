// Global tier — the legacy whole-tool whitelist + blocklist from
// `safety_policy.json`. Presentational: receives the lists + the remove/unblock
// + refresh callbacks from the PermissionsSettings shell's hook. Split out of the
// 328-line components/settings/PermissionsSettings.tsx during P3a; markup is
// byte-identical so behavior is preserved exactly.
import * as React from 'react'
import { Trash2, RefreshCw, ShieldCheck, ShieldOff } from 'lucide-react'
import { Button } from '@/components/ui/button'

interface GlobalAllowBlockListsProps {
  allowList: string[]
  blockList: string[]
  loading: boolean
  onRefresh: () => void
  onRemoveAllow: (toolName: string) => void
  onUnblock: (toolName: string) => void
}

export function GlobalAllowBlockLists({
  allowList,
  blockList,
  loading,
  onRefresh,
  onRemoveAllow,
  onUnblock,
}: GlobalAllowBlockListsProps): React.ReactElement {
  return (
    <section>
      <div className="mb-2.5 flex items-center justify-between">
        <h3 className="text-[12px] font-semibold uppercase tracking-widest text-muted-foreground/70">
          全局放行 / 阻止
        </h3>
        <Button size="sm" variant="ghost" onClick={() => void onRefresh()} disabled={loading} title="刷新">
          <RefreshCw className="size-3.5" />
        </Button>
      </div>
      <p className="mb-2 text-[11.5px] text-muted-foreground/70 leading-relaxed">
        全局放行 = 该工具的<b>所有</b>调用自动通过（包括 <code className="px-1 rounded bg-muted/60">bash rm -rf</code> 这种）。粒度过粗，建议改用下方"权限规则"针对命令前缀放行。
      </p>
      <div className="grid grid-cols-2 gap-3">
        {/* Allow list */}
        <div className="rounded-lg border border-green-500/30 bg-green-500/5 p-2">
          <div className="mb-1.5 flex items-center gap-1.5 text-[11px] font-medium text-green-700 dark:text-green-400">
            <ShieldCheck className="size-3.5" />
            全局放行（auto-approve）
          </div>
          {allowList.length === 0 ? (
            <div className="text-[11.5px] text-muted-foreground/60 px-1 py-2">空</div>
          ) : (
            <ul className="space-y-1">
              {allowList.map((tool) => (
                <li key={tool} className="flex items-center justify-between gap-2 px-1.5 py-1 rounded hover:bg-green-500/10 group">
                  <code className="font-mono text-[12px]">{tool}</code>
                  <Button
                    size="sm" variant="ghost"
                    onClick={() => void onRemoveAllow(tool)}
                    className="h-6 w-6 p-0 opacity-0 group-hover:opacity-100"
                    title="移除"
                  >
                    <Trash2 className="size-3 text-muted-foreground/70" />
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </div>
        {/* Block list */}
        <div className="rounded-lg border border-red-500/30 bg-red-500/5 p-2">
          <div className="mb-1.5 flex items-center gap-1.5 text-[11px] font-medium text-red-700 dark:text-red-400">
            <ShieldOff className="size-3.5" />
            全局阻止（block）
          </div>
          {blockList.length === 0 ? (
            <div className="text-[11.5px] text-muted-foreground/60 px-1 py-2">空</div>
          ) : (
            <ul className="space-y-1">
              {blockList.map((tool) => (
                <li key={tool} className="flex items-center justify-between gap-2 px-1.5 py-1 rounded hover:bg-red-500/10 group">
                  <code className="font-mono text-[12px]">{tool}</code>
                  <Button
                    size="sm" variant="ghost"
                    onClick={() => void onUnblock(tool)}
                    className="h-6 w-6 p-0 opacity-0 group-hover:opacity-100"
                    title="解除"
                  >
                    <Trash2 className="size-3 text-muted-foreground/70" />
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </section>
  )
}
