/**
 * WorkspaceRail — flat session list for the currently active workspace.
 *
 * Phase 4b (ARC-style switcher): replaces the previous tree-of-all-
 * workspaces render. WorkspaceRail used to map over all workspaces and
 * render a <WorkspaceGroup> per workspace; now it renders only the
 * active workspace's sessions as a flat list.
 *
 * Workspace-level affordances (rename / delete / create) moved to
 * WorkspaceHeader (top) and WorkspaceSwitcherBar (bottom).
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import {
  workspacesAtom,
  activeWorkspaceIdAtom,
  workspaceSessionsAtom,
  refreshWorkspacesAtom,
} from '@/atoms/workspace'
import { SessionItem } from './SessionItem'
import { MoveSessionDialog } from '@/features/agent'
import {
  agentSessionsAtom,
  agentSessionIndicatorMapAtom,
  togglePinAgentSessionAtom,
} from '@/atoms/agent-atoms'
import { tabsAtom } from '@/atoms/tab-atoms'
import type { AgentWorkspace, AgentSessionMeta } from '@/lib/agent-types'
import { toggleArchiveAgentSession, deleteAgentSession } from '@/lib/tauri-bridge'
import type { WorkspaceSession } from '@/atoms/workspace'
import { Archive, ArrowLeft } from 'lucide-react'
import { toast } from 'sonner'

/**
 * True when a session was produced by an automation run (origin metadata
 * starts with "automation:"). Such run-sessions are reached through the
 * AutomationHub activity list, not the workspace session rail (design §0.4).
 */
export function isAutomationSession(s: { metadataJson?: string | null }): boolean {
  if (!s.metadataJson) return false
  try {
    const meta = JSON.parse(s.metadataJson) as { origin?: string }
    return typeof meta.origin === 'string' && meta.origin.startsWith('automation:')
  } catch {
    return false
  }
}

interface WorkspaceRailProps {
  activeSessionId: string | null
  onSelectSession: (sessionId: string) => void
  onDeleteSession?: (sessionId: string) => void
}

/** Section label inside WorkspaceRail. Matches the OVERVIEW labels
 *  used in the approval modal (text-[10px], uppercase, tracking-wider). */
function SegmentHeader({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <p className="text-[10px] font-semibold uppercase tracking-wider
                  text-muted-foreground/70 mt-2 mb-1 px-2">
      {children}
    </p>
  )
}

/** Row shown in the archived view — inline restore + permanent-delete buttons. */
function ArchivedSessionRow({
  s,
  onRestore,
  onDelete,
}: {
  s: WorkspaceSession
  onRestore: () => void
  onDelete: () => void
}): React.ReactElement {
  return (
    <div className="flex items-center gap-2 rounded-md px-2 py-1.5 text-[13px] text-muted-foreground hover:bg-muted/50 transition-colors">
      <span
        className="shrink-0 text-[14px] leading-none"
        style={{ fontFamily: "'Noto Emoji', sans-serif", width: '18px' }}
      >
        {s.titleEmoji || '💬'}
      </span>
      <span className="flex-1 truncate">{s.title || 'New session'}</span>
      <div className="shrink-0 flex items-center gap-1">
        <button
          onClick={(e) => { e.stopPropagation(); onRestore() }}
          className="titlebar-no-drag text-[11px] px-2 py-0.5 rounded border border-border/60 text-foreground/60 hover:text-foreground hover:bg-muted transition-colors"
        >
          恢复
        </button>
        <button
          onClick={(e) => { e.stopPropagation(); onDelete() }}
          className="titlebar-no-drag text-[11px] px-2 py-0.5 rounded border border-danger/30 text-danger/60 hover:text-danger hover:bg-danger/5 transition-colors"
        >
          永久删除
        </button>
      </div>
    </div>
  )
}

export function WorkspaceRail({
  activeSessionId,
  onSelectSession,
  onDeleteSession,
}: WorkspaceRailProps): React.ReactElement {
  const workspaces = useAtomValue(workspacesAtom)
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const workspaceSessions = useAtomValue(workspaceSessionsAtom)
  const indicatorMap = useAtomValue(agentSessionIndicatorMapAtom)
  const refreshWorkspaces = useSetAtom(refreshWorkspacesAtom)
  const setAgentSessions = useSetAtom(agentSessionsAtom)

  const [moveTargetSessionId, setMoveTargetSessionId] = React.useState<string | null>(null)
  const [showArchived, setShowArchived] = React.useState(false)
  const agentSessions = useAtomValue(agentSessionsAtom)
  const moveTargetSession = moveTargetSessionId
    ? agentSessions.find((s) => s.id === moveTargetSessionId)
    : null

  const agentWorkspaces: AgentWorkspace[] = React.useMemo(
    () => workspaces.map((w) => ({
      id: w.id,
      name: w.name,
      icon: w.icon,
      path: w.path,
      createdAt: Date.parse(w.createdAt) || Date.now(),
      updatedAt: Date.parse(w.updatedAt) || Date.now(),
    })),
    [workspaces],
  )

  React.useEffect(() => {
    refreshWorkspaces()
  }, [refreshWorkspaces])

  const togglePin = useSetAtom(togglePinAgentSessionAtom)

  // Open-tab indicator — derive the set of session ids that currently have
  // a tab in the global pool. Read tabsAtom (not visibleTabsAtom) so the
  // indicator works even before workspace switch settles the visible slice.
  const tabs = useAtomValue(tabsAtom)
  const openSessionIds = React.useMemo(
    () => new Set(tabs.map((t) => t.sessionId)),
    [tabs],
  )

  const allSessions = (
    activeWorkspaceId ? (workspaceSessions[activeWorkspaceId] ?? []) : []
  ).filter((s) => !isAutomationSession(s))

  const archivedCount = allSessions.filter((s) => !!s.archived).length
  const sessions = allSessions.filter((s) => showArchived ? !!s.archived : !s.archived)

  // Two-segment split: pinned (sorted by pinnedAt DESC — most recently
  // pinned at the top) and unpinned (preserves the source atom's
  // updatedAt DESC order).
  const pinned = sessions
    .filter((s) => s.pinnedAt !== null)
    .sort((a, b) => (b.pinnedAt ?? 0) - (a.pinnedAt ?? 0))
  const unpinned = sessions.filter((s) => s.pinnedAt === null)

  const handleTogglePin = async (id: string): Promise<void> => {
    try {
      await togglePin(id)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`固定失败：${msg}`)
    }
  }

  const handleToggleArchive = async (id: string): Promise<void> => {
    try {
      await toggleArchiveAgentSession(id)
      // Optimistically flip the archived flag so the rail updates immediately
      // without waiting for the next window-focus refetch.
      setAgentSessions((prev: AgentSessionMeta[]) =>
        prev.map((s) => s.id === id ? { ...s, archived: !s.archived } : s)
      )
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`归档操作失败：${msg}`)
    }
  }

  const handlePermanentDelete = async (id: string): Promise<void> => {
    try {
      await deleteAgentSession(id)
      setAgentSessions((prev: AgentSessionMeta[]) => prev.filter((s) => s.id !== id))
      onDeleteSession?.(id)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.error(`删除失败：${msg}`)
    }
  }

  return (
    <>
      {/* Workspace-switch animation is owned by LeftSidebar — it wraps
          this rail + WorkspaceHeader in a single ARC-style horizontal
          slide so they move as one piece. Keep this container plain. */}
      <div className="flex-1 overflow-y-auto px-3 pt-1 pb-1 scrollbar-none">
        {!showArchived && sessions.length === 0 && (
          <p className="text-[11px] text-muted-foreground px-2 py-3 italic">
            尚无会话。点击上方"新会话"开始。
          </p>
        )}

        {showArchived ? (
          /* Archived view — context banner + inline restore/delete rows. */
          <div className="rounded-xl border border-border/50 bg-muted/30 overflow-hidden mt-1">
            {/* Context header */}
            <div className="flex items-center gap-2 px-3 py-2 border-b border-border/40 bg-muted/40">
              <Archive size={12} className="text-muted-foreground/70 shrink-0" />
              <span className="text-[11px] font-semibold tracking-wide text-muted-foreground/70 uppercase">
                已归档
              </span>
              <span className="ml-auto text-[10px] text-muted-foreground/50">
                {sessions.length} 个会话
              </span>
            </div>
            {sessions.length === 0 ? (
              <p className="text-[11px] text-muted-foreground/60 px-3 py-3 italic">
                暂无已归档会话。
              </p>
            ) : (
              <div className="p-1">
                {sessions.map((s) => (
                  <ArchivedSessionRow
                    key={s.id}
                    s={s}
                    onRestore={() => void handleToggleArchive(s.id)}
                    onDelete={() => void handlePermanentDelete(s.id)}
                  />
                ))}
              </div>
            )}
          </div>
        ) : (
          <>
            {pinned.length > 0 && (
              <>
                <SegmentHeader>📌 固定</SegmentHeader>
                {pinned.map((s) => (
                  <SessionItem
                    key={s.id}
                    id={s.id}
                    title={s.title}
                    titleEmoji={s.titleEmoji}
                    titlePending={s.titlePending}
                    isActive={activeSessionId === s.id}
                    running={indicatorMap.get(s.id) === 'running'}
                    isPinned
                    isArchived={!!s.archived}
                    isOpen={openSessionIds.has(s.id)}
                    imChannelType={s.imChannelType}
                    onClick={() => onSelectSession(s.id)}
                    onDelete={onDeleteSession ? () => onDeleteSession(s.id) : undefined}
                    onMove={() => setMoveTargetSessionId(s.id)}
                    onTogglePin={() => void handleTogglePin(s.id)}
                    onToggleArchive={() => void handleToggleArchive(s.id)}
                  />
                ))}
              </>
            )}

            {pinned.length > 0 && unpinned.length > 0 && (
              <SegmentHeader>会话</SegmentHeader>
            )}

            {unpinned.map((s) => (
              <SessionItem
                key={s.id}
                id={s.id}
                title={s.title}
                titleEmoji={s.titleEmoji}
                titlePending={s.titlePending}
                isActive={activeSessionId === s.id}
                running={indicatorMap.get(s.id) === 'running'}
                isPinned={false}
                isArchived={!!s.archived}
                isOpen={openSessionIds.has(s.id)}
                imChannelType={s.imChannelType}
                onClick={() => onSelectSession(s.id)}
                onDelete={onDeleteSession ? () => onDeleteSession(s.id) : undefined}
                onMove={() => setMoveTargetSessionId(s.id)}
                onTogglePin={() => void handleTogglePin(s.id)}
                onToggleArchive={() => void handleToggleArchive(s.id)}
              />
            ))}
          </>
        )}
      </div>

      {/* Archived sessions toggle — only visible when ≥1 session has been archived. */}
      {(archivedCount > 0 || showArchived) && (
        <div className="px-3 pb-2 shrink-0">
          {showArchived ? (
            <button
              onClick={() => setShowArchived(false)}
              className="titlebar-no-drag w-full flex items-center gap-2 px-3 py-2 rounded-[10px] text-[12px] text-foreground/60 bg-foreground/[0.04] hover:bg-foreground/[0.07] hover:text-foreground/80 transition-colors"
            >
              <ArrowLeft size={13} className="text-foreground/50" />
              <span>返回活跃会话</span>
            </button>
          ) : (
            <button
              onClick={() => setShowArchived(true)}
              className="titlebar-no-drag w-full flex items-center gap-2 px-3 py-2 rounded-[10px] text-[12px] text-foreground/40 hover:bg-foreground/[0.04] hover:text-foreground/60 transition-colors"
            >
              <Archive size={13} className="text-foreground/30" />
              <span>已归档 ({archivedCount})</span>
            </button>
          )}
        </div>
      )}
      {moveTargetSession && (
        <MoveSessionDialog
          open={moveTargetSessionId !== null}
          onOpenChange={(open) => { if (!open) setMoveTargetSessionId(null) }}
          sessionId={moveTargetSession.id}
          currentWorkspaceId={moveTargetSession.workspaceId}
          workspaces={agentWorkspaces}
          onMoved={() => {
            setMoveTargetSessionId(null)
            void refreshWorkspaces()
          }}
        />
      )}
    </>
  )
}
