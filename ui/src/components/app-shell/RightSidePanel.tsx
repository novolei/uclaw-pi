/**
 * RightSidePanel — 右侧边栏容器（带标签页切换）
 *
 * 在 Agent 模式下显示四个标签页：文件浏览器、Agent Teams、计划文件、轨迹回放。
 * 监听全局 plan:updated 事件，自动切换到 Plan 标签页。
 */

import * as React from 'react'
import { useAtomValue, useSetAtom } from 'jotai'
import { listen } from '@tauri-apps/api/event'
import { FolderOpen, Users, ListChecks, History, Globe } from 'lucide-react'
import { appModeAtom } from '@/atoms/app-mode'
import { currentAgentSessionIdAtom, agentSessionPathMapAtom, workspaceActiveRightPanelTabMapAtom, agentSessionsAtom } from '@/atoms/agent-atoms'
import { isAutomationSession } from '@/components/workspace/WorkspaceRail'
import { activeWorkspaceIdAtom, workspaceSwitchDirectionAtom } from '@/atoms/workspace'
import { motion, AnimatePresence, type Variants } from 'motion/react'

const rightPanelSlideVariants: Variants = {
  enter: (dir: 'forward' | 'backward') => ({
    opacity: 0,
    x: dir === 'forward' ? 32 : -32,
  }),
  center: { opacity: 1, x: 0 },
  exit: (dir: 'forward' | 'backward') => ({
    opacity: 0,
    x: dir === 'forward' ? -32 : 32,
  }),
}
import { activePlanAtom } from '@/atoms/agent-teams'
import { WorkspaceFilesView } from '@/features/agent'
import { AgentTeamsPanel } from '@/components/agent/AgentTeamsPanel'
import { PlanViewer } from '@/features/agent'
import { TrajectoryReel } from '@/features/agent'
import { BrowserViewer } from '@/features/agent'
import { useWindowDragOnMove } from '@/hooks/useWindowDragOnMove'

export type ActiveTab = 'files' | 'teams' | 'plan' | 'trajectory' | 'browser'

/**
 * Which right-panel tabs to show. For automation run-sessions, teams +
 * browser are hidden — files/plan/trajectory always matter for a run, but
 * a run rarely uses teams or browser, and the precise per-run capability
 * map is a Phase 2b refinement (design §0.6).
 */
export function visibleTabs(isAutomationRun: boolean): ActiveTab[] {
  return isAutomationRun
    ? ['files', 'plan', 'trajectory']
    : ['files', 'teams', 'plan', 'trajectory', 'browser']
}

interface TabButtonProps {
  isActive: boolean
  onClick: () => void
  icon: React.ReactNode
  label: string
}

function TabButton({ isActive, onClick, icon, label }: TabButtonProps): React.ReactElement {
  const windowDrag = useWindowDragOnMove()

  return (
    <button
      onClick={(e) => {
        if (windowDrag.consumeClickIfDragged(e)) return
        onClick()
      }}
      onPointerDown={windowDrag.onPointerDown}
      onPointerMove={windowDrag.onPointerMove}
      onPointerUp={windowDrag.onPointerUp}
      onPointerCancel={windowDrag.onPointerCancel}
      className={[
        // Individual tab buttons opt out because the surrounding tab row is
        // a Tauri drag region.
        'titlebar-no-drag window-drag-tab flex items-center gap-1.5 px-2.5 py-1.5 rounded-md',
        'text-[11px] font-medium transition-colors',
        windowDrag.isDragging ? 'is-window-dragging' : '',
        isActive
          ? 'bg-primary/10 text-foreground shadow-[inset_0_0_0_1px_hsl(var(--primary)/0.2)]'
          : 'text-muted-foreground hover:text-foreground hover:bg-foreground/[0.04]',
      ].join(' ')}
      title={label}
    >
      {icon}
      <span>{label}</span>
    </button>
  )
}

interface PlanUpdatedPayload {
  filename: string
  content: string
}

export function RightSidePanel(): React.ReactElement | null {
  const appMode = useAtomValue(appModeAtom)
  const currentSessionId = useAtomValue(currentAgentSessionIdAtom)
  const sessionPathMap = useAtomValue(agentSessionPathMapAtom)
  const plan = useAtomValue(activePlanAtom)
  const setActivePlan = useSetAtom(activePlanAtom)

  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const switchDirection = useAtomValue(workspaceSwitchDirectionAtom)
  const tabMap = useAtomValue(workspaceActiveRightPanelTabMapAtom)
  const setTabMap = useSetAtom(workspaceActiveRightPanelTabMapAtom)

  const activeTab: ActiveTab = activeWorkspaceId
    ? (tabMap.get(activeWorkspaceId) ?? 'files')
    : 'files'

  const sessions = useAtomValue(agentSessionsAtom)
  const currentSession = sessions.find((s) => s.id === currentSessionId) ?? null
  const isAutomationRun = currentSession ? isAutomationSession(currentSession) : false
  const tabs = visibleTabs(isAutomationRun)
  const effectiveTab: ActiveTab = tabs.includes(activeTab) ? activeTab : 'files'

  const setActiveTab = React.useCallback((tab: ActiveTab) => {
    if (!activeWorkspaceId) return
    setTabMap((prev) => {
      const next = new Map(prev)
      next.set(activeWorkspaceId, tab)
      return next
    })
  }, [activeWorkspaceId, setTabMap])

  // Subscribe to plan:updated events. Only auto-switch the tab for the
  // currently-active workspace (which owns the agent firing the event).
  React.useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | null = null

    listen<PlanUpdatedPayload>('plan:updated', ({ payload }) => {
      setActivePlan({ filename: payload.filename, content: payload.content })
      if (activeWorkspaceId) {
        setTabMap((prev) => {
          const next = new Map(prev)
          next.set(activeWorkspaceId, 'plan')
          return next
        })
      }
    }).then((fn) => {
      if (cancelled) fn()
      else unlisten = fn
    })

    return () => {
      cancelled = true
      unlisten?.()
    }
    // setActivePlan and setTabMap are stable Jotai write-atom setters.
    // activeWorkspaceId is intentionally a dep so the closure captures
    // the current workspace at registration time.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeWorkspaceId])

  // Only show in agent mode with an active session
  if (appMode !== 'agent' || !currentSessionId) {
    return null
  }

  const sessionPath = sessionPathMap.get(currentSessionId) ?? null

  return (
    <div className="relative h-full w-[400px] flex-shrink-0 overflow-hidden bg-content-area rounded-2xl shadow-xl flex flex-col">
      {/* Tab bar — sits at the top with a small drag-only strip above
          so users can still drag the window from the panel's top edge. */}
      <div data-tauri-drag-region className="h-[8px] flex-shrink-0 titlebar-drag-region" />
      <div data-tauri-drag-region className="titlebar-drag-region flex items-center gap-1 px-2 pb-1.5 border-b border-border/40 flex-shrink-0">
        {tabs.includes('files') && (
          <TabButton
            isActive={effectiveTab === 'files'}
            onClick={() => setActiveTab('files')}
            icon={<FolderOpen size={13} />}
            label="Files"
          />
        )}
        {tabs.includes('teams') && (
          <TabButton
            isActive={effectiveTab === 'teams'}
            onClick={() => setActiveTab('teams')}
            icon={<Users size={13} />}
            label="Teams"
          />
        )}
        {tabs.includes('plan') && (
          <TabButton
            isActive={effectiveTab === 'plan'}
            onClick={() => setActiveTab('plan')}
            icon={<ListChecks size={13} />}
            label="Plan"
          />
        )}
        {tabs.includes('trajectory') && (
          <TabButton
            isActive={effectiveTab === 'trajectory'}
            onClick={() => setActiveTab('trajectory')}
            icon={<History size={13} />}
            label="Trajectory"
          />
        )}
        {tabs.includes('browser') && (
          <TabButton
            isActive={effectiveTab === 'browser'}
            onClick={() => setActiveTab('browser')}
            icon={<Globe size={13} />}
            label="Browser"
          />
        )}
      </div>

      {/* Tab content — `key` combines workspace + active tab so:
          - Workspace switch → motion exit-then-enter with the same
            directional variants as LeftSidebar + TabBar (in sync)
          - Tab change (Files → Teams etc) → motion exit-then-enter
          Same workspace + same tab → no remount (heavy children like
          WorkspaceFilesView keep their internal state). */}
      <AnimatePresence mode="wait" custom={switchDirection} initial={false}>
        <motion.div
          key={`${activeWorkspaceId ?? 'no-ws'}:${effectiveTab}`}
          custom={switchDirection}
          variants={rightPanelSlideVariants}
          initial="enter"
          animate="center"
          exit="exit"
          transition={{ duration: 0.26, ease: [0.32, 0.72, 0, 1] }}
          className="flex-1 min-h-0 overflow-auto titlebar-no-drag"
        >
          {effectiveTab === 'files' && (
            <WorkspaceFilesView sessionId={currentSessionId} sessionPath={sessionPath} />
          )}
          {effectiveTab === 'teams' && (
            <AgentTeamsPanel />
          )}
          {effectiveTab === 'plan' && (
            plan ? (
              <PlanViewer planContent={plan.content} planFilename={plan.filename} />
            ) : (
              <div className="p-3 text-[12px] text-muted-foreground">
                No active plan. The agent will create one using plan_write.
              </div>
            )
          )}
          {effectiveTab === 'trajectory' && (
            <TrajectoryReel sessionId={currentSessionId} />
          )}
          {effectiveTab === 'browser' && (
            <BrowserViewer />
          )}
        </motion.div>
      </AnimatePresence>
    </div>
  )
}
