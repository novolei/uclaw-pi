import { useEffect, useState, useMemo } from 'react'
import { getAgentSessionMessages } from '@/lib/tauri-bridge'
import type { AutomationActivity } from '@/lib/tauri-bridge'
import { AgentMessages } from '@/features/agent'
import type { AgentMessage } from '@/lib/agent-types'
import { ActivityMarkdown } from './ActivityMarkdown'
import { ArtifactChip, OUTCOME_CONFIG } from './ActivityListItem'
import type { ReportArtifact } from './ActivityListItem'

interface Props {
  sessionId: string
  isRunning?: boolean
  activity?: AutomationActivity | null
  onBack: () => void
}

function ReportCard({ activity }: { activity: AutomationActivity }) {
  const [collapsed, setCollapsed] = useState(false)
  const isActive = activity.status === 'running' || activity.status === 'queued'

  const artifacts = useMemo<ReportArtifact[]>(() => {
    try { return JSON.parse(activity.reportArtifactsJson) as ReportArtifact[] }
    catch { return [] }
  }, [activity.reportArtifactsJson])

  if (!isActive && !activity.reportText) return null

  const outcomeCfg = activity.reportOutcome
    ? (OUTCOME_CONFIG[activity.reportOutcome] ?? null)
    : null

  return (
    <div className="mx-3 mt-3 mb-1 border border-border/50 rounded-lg overflow-hidden shrink-0">
      <button
        onClick={() => setCollapsed((v) => !v)}
        className="titlebar-no-drag w-full flex items-center gap-2 px-3 py-2 text-xs border-b border-border/40 bg-muted/30 hover:bg-muted/50 transition-colors"
      >
        <span className="font-medium text-foreground/80">运行报告</span>
        {outcomeCfg && (
          <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium ${outcomeCfg.className}`}>
            {outcomeCfg.label}
          </span>
        )}
        <span className="ml-auto text-muted-foreground">{collapsed ? '▸' : '▾'}</span>
      </button>
      {!collapsed && (
        <div className="px-3 py-2">
          {isActive && !activity.reportText ? (
            <p className="text-xs text-muted-foreground italic">运行中，暂无报告…</p>
          ) : (
            <>
              {activity.reportText && (
                <ActivityMarkdown content={activity.reportText} />
              )}
              {artifacts.length > 0 && (
                <div className="flex flex-wrap gap-1.5 mt-2">
                  {artifacts.map((a, i) => (
                    <ArtifactChip key={i} artifact={a} workingDir={activity.workingDir} />
                  ))}
                </div>
              )}
            </>
          )}
        </div>
      )}
    </div>
  )
}

export function RunSessionSubView({ sessionId, isRunning, activity, onBack }: Props) {
  const [messages, setMessages] = useState<AgentMessage[]>([])
  const [loaded, setLoaded] = useState(false)

  // Initial load
  useEffect(() => {
    setLoaded(false)
    getAgentSessionMessages(sessionId).then((msgs) => {
      setMessages(msgs as AgentMessage[])
      setLoaded(true)
    })
  }, [sessionId])

  // Poll while run is active so the transcript stays live.
  useEffect(() => {
    if (!isRunning) return
    const id = setInterval(() => {
      getAgentSessionMessages(sessionId).then((msgs) =>
        setMessages(msgs as AgentMessage[])
      )
    }, 2000)
    return () => clearInterval(id)
  }, [isRunning, sessionId])

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Breadcrumb */}
      <div className="flex items-center gap-1 px-3 py-2 border-b border-border/50 text-xs text-muted-foreground shrink-0">
        <button
          onClick={onBack}
          className="titlebar-no-drag text-primary hover:underline"
        >
          ← 动态
        </button>
        <span>/</span>
        <span>运行详情</span>
        {isRunning && (
          <span className="ml-auto flex items-center gap-1 text-primary">
            <span className="size-1.5 rounded-full bg-primary animate-pulse" />
            运行中
          </span>
        )}
      </div>

      {/* Report card (pinned above transcript) */}
      {activity && <ReportCard activity={activity} />}

      {/* Divider */}
      {activity && (activity.reportText || activity.status === 'running' || activity.status === 'queued') && (
        <div className="px-3 pt-2 pb-1 shrink-0">
          <p className="text-[10px] uppercase tracking-wider text-muted-foreground/50 font-semibold">
            对话过程
          </p>
        </div>
      )}

      {/* Transcript */}
      <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
        <AgentMessages
          sessionId={sessionId}
          messages={messages}
          messagesLoaded={loaded}
          streaming={false}
        />
      </div>
    </div>
  )
}
