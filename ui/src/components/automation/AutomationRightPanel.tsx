import { useState } from 'react'
import { WorkspaceFilesView } from '@/components/agent/SidePanel'
import { TrajectoryReel } from '@/features/agent'

type Tab = 'files' | 'trajectory'

interface Props {
  sessionId: string
  sessionPath: string | null
}

export function AutomationRightPanel({ sessionId, sessionPath }: Props) {
  const [tab, setTab] = useState<Tab>('files')

  return (
    <div className="w-[380px] shrink-0 flex flex-col h-full border-l border-border/50 bg-background">
      {/* tab bar */}
      <div className="flex gap-0 border-b border-border/50 px-2 pt-2 shrink-0">
        {(['files', 'trajectory'] as Tab[]).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={[
              'titlebar-no-drag px-3 py-1.5 text-xs rounded-t border-b-2 transition-colors',
              tab === t
                ? 'border-primary text-primary'
                : 'border-transparent text-muted-foreground hover:text-foreground',
            ].join(' ')}
          >
            {t === 'files' ? '文件' : '轨迹'}
          </button>
        ))}
      </div>

      {/* content */}
      <div className="flex-1 overflow-hidden">
        {tab === 'files' && (
          <WorkspaceFilesView sessionId={sessionId} sessionPath={sessionPath} />
        )}
        {tab === 'trajectory' && (
          <TrajectoryReel sessionId={sessionId} />
        )}
      </div>
    </div>
  )
}
