/**
 * LeftSidebar - 左侧导航栏
 *
 * 包含：
 * - Chat/Agent 模式切换器
 * - 导航菜单项（点击切换主内容区视图）
 * - 置顶对话区域（可展开/收起）
 * - 对话列表（新对话按钮 + 右键菜单 + 按 updatedAt 降序排列）
 *
 * [Migration] Proma → uClaw: window.electronAPI 调用通过 tauri-bridge 兼容层桥接
 */

import * as React from 'react'
import { useAtom, useSetAtom, useAtomValue } from 'jotai'
import { toast } from 'sonner'
import { Pin, PinOff, Settings, Plus, Trash2, Pencil, ChevronDown, ChevronRight, Plug, Zap, ArrowRightLeft, Search, Archive, ArchiveRestore, ArrowLeft, Hammer, LoaderCircle, Bot, Palmtree } from 'lucide-react'
import { cn } from '@/lib/utils'
import { Tooltip, TooltipTrigger, TooltipContent } from '@/components/ui/tooltip'
import { useWorkspaceSwipe } from '@/hooks/useWorkspaceSwipe'
import { ModeSwitcher } from './ModeSwitcher'
import { UserAvatar } from '@/components/chat/UserAvatar'
import { activeViewAtom } from '@/atoms/active-view'
import { appModeAtom } from '@/atoms/app-mode'
import { settingsTabAtom, settingsOpenAtom } from '@/atoms/settings-tab'
import {
  conversationsAtom,
  currentConversationIdAtom,
  selectedModelAtom,
  streamingConversationIdsAtom,
  conversationModelsAtom,
  conversationContextLengthAtom,
  conversationThinkingEnabledAtom,
  conversationParallelModeAtom,
} from '@/atoms/chat-atoms'
import {
  agentSessionsAtom,
  currentAgentSessionIdAtom,
  agentSessionIndicatorMapAtom,
  unviewedCompletedSessionIdsAtom,
  workingDoneSessionIdsAtom,
  agentChannelIdAtom,
  agentModelIdAtom,
  agentSessionChannelMapAtom,
  agentSessionModelMapAtom,
  currentAgentWorkspaceIdAtom,
  agentWorkspacesAtom,
  workspaceCapabilitiesVersionAtom,
  agentSidePanelOpenMapAtom,
  agentSessionAttachedDirsMapAtom,
  workspaceAttachedDirsMapAtom,
} from '@/atoms/agent-atoms'
import type { SessionIndicatorStatus } from '@/atoms/agent-atoms'
import {
  tabsAtom,
  activeTabIdAtom,
  closeTab,
  updateTabTitle,
} from '@/atoms/tab-atoms'
import { userProfileAtom } from '@/atoms/user-profile'
import { sidebarViewModeAtom, agentSidebarTopHeightAtom } from '@/atoms/sidebar-atoms'
import { searchPaletteOpenAtom } from '@/atoms/search-atoms'
import { hasUpdateAtom } from '@/atoms/updater'
import { draftSessionIdsAtom } from '@/atoms/draft-session-atoms'
import { workingSessionGroupsAtom, workingSessionIdsSetAtom } from '@/atoms/working-atoms'
import { hasEnvironmentIssuesAtom } from '@/atoms/environment'
import { promptConfigAtom, selectedPromptIdAtom, conversationPromptIdAtom } from '@/atoms/system-prompt-atoms'
import { useOpenSession } from '@/hooks/useOpenSession'
import { useSyncActiveTabSideEffects } from '@/hooks/useSyncActiveTabSideEffects'
import { WorkspaceRail } from '@/components/workspace/WorkspaceRail'
import { WorkspaceHeader } from '@/components/workspace/WorkspaceHeader'
import { WorkspaceSwitcherBar } from '@/components/workspace/WorkspaceSwitcherBar'
import { syncWorkspaceSessionsAtom, refreshWorkspacesAtom, activeWorkspaceIdAtom, workspacesAtom, workspaceSwitchDirectionAtom, swipeGestureAtom } from '@/atoms/workspace'
import { homeOfficePanelOpenAtom } from '@/atoms/home-office-atoms'
import { topLevelViewAtom } from '@/atoms/top-level-view'
import { kaleidoscopeModuleAtom } from '@/atoms/kaleidoscope'
import { getWorkspaceIcon } from '@/lib/workspace-icons'
import { MoveSessionDialog } from '@/features/agent'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { motion, AnimatePresence, type Variants } from 'motion/react'

/**
 * Variants for the LeftSidebar workspace block's ARC-style slide.
 * `custom` carries the switch direction so the EXIT variant (which
 * runs after the element is removed from React) still knows which way
 * to slide out.
 */
/**
 * Arc-style cross-pass: the OUT-going workspace and the IN-coming
 * workspace occupy the same absolute slot and slide past each other
 * in opposite directions. Pure translation — no opacity fade — so
 * both panels stay fully visible mid-transition (the visual "two
 * spaces sliding by" effect Arc made famous).
 *
 *   forward  → outgoing slides LEFT  (-100%), incoming slides in from RIGHT (+100%)
 *   backward → outgoing slides RIGHT (+100%), incoming slides in from LEFT  (-100%)
 *
 * Mid-transition both are at ±50%, half visible each. Parent wrapper
 * needs `position: relative overflow-hidden` to clip the off-screen
 * portions.
 */
const workspaceSlideVariants: Variants = {
  enter: (dir: 'forward' | 'backward') => ({
    x: dir === 'forward' ? '100%' : '-100%',
  }),
  center: { x: '0%' },
  exit: (dir: 'forward' | 'backward') => ({
    x: dir === 'forward' ? '-100%' : '100%',
  }),
}

/**
 * Compact destination card shown while the swipe gesture is active.
 * Renders the workspace's icon + name + tilde-trimmed path so the
 * user gets immediate feedback about which workspace they're sliding
 * toward, without needing to render the full session list (the
 * workspace's session data lives in atoms keyed by the *active*
 * workspace; surfacing another workspace's data would require a
 * deeper atom refactor — out of scope for the gesture polish).
 */
function GesturePreviewCard({ workspaceId }: { workspaceId: string }): React.ReactElement | null {
  const workspaces = useAtomValue(workspacesAtom)
  const ws = workspaces.find((w) => w.id === workspaceId)
  if (!ws) return null
  const Icon = getWorkspaceIcon(ws.icon)
  const home = ws.path?.match(/^(?:\/Users\/[^/]+|\/home\/[^/]+)\/(.*)$/)
  const displayPath = home ? `~/${home[1]}` : ws.path ?? ''
  // Anchored card layout: absolute fill + flex-center (instead of relying on
  // h-full inside a flex parent, which collapsed to content-height on some
  // layouts and made the icon drift far from center).
  return (
    <div className="absolute inset-0 flex flex-col items-center justify-center px-6 text-center select-none">
      <div className="size-14 rounded-2xl bg-foreground/[0.06] ring-1 ring-foreground/[0.05] flex items-center justify-center mb-3">
        <Icon className="size-7 text-foreground/70" />
      </div>
      <div className="text-[14px] font-semibold text-foreground/90 truncate max-w-full">{ws.name}</div>
      {displayPath && (
        <div className="mt-1 text-[10.5px] text-muted-foreground/65 font-mono tabular-nums truncate max-w-full">
          {displayPath}
        </div>
      )}
    </div>
  )
}
import type { ActiveView } from '@/atoms/active-view'
import type { ConversationMeta } from '@/lib/chat-types'
import type { AgentSessionMeta, AgentWorkspace, WorkspaceCapabilities } from '@/lib/agent-types'
import { SidebarGitActions } from './SidebarGitActions'
import {
  getWorkspaceCapabilities,
  listConversations as listConversationsIPC,
  getUserProfile,
  listAgentSessions,
  createConversation as createConversationIPC,
  updateConversationTitle,
  togglePinConversation,
  toggleArchiveConversation,
  deleteConversation as deleteConversationIPC,
  deleteAgentSession,
  createAgentSession,
  updateAgentSessionTitle,
  togglePinAgentSession,
  toggleManualWorkingAgentSession,
  toggleArchiveAgentSession,
} from '@/lib/tauri-bridge'

interface SidebarItemProps {
  icon: React.ReactNode
  label: string
  active?: boolean
  suffix?: React.ReactNode
  onClick?: () => void
}

/**
 * Renders the agent-mode sidebar body with Arc-style cross-pass on
 * workspace flip + iOS-style gesture-driven slide while the user is
 * actively swiping. Three coordinated layers:
 *
 *  1. The destination preview card (only when gesture active) — slides
 *     in from the side opposite the current workspace, positioned at
 *     `offsetPx ± containerWidth` so it stays attached to the edge of
 *     the moving workspace.
 *  2. The current workspace's content inside AnimatePresence:
 *       - Gesture active → animate.x = offsetPx, transition.duration = 0
 *         (follow the finger 1:1 minus rubber band).
 *       - Gesture cleared, workspace unchanged → spring back to x: 0.
 *       - Gesture committed (workspace flipped) → AnimatePresence
 *         fires the cross-pass via the slide variants.
 */
interface SidebarBodyProps {
  activeWorkspaceId: string | null
  switchDirection: 'forward' | 'backward'
  activeTabId: string | null
  agentSessions: AgentSessionMeta[]
  handleSelectAgentSession: (id: string, title: string) => void
  handleRequestDelete: (id: string) => void
  workspaces: AgentWorkspace[]
}
function SidebarBodyWithGesture({
  activeWorkspaceId,
  switchDirection,
  activeTabId,
  agentSessions,
  handleSelectAgentSession,
  handleRequestDelete,
}: SidebarBodyProps): React.ReactElement {
  const gesture = useAtomValue(swipeGestureAtom)
  const isGestureActive = gesture !== null
  const offsetPx = gesture?.offsetPx ?? 0
  const containerWidth = gesture?.containerWidth ?? 0
  const previewWorkspaceId = gesture?.previewWorkspaceId ?? null
  // Preview is glued to the trailing edge of the current workspace:
  //   offsetPx > 0 (current sliding right) → preview enters from LEFT  (offsetPx − width)
  //   offsetPx < 0 (current sliding left)  → preview enters from RIGHT (offsetPx + width)
  const previewOffsetPx = offsetPx === 0 || !gesture
    ? 0
    : offsetPx > 0
      ? offsetPx - containerWidth
      : offsetPx + containerWidth

  // Preview opacity tracks gesture progress, so the destination card fades
  // in as the user swipes further — no horizontal translation on the card
  // itself, otherwise its content drifts past the panel centerline mid-
  // swipe and reads as "the card is in the wrong place". The bg layer DOES
  // translate (slides in from the trailing edge as a curtain reveal); the
  // card text stays anchored at panel center.
  const previewProgress = containerWidth > 0
    ? Math.min(1, Math.abs(offsetPx) / (containerWidth * 0.45))
    : 0

  return (
    <div className="relative flex-1 min-h-0 overflow-hidden">
      {previewWorkspaceId && (
        <>
          {/* Sliding background — gives the visual "another space sliding in" */}
          <div
            className="absolute inset-0 bg-background pointer-events-none"
            style={{
              transform: `translate3d(${previewOffsetPx}px, 0, 0)`,
              willChange: 'transform',
            }}
          />
          {/* Card content — anchored at panel center, fades in with gesture */}
          <div
            className="absolute inset-0 pointer-events-none"
            style={{ opacity: previewProgress, willChange: 'opacity' }}
          >
            <GesturePreviewCard workspaceId={previewWorkspaceId} />
          </div>
        </>
      )}
      <AnimatePresence custom={switchDirection} initial={false}>
        <motion.div
          key={activeWorkspaceId ?? 'no-ws'}
          custom={switchDirection}
          variants={workspaceSlideVariants}
          initial="enter"
          animate={isGestureActive ? { x: offsetPx } : 'center'}
          exit="exit"
          transition={
            isGestureActive
              ? { duration: 0 }
              : { type: 'spring', stiffness: 520, damping: 36, mass: 0.55 }
          }
          className="absolute inset-0 flex flex-col bg-background"
          style={{ willChange: 'transform' }}
        >
          <WorkspaceHeader />
          <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
            <WorkspaceRail
              activeSessionId={activeTabId}
              onSelectSession={(id) => {
                const session = agentSessions.find((s) => s.id === id)
                handleSelectAgentSession(id, session?.title ?? '')
              }}
              onDeleteSession={(id) => handleRequestDelete(id)}
            />
          </div>
        </motion.div>
      </AnimatePresence>
    </div>
  )
}

function SidebarItem({ icon, label, active, suffix, onClick }: SidebarItemProps): React.ReactElement {
  return (
    <button
      onClick={onClick}
      className={cn(
        'w-full flex items-center justify-between px-3 py-2 rounded-[10px] text-[13px] transition-colors duration-100 titlebar-no-drag',
        active
          ? 'bg-primary/10 text-foreground shadow-[0_1px_2px_0_rgba(0,0,0,0.05)]'
          : 'text-foreground/60 hover:bg-primary/5 hover:text-foreground'
      )}
    >
      <div className="flex items-center gap-3">
        <span className="flex-shrink-0 w-[18px] h-[18px]">{icon}</span>
        <span>{label}</span>
      </div>
      {suffix}
    </button>
  )
}

export interface LeftSidebarProps {
  width?: number
}

type SidebarItemId = 'pinned' | 'all-chats'

const ITEM_TO_VIEW: Record<SidebarItemId, ActiveView> = {
  pinned: 'conversations',
  'all-chats': 'conversations',
}

type DateGroup = '今天' | '昨天' | '更早'

function groupByDate<T extends { updatedAt: number }>(items: T[]): Array<{ label: DateGroup; items: T[] }> {
  const now = new Date()
  const todayStart = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
  const yesterdayStart = todayStart - 86_400_000
  const today: T[] = []
  const yesterday: T[] = []
  const earlier: T[] = []
  for (const item of items) {
    if (item.updatedAt >= todayStart) today.push(item)
    else if (item.updatedAt >= yesterdayStart) yesterday.push(item)
    else earlier.push(item)
  }
  const groups: Array<{ label: DateGroup; items: T[] }> = []
  if (today.length > 0) groups.push({ label: '今天', items: today })
  if (yesterday.length > 0) groups.push({ label: '昨天', items: yesterday })
  if (earlier.length > 0) groups.push({ label: '更早', items: earlier })
  return groups
}

export function LeftSidebar({ width }: LeftSidebarProps): React.ReactElement {
  // Magic Mouse / trackpad horizontal swipe → switch workspace, scoped
  // to this sidebar so swipes inside chat / preview / files-rail don't
  // hijack the gesture (same wrap-around as Shift+←/→ at AppShell level).
  const sidebarRef = React.useRef<HTMLDivElement | null>(null)
  useWorkspaceSwipe(sidebarRef)

  const [activeView, setActiveView] = useAtom(activeViewAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const setTopLevelView = useSetAtom(topLevelViewAtom)
  const setKaleidoscopeModule = useSetAtom(kaleidoscopeModuleAtom)
  const [, setHomeOfficeOpen] = useAtom(homeOfficePanelOpenAtom)
  const [activeItem, setActiveItem] = React.useState<SidebarItemId>('all-chats')
  const [conversations, setConversations] = useAtom(conversationsAtom)
  const [currentConversationId, setCurrentConversationId] = useAtom(currentConversationIdAtom)
  const draftSessionIds = useAtomValue(draftSessionIdsAtom)
  const setDraftSessionIds = useSetAtom(draftSessionIdsAtom)
  const [hoveredId, setHoveredId] = React.useState<string | null>(null)
  const [pendingDeleteId, setPendingDeleteId] = React.useState<string | null>(null)
  const [moveTargetId, setMoveTargetId] = React.useState<string | null>(null)
  const [pinnedExpanded, setPinnedExpanded] = React.useState(true)
  const [agentSubTab, setAgentSubTab] = React.useState<'working' | 'pinned'>('working')
  const [userProfile, setUserProfile] = useAtom(userProfileAtom)
  const selectedModel = useAtomValue(selectedModelAtom)
  const streamingIds = useAtomValue(streamingConversationIdsAtom)
  const mode = useAtomValue(appModeAtom)
  const hasUpdate = useAtomValue(hasUpdateAtom)
  const hasEnvironmentIssues = useAtomValue(hasEnvironmentIssuesAtom)
  const promptConfig = useAtomValue(promptConfigAtom)
  const setSelectedPromptId = useSetAtom(selectedPromptIdAtom)

  const [agentSessions, setAgentSessions] = useAtom(agentSessionsAtom)
  const [currentAgentSessionId, setCurrentAgentSessionId] = useAtom(currentAgentSessionIdAtom)
  const agentIndicatorMap = useAtomValue(agentSessionIndicatorMapAtom)
  const setUnviewedCompleted = useSetAtom(unviewedCompletedSessionIdsAtom)
  const agentChannelId = useAtomValue(agentChannelIdAtom)
  const agentModelId = useAtomValue(agentModelIdAtom)
  const setSessionChannelMap = useSetAtom(agentSessionChannelMapAtom)
  const setSessionModelMap = useSetAtom(agentSessionModelMapAtom)
  const currentWorkspaceId = useAtomValue(currentAgentWorkspaceIdAtom)
  const workspaces = useAtomValue(agentWorkspacesAtom)
  const wsList = useAtomValue(workspacesAtom)
  const setSessionAttachedDirsMap = useSetAtom(agentSessionAttachedDirsMapAtom)
  const setWsAttachedDirsMap = useSetAtom(workspaceAttachedDirsMapAtom)
  const [capabilities, setCapabilities] = React.useState<WorkspaceCapabilities | null>(null)
  const capabilitiesVersion = useAtomValue(workspaceCapabilitiesVersionAtom)

  const [tabs, setTabs] = useAtom(tabsAtom)
  const [activeTabId, setActiveTabId] = useAtom(activeTabIdAtom)
  const openSession = useOpenSession()
  const syncActiveTabSideEffects = useSyncActiveTabSideEffects()

  const [viewMode, setViewMode] = useAtom(sidebarViewModeAtom)
  const setSearchPaletteOpen = useSetAtom(searchPaletteOpenAtom)
  const [agentTopHeight, setAgentTopHeight] = useAtom(agentSidebarTopHeightAtom)
  const agentSplitContainerRef = React.useRef<HTMLDivElement>(null)
  const agentTopResizing = React.useRef(false)
  const agentTopResizeCleanup = React.useRef<(() => void) | null>(null)

  React.useEffect(() => { return () => { agentTopResizeCleanup.current?.() } }, [])
  React.useEffect(() => {
    if (agentTopHeight > 0) return
    const el = agentSplitContainerRef.current
    if (!el) return
    const h = el.getBoundingClientRect().height
    if (h > 0) setAgentTopHeight(Math.round(h * 0.4))
  }, [agentTopHeight, setAgentTopHeight, mode, viewMode])

  React.useEffect(() => {
    const handleBlur = (): void => setHoveredId(null)
    window.addEventListener('blur', handleBlur)
    return () => window.removeEventListener('blur', handleBlur)
  }, [])

  React.useEffect(() => {
    if (!activeTabId) return
    requestAnimationFrame(() => {
      const el = document.querySelector('.session-item-selected')
      el?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
    })
  }, [activeTabId])

  const setConvModels = useSetAtom(conversationModelsAtom)
  const setConvContextLength = useSetAtom(conversationContextLengthAtom)
  const setConvThinking = useSetAtom(conversationThinkingEnabledAtom)
  const setConvParallel = useSetAtom(conversationParallelModeAtom)
  const setConvPromptId = useSetAtom(conversationPromptIdAtom)
  const setAgentSidePanelOpen = useSetAtom(agentSidePanelOpenMapAtom)
  const setWorkingDone = useSetAtom(workingDoneSessionIdsAtom)
  const syncWorkspaceSessions = useSetAtom(syncWorkspaceSessionsAtom)
  const refreshWorkspaces = useSetAtom(refreshWorkspacesAtom)
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const switchDirection = useAtomValue(workspaceSwitchDirectionAtom)
  const setCurrentAgentWorkspaceId = useSetAtom(currentAgentWorkspaceIdAtom)

  const cleanupMapAtoms = React.useCallback((id: string) => {
    const deleteKey = <T,>(prev: Map<string, T>): Map<string, T> => {
      if (!prev.has(id)) return prev
      const map = new Map(prev)
      map.delete(id)
      return map
    }
    setConvModels(deleteKey)
    setConvContextLength(deleteKey)
    setConvThinking(deleteKey)
    setConvParallel(deleteKey)
    setConvPromptId(deleteKey)
    setAgentSidePanelOpen(deleteKey)
    setSessionChannelMap(deleteKey)
    setSessionModelMap(deleteKey)
  }, [setConvModels, setConvContextLength, setConvThinking, setConvParallel, setConvPromptId, setAgentSidePanelOpen, setSessionChannelMap, setSessionModelMap])

  React.useEffect(() => {
    syncWorkspaceSessions(agentSessions as any)
  }, [agentSessions, syncWorkspaceSessions])

  const workspaceNameMap = React.useMemo(() => {
    const map = new Map<string, string>()
    for (const w of workspaces) map.set(w.id, w.name)
    return map
  }, [workspaces])

  React.useEffect(() => {
    if (!currentWorkspaceId || mode !== 'agent') { setCapabilities(null); return }
    getWorkspaceCapabilities(currentWorkspaceId).then(setCapabilities).catch(console.error)
  }, [currentWorkspaceId, mode, activeView, capabilitiesVersion])

  const pinnedConversations = React.useMemo(
    () => viewMode === 'active' ? conversations.filter((c) => c.pinned && !draftSessionIds.has(c.id)) : [],
    [conversations, viewMode, draftSessionIds]
  )

  const workingGroups = useAtomValue(workingSessionGroupsAtom)
  const workingSessionIds = useAtomValue(workingSessionIdsSetAtom)
  const hasWorkingSessions = workingGroups.todo.length > 0 || workingGroups.running.length > 0 || workingGroups.done.length > 0

  const pinnedAgentSessions = React.useMemo(
    () => viewMode === 'active' ? agentSessions.filter((s) => s.pinned && !draftSessionIds.has(s.id) && !workingSessionIds.has(s.id) && (!currentWorkspaceId || s.workspaceId === currentWorkspaceId)) : [],
    [agentSessions, viewMode, draftSessionIds, currentWorkspaceId, workingSessionIds]
  )

  const prevActiveTabIdForSubTab = React.useRef<string | null>(activeTabId)
  React.useEffect(() => {
    if (activeTabId === prevActiveTabIdForSubTab.current) return
    prevActiveTabIdForSubTab.current = activeTabId
    if (mode !== 'agent' || viewMode !== 'active' || !activeTabId) return
    if (pinnedAgentSessions.some((s) => s.id === activeTabId)) setAgentSubTab('pinned')
    else if (workingSessionIds.has(activeTabId)) setAgentSubTab('working')
  }, [activeTabId, mode, viewMode, pinnedAgentSessions, workingSessionIds])

  const conversationGroups = React.useMemo(() => {
    const filtered = viewMode === 'archived'
      ? conversations.filter((c) => c.archived && !draftSessionIds.has(c.id))
      : conversations.filter((c) => !c.archived && !c.pinned && !draftSessionIds.has(c.id))
    return groupByDate(filtered)
  }, [conversations, viewMode, draftSessionIds])

  const archivedConversationCount = React.useMemo(() => conversations.filter((c) => c.archived).length, [conversations])
  const archivedAgentSessionCount = React.useMemo(
    () => agentSessions.filter((s) => s.archived && (!currentWorkspaceId || s.workspaceId === currentWorkspaceId)).length,
    [agentSessions, currentWorkspaceId]
  )

  // 初始加载 workspaces + active workspace ID
  React.useEffect(() => {
    refreshWorkspaces()
  }, [refreshWorkspaces])

  // 同步 activeWorkspaceIdAtom → currentAgentWorkspaceIdAtom（两个原子统一）
  React.useEffect(() => {
    setCurrentAgentWorkspaceId(activeWorkspaceId)
  }, [activeWorkspaceId, setCurrentAgentWorkspaceId])

  // 初始加载对话列表 + 用户档案 + Agent 会话
  React.useEffect(() => {
    listConversationsIPC().then((list: any) => setConversations(list as any)).catch(console.error)
    getUserProfile().then(setUserProfile).catch(console.error)
    listAgentSessions().then((sessions) => {
      setAgentSessions(sessions)
      // Phase 2: hydrate agentSessionAttachedDirsMapAtom from session data
      const map = new Map<string, string[]>()
      for (const s of sessions) {
        if (Array.isArray(s.attachedDirs) && s.attachedDirs.length > 0) {
          map.set(s.id, s.attachedDirs as string[])
        }
      }
      if (map.size > 0) setSessionAttachedDirsMap(map)
    }).catch(console.error)
  }, [setConversations, setUserProfile, setAgentSessions, setSessionAttachedDirsMap])

  // Phase 2: hydrate workspaceAttachedDirsMapAtom whenever workspacesAtom changes
  React.useEffect(() => {
    if (wsList.length === 0) return
    const map = new Map<string, string[]>()
    for (const w of wsList) {
      if (Array.isArray(w.attachedDirs) && w.attachedDirs.length > 0) {
        map.set(w.id, w.attachedDirs)
      }
    }
    setWsAttachedDirsMap(map)
  }, [wsList, setWsAttachedDirsMap])

  React.useEffect(() => {
    const handleFocus = (): void => {
      listConversationsIPC().then((list) => setConversations(list as any)).catch(console.error)
      listAgentSessions().then(setAgentSessions).catch(console.error)
    }
    window.addEventListener('focus', handleFocus)
    return () => window.removeEventListener('focus', handleFocus)
  }, [setConversations, setAgentSessions])

  const handleItemClick = (item: SidebarItemId): void => {
    if (item === 'pinned') { setPinnedExpanded((prev) => !prev); return }
    setActiveItem(item)
    setActiveView(ITEM_TO_VIEW[item])
  }

  React.useEffect(() => { setViewMode('active') }, [mode, setViewMode])

  const handleNewConversation = async (): Promise<void> => {
    try {
      const meta = await createConversationIPC({ title: undefined, modelId: selectedModel?.modelId, channelId: selectedModel?.channelId } as any)
      setConversations((prev: any) => [meta, ...prev])
      openSession('chat', meta.id, meta.title)
      setActiveView('conversations')
      setActiveItem('all-chats')
      if (promptConfig.defaultPromptId) setSelectedPromptId(promptConfig.defaultPromptId)
    } catch (error) { console.error('[侧边栏] 创建对话失败:', error) }
  }

  const handleSelectConversation = (id: string, title: string): void => {
    openSession('chat', id, title)
    setActiveView('conversations')
    setActiveItem('all-chats')
  }

  const handleRequestDelete = (id: string): void => { setPendingDeleteId(id) }

  const handleRename = async (id: string, newTitle: string): Promise<void> => {
    try {
      const updated = await updateConversationTitle(id, newTitle)
      setConversations((prev: any) => prev.map((c: any) => (c.id === updated.id ? updated : c)))
      setTabs((prev) => updateTabTitle(prev, id, newTitle))
    } catch (error) { console.error('[侧边栏] 重命名对话失败:', error) }
  }

  const handleTogglePin = async (id: string): Promise<void> => {
    try {
      const original = conversations.find((c) => c.id === id)
      const updated = await togglePinConversation(id)
      setConversations((prev: any) => prev.map((c: any) => (c.id === updated.id ? updated : c)))
      if (original?.archived && updated.pinned && !updated.archived) toast.success('已取消归档并置顶')
    } catch (error) { console.error('[侧边栏] 切换置顶失败:', error) }
  }

  const handleToggleArchive = async (id: string): Promise<void> => {
    try {
      const archivedAt = await toggleArchiveConversation(id)
      const isNowArchived = archivedAt !== null
      setConversations((prev: any) => prev.map((c: any) => (c.id === id ? { ...c, archived: isNowArchived, archivedAt } : c)))
      if (isNowArchived) {
        const wasActive = activeTabId === id
        const tabResult = closeTab(tabs, activeTabId, id)
        setTabs(tabResult.tabs)
        setActiveTabId(tabResult.activeTabId)
        cleanupMapAtoms(id)
        if (wasActive) {
          const newActiveTab = tabResult.activeTabId ? tabResult.tabs.find((t) => t.id === tabResult.activeTabId) ?? null : null
          syncActiveTabSideEffects(newActiveTab)
        }
      }
      toast.success(isNowArchived ? '已归档' : '已取消归档')
    } catch (error) { console.error('[侧边栏] 切换归档失败:', error) }
  }

  const handleConfirmDelete = async (): Promise<void> => {
    if (!pendingDeleteId) return
    const wasActive = activeTabId === pendingDeleteId
    const tabResult = closeTab(tabs, activeTabId, pendingDeleteId)
    setTabs(tabResult.tabs)
    setActiveTabId(tabResult.activeTabId)
    if (wasActive) {
      const newActiveTab = tabResult.activeTabId ? tabResult.tabs.find((t) => t.id === tabResult.activeTabId) ?? null : null
      syncActiveTabSideEffects(newActiveTab)
    }
    setDraftSessionIds((prev: Set<string>) => { if (!prev.has(pendingDeleteId)) return prev; const next = new Set(prev); next.delete(pendingDeleteId); return next })
    cleanupMapAtoms(pendingDeleteId)
    setWorkingDone((prev) => { if (!prev.has(pendingDeleteId)) return prev; const next = new Set(prev); next.delete(pendingDeleteId); return next })

    if (mode === 'agent') {
      try {
        const deleted = await deleteAgentSession(pendingDeleteId)
        if (!deleted) {
          toast.error('删除失败：未找到该会话')
        }
        const sessions = await listAgentSessions()
        setAgentSessions(sessions)
      } catch (error) {
        const msg = error instanceof Error ? error.message : String(error)
        console.error('[侧边栏] 删除 Agent 会话失败:', error)
        toast.error(`删除失败：${msg}`)
        // Backend failed — refresh from disk so the (still-present) session
        // reappears in the list. Otherwise the user is left thinking the
        // delete worked because the local list was optimistically cleared.
        try {
          const sessions = await listAgentSessions()
          setAgentSessions(sessions)
        } catch { /* ignore */ }
      } finally { setPendingDeleteId(null) }
      return
    }
    try {
      await deleteConversationIPC(pendingDeleteId)
      const conversations = await listConversationsIPC()
      setConversations(conversations as any)
    } catch (error) {
      console.error('[侧边栏] 删除对话失败:', error)
      setConversations((prev: any) => prev.filter((c: any) => c.id !== pendingDeleteId))
    } finally { setPendingDeleteId(null) }
  }

  const handleNewAgentSession = async (): Promise<void> => {
    try {
      const meta = await createAgentSession(undefined, agentChannelId || undefined, currentWorkspaceId || undefined)
      // Guard against a null/invalid meta so it never enters agentSessions —
      // a null session crashes working-atoms `sessions.map(s => [s.id, …])`.
      if (!meta?.id) {
        console.error('[侧边栏] createAgentSession 返回空 meta，跳过创建')
        return
      }
      setAgentSessions((prev: any) => [meta, ...prev])
      if (agentChannelId) setSessionChannelMap((prev) => { const map = new Map(prev); map.set(meta.id, agentChannelId); return map })
      if (agentModelId) setSessionModelMap((prev) => { const map = new Map(prev); map.set(meta.id, agentModelId); return map })
      openSession('agent', meta.id, meta.title)
      setActiveView('conversations')
      setActiveItem('all-chats')
    } catch (error) { console.error('[侧边栏] 创建 Agent 会话失败:', error) }
  }

  const handleSelectAgentSession = (id: string, title: string): void => {
    openSession('agent', id, title)
    setActiveView('conversations')
    setActiveItem('all-chats')
    setUnviewedCompleted((prev: Set<string>) => { if (!prev.has(id)) return prev; const next = new Set(prev); next.delete(id); return next })
  }

  const handleAgentRename = async (id: string, newTitle: string): Promise<void> => {
    try {
      const updated = await updateAgentSessionTitle(id, newTitle)
      setAgentSessions((prev: any) => prev.map((s: any) => (s.id === updated.id ? updated : s)))
      setTabs((prev) => updateTabTitle(prev, id, newTitle))
    } catch (error) { console.error('[侧边栏] 重命名 Agent 会话失败:', error) }
  }

  const handleTogglePinAgent = async (id: string): Promise<void> => {
    try {
      const newPinnedAt = await togglePinAgentSession(id)
      setAgentSessions((prev: any) => prev.map((s: any) => (s.id === id ? { ...s, pinnedAt: newPinnedAt } : s)))
    } catch (error) { console.error('[侧边栏] 切换 Agent 会话置顶失败:', error) }
  }

  const handleToggleManualWorkingAgent = async (id: string): Promise<void> => {
    try {
      const isCurrentlyInWorking = workingSessionIds.has(id)
      if (isCurrentlyInWorking) {
        const session = agentSessions.find((s) => s.id === id)
        if (session?.manualWorking) {
          const updated = await toggleManualWorkingAgentSession(id)
          setAgentSessions((prev: any) => prev.map((s: any) => (s.id === updated.id ? updated : s)))
        }
        setWorkingDone((prev) => { if (!prev.has(id)) return prev; const next = new Set(prev); next.delete(id); return next })
      } else {
        const original = agentSessions.find((s) => s.id === id)
        const updated = await toggleManualWorkingAgentSession(id)
        setAgentSessions((prev: any) => prev.map((s: any) => (s.id === updated.id ? updated : s)))
        if (original?.archived && updated.manualWorking && !updated.archived) toast.success('已取消归档并标记为工作中')
      }
    } catch (error) { console.error('[Sidebar] Failed to toggle manual working:', error); toast.error('操作失败') }
  }

  const handleToggleArchiveAgent = async (id: string): Promise<void> => {
    try {
      const archivedAt = await toggleArchiveAgentSession(id)
      const isNowArchived = archivedAt !== null
      setAgentSessions((prev: any) => prev.map((s: any) => (s.id === id ? { ...s, archived: isNowArchived, archivedAt } : s)))
      if (isNowArchived) {
        const wasActive = activeTabId === id
        const tabResult = closeTab(tabs, activeTabId, id)
        setTabs(tabResult.tabs)
        setActiveTabId(tabResult.activeTabId)
        cleanupMapAtoms(id)
        setWorkingDone((prev) => { if (!prev.has(id)) return prev; const next = new Set(prev); next.delete(id); return next })
        if (wasActive) {
          const newActiveTab = tabResult.activeTabId ? tabResult.tabs.find((t) => t.id === tabResult.activeTabId) ?? null : null
          syncActiveTabSideEffects(newActiveTab)
        }
      }
      toast.success(isNowArchived ? '已归档' : '已取消归档')
    } catch (error) { console.error('[侧边栏] 切换 Agent 会话归档失败:', error) }
  }

  const handleSessionMoved = (updatedSession: AgentSessionMeta, targetWorkspaceName: string): void => {
    setAgentSessions((prev: any) => prev.map((s: any) => (s.id === updatedSession.id ? updatedSession : s)))
    if (currentAgentSessionId === updatedSession.id) {
      const tabResult = closeTab(tabs, activeTabId, updatedSession.id)
      setTabs(tabResult.tabs)
      setActiveTabId(tabResult.activeTabId)
      setCurrentAgentSessionId(null)
      setWorkingDone((prev) => { if (!prev.has(updatedSession.id)) return prev; const next = new Set(prev); next.delete(updatedSession.id); return next })
    }
    setMoveTargetId(null)
    toast.success('会话已迁移', { description: `已迁移到「${targetWorkspaceName}」，请切换工作区查看` })
  }

  const filteredAgentSessions = React.useMemo(() => {
    const byWorkspace = agentSessions.filter((s) => s.workspaceId === currentWorkspaceId && !draftSessionIds.has(s.id))
    return viewMode === 'archived'
      ? byWorkspace.filter((s) => s.archived)
      : byWorkspace.filter((s) => !s.archived && !s.pinned && !workingSessionIds.has(s.id))
  }, [agentSessions, currentWorkspaceId, viewMode, draftSessionIds, workingSessionIds])

  const agentSessionGroups = React.useMemo(() => groupByDate(filteredAgentSessions), [filteredAgentSessions])

  const handleAgentTopResizeStart = React.useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    const container = agentSplitContainerRef.current
    if (!container) return
    agentTopResizing.current = true
    const startY = e.clientY
    const startH = Math.max(0, agentTopHeight)
    const containerHeight = container.getBoundingClientRect().height
    const minH = 80
    const maxH = Math.max(minH, Math.floor(containerHeight * 0.7))
    const onMove = (ev: MouseEvent): void => {
      if (!agentTopResizing.current) return
      const delta = ev.clientY - startY
      setAgentTopHeight(Math.min(maxH, Math.max(minH, startH + delta)))
    }
    const onUp = (): void => {
      agentTopResizing.current = false
      document.removeEventListener('mousemove', onMove)
      document.removeEventListener('mouseup', onUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      agentTopResizeCleanup.current = null
    }
    document.addEventListener('mousemove', onMove)
    document.addEventListener('mouseup', onUp)
    document.body.style.cursor = 'row-resize'
    document.body.style.userSelect = 'none'
    agentTopResizeCleanup.current = onUp
  }, [agentTopHeight, setAgentTopHeight])

  // ===== Delete-confirmation modal =====
  // Visual language mirrors the approval modal (PR #85): rounded-2xl
  // dialog with a hero header (colored disc icon + title + supporting
  // line + risk-tinted pill) over a detail card that previews exactly
  // what's being deleted. The previous flat 2-line text dialog gave the
  // user no way to verify they were about to delete the right session.
  //
  // Detail card content depends on mode:
  //   - 'agent' → look up the session in agentSessions; show emoji,
  //     title, workspace name (if known), message count, last-updated
  //   - 'chat'  → look up in conversations; show title, last-updated
  // Falls back to a bare "确认删除" if the row is missing (race after
  // a refetch — still allows the user to confirm rather than blocking).
  const pendingDeleteAgentSession = pendingDeleteId
    ? agentSessions.find((s) => s.id === pendingDeleteId)
    : undefined
  const pendingDeleteConversation = pendingDeleteId
    ? conversations.find((c) => c.id === pendingDeleteId)
    : undefined
  const pendingDeleteWorkspace = pendingDeleteAgentSession?.workspaceId
    ? workspaces.find((w) => w.id === pendingDeleteAgentSession.workspaceId)
    : undefined

  const deleteDialog = (
    <AlertDialog open={pendingDeleteId !== null} onOpenChange={(open) => { if (!open) setPendingDeleteId(null) }}>
      <AlertDialogContent
        className="sm:max-w-md overflow-hidden p-0 rounded-2xl sm:rounded-2xl [&>*]:min-w-0"
        onKeyDown={(e) => { if (e.key === 'Enter') { e.preventDefault(); handleConfirmDelete() } }}
      >
        <div className="p-5 space-y-4">
          <AlertDialogHeader>
            <div className="flex items-start gap-3">
              <div
                className="shrink-0 inline-flex items-center justify-center size-10 rounded-xl bg-danger-bg text-danger"
                aria-hidden
              >
                <Trash2 className="size-5" />
              </div>
              <div className="min-w-0 flex-1">
                <AlertDialogTitle className="text-base font-semibold leading-tight text-foreground">
                  {mode === 'agent' ? '删除会话' : '删除对话'}
                </AlertDialogTitle>
                <AlertDialogDescription className="text-[12.5px] mt-0.5 text-muted-foreground">
                  此操作无法撤销。该{mode === 'agent' ? '会话' : '对话'}及其所有消息将被永久删除。
                </AlertDialogDescription>
              </div>
              <span
                className="shrink-0 inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[10.5px] font-semibold uppercase tracking-wide bg-danger-bg text-danger"
              >
                <span className="size-1.5 rounded-full bg-danger" aria-hidden />
                危险
              </span>
            </div>
          </AlertDialogHeader>

          {/* Detail card — shows which row is being deleted. */}
          {(pendingDeleteAgentSession || pendingDeleteConversation) && (
            <div className="relative rounded-lg border border-border/60 bg-muted/30 overflow-hidden">
              <div className="absolute left-0 top-0 bottom-0 w-[3px] bg-danger" aria-hidden />
              <div className="pl-4 pr-3 py-3 space-y-1.5">
                {/* Title row */}
                <div className="flex items-center gap-2">
                  {pendingDeleteAgentSession ? (
                    <span className="text-[14px] leading-none shrink-0" style={{ fontFamily: "'Noto Emoji', sans-serif" }}>
                      {pendingDeleteAgentSession.titleEmoji || '💬'}
                    </span>
                  ) : null}
                  <span className="text-[13px] font-medium text-foreground truncate">
                    {pendingDeleteAgentSession?.title ?? pendingDeleteConversation?.title ?? '未命名'}
                  </span>
                </div>
                {/* Metadata rows */}
                <div className="space-y-0.5 text-[11.5px] text-muted-foreground/85 font-mono">
                  {pendingDeleteWorkspace && (
                    <div className="flex gap-2">
                      <span className="text-muted-foreground/60 shrink-0 min-w-[4rem]">工作区</span>
                      <span className="truncate">{pendingDeleteWorkspace.name}</span>
                    </div>
                  )}
                  {pendingDeleteAgentSession && (
                    <div className="flex gap-2">
                      <span className="text-muted-foreground/60 shrink-0 min-w-[4rem]">消息数</span>
                      <span>{pendingDeleteAgentSession.messageCount}</span>
                    </div>
                  )}
                  <div className="flex gap-2">
                    <span className="text-muted-foreground/60 shrink-0 min-w-[4rem]">会话 ID</span>
                    <span className="truncate" title={pendingDeleteId ?? ''}>{pendingDeleteId ?? ''}</span>
                  </div>
                </div>
              </div>
            </div>
          )}

          <AlertDialogFooter className="flex-row gap-2 sm:justify-end">
            <AlertDialogCancel asChild>
              <Button variant="ghost" disabled={false}>
                取消
              </Button>
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={handleConfirmDelete}
              className="bg-danger text-white hover:bg-danger/90"
            >
              <Trash2 className="size-3.5 mr-1" />
              删除
            </AlertDialogAction>
          </AlertDialogFooter>
        </div>
      </AlertDialogContent>
    </AlertDialog>
  )

  const moveDialog = (
    <MoveSessionDialog
      open={moveTargetId !== null}
      onOpenChange={(open) => { if (!open) setMoveTargetId(null) }}
      sessionId={moveTargetId ?? ''}
      currentWorkspaceId={currentWorkspaceId ?? undefined}
      workspaces={workspaces}
      onMoved={handleSessionMoved}
    />
  )

  // ===== 展开状态 =====
  return (
    <div ref={sidebarRef} className="relative h-full flex flex-col bg-background rounded-2xl shadow-xl transition-[width] duration-300" style={{ width: width ?? 280, minWidth: 180, flexShrink: 1 }}>
      {/* 顶部独立拖拽条：absolute + z-1 让它叠在 sidebar 内容之上，
          WKWebView 的文本选择层不会再抢占拖拽。 */}
      <div data-tauri-drag-region aria-hidden="true" className="sidebar-window-drag-strip h-[30px]" />
      {/* 占位高度，让正常 flex 流仍然预留 30px。 */}
      <div className="h-[30px] flex-shrink-0" aria-hidden="true" />
      <div>
        <div className="flex items-start gap-1.5 px-3">
          <div className="flex-1 min-w-0"><ModeSwitcher /></div>
        </div>
      </div>

      <div className="px-3 pt-2 flex items-center gap-1.5">
        <button onClick={mode === 'agent' ? handleNewAgentSession : handleNewConversation} className="flex-1 flex items-center gap-2 px-3 py-2 rounded-[10px] text-[13px] font-medium text-foreground/70 bg-primary/5 hover:bg-primary/10 transition-colors duration-100 titlebar-no-drag border border-dashed border-[hsl(var(--dashed-border))] hover:border-[hsl(var(--dashed-border-hover))]">
          <Plus size={14} />
          <span>{mode === 'agent' ? '新会话' : '新对话'}</span>
        </button>
        <Tooltip>
          <TooltipTrigger asChild>
            <button onClick={() => setSearchPaletteOpen(true)} className="flex-shrink-0 size-[36px] flex items-center justify-center rounded-[10px] text-foreground/40 bg-primary/5 hover:bg-primary/10 hover:text-foreground/60 transition-colors duration-100 titlebar-no-drag border border-dashed border-[hsl(var(--dashed-border))] hover:border-[hsl(var(--dashed-border-hover))]">
              <Search size={14} />
            </button>
          </TooltipTrigger>
          <TooltipContent side="bottom">搜索 (⌘K)</TooltipContent>
        </Tooltip>
      </div>

      {mode === 'chat' && (
        <div className="flex flex-col gap-1 pt-3 px-3">
          <SidebarItem icon={<Pin size={16} />} label="置顶对话"
            suffix={pinnedConversations.length > 0 ? (pinnedExpanded ? <ChevronDown size={14} className="text-foreground/40" /> : <ChevronRight size={14} className="text-foreground/40" />) : undefined}
            onClick={() => handleItemClick('pinned')} />
        </div>
      )}

      {mode === 'chat' && pinnedExpanded && pinnedConversations.length > 0 && (
        <div className="px-3 pt-1 pb-1">
          <div className="flex flex-col gap-0.5 pl-1 border-l-2 border-primary/20 ml-2">
            {pinnedConversations.map((conv) => (
              <ConversationItem key={`pinned-${conv.id}`} conversation={conv} active={conv.id === activeTabId} hovered={conv.id === hoveredId} streaming={streamingIds.has(conv.id)} showPinIcon={false}
                onSelect={() => handleSelectConversation(conv.id, conv.title)} onRequestDelete={() => handleRequestDelete(conv.id)} onRename={handleRename} onTogglePin={handleTogglePin} onToggleArchive={handleToggleArchive}
                onMouseEnter={() => setHoveredId(conv.id)} onMouseLeave={() => setHoveredId(null)} />
            ))}
          </div>
        </div>
      )}

      {/* 主内容区：对话/会话列表 */}
      {mode === 'agent' ? (
        // Three rendering modes share the same absolute slot:
        //   1. Idle: AnimatePresence cross-pass on workspace flip.
        //   2. Active swipe gesture: current sidebar follows the finger
        //      (rubber-band damped); a "destination card" of the preview
        //      workspace slides in alongside.
        //   3. Snap-back: gesture cleared without commit → spring back
        //      to translateX 0.
        <SidebarBodyWithGesture
          activeWorkspaceId={activeWorkspaceId ?? null}
          switchDirection={switchDirection}
          activeTabId={activeTabId ?? null}
          agentSessions={agentSessions}
          handleSelectAgentSession={handleSelectAgentSession}
          handleRequestDelete={handleRequestDelete}
          workspaces={workspaces}
        />
      ) : (
        <div className="flex-1 overflow-y-auto px-3 pt-2 pb-3 scrollbar-none">
          {conversationGroups.map((group) => (
            <div key={group.label} className="mb-1">
              <div className="px-3 pt-2 pb-1 text-[11px] font-medium text-foreground/40 select-none">{group.label}</div>
              <div className="flex flex-col gap-0.5">
                {group.items.map((conv) => (
                  <ConversationItem key={conv.id} conversation={conv} active={conv.id === activeTabId} hovered={conv.id === hoveredId} streaming={streamingIds.has(conv.id)} showPinIcon={!!conv.pinned}
                    onSelect={() => handleSelectConversation(conv.id, conv.title)} onRequestDelete={() => handleRequestDelete(conv.id)} onRename={handleRename} onTogglePin={handleTogglePin} onToggleArchive={handleToggleArchive}
                    onMouseEnter={() => setHoveredId(conv.id)} onMouseLeave={() => setHoveredId(null)} />
                ))}
              </div>
            </div>
          ))}
        </div>
      )}

      {/* 归档入口 */}
      <div className="px-3 pb-1">
        {viewMode === 'active' ? (
          <>
            {mode === 'chat' && archivedConversationCount > 0 && (
              <button onClick={() => setViewMode('archived')} className="w-full flex items-center gap-2 px-3 py-2 rounded-[10px] text-[12px] text-foreground/40 hover:bg-foreground/[0.04] hover:text-foreground/60 transition-colors titlebar-no-drag">
                <Archive size={13} className="text-foreground/30" /><span>已归档 ({archivedConversationCount})</span>
              </button>
            )}
          </>
        ) : (
          <button onClick={() => setViewMode('active')} className="w-full flex items-center gap-2 px-3 py-2 rounded-[10px] text-[12px] text-foreground/60 bg-foreground/[0.04] hover:bg-foreground/[0.07] hover:text-foreground/80 transition-colors titlebar-no-drag">
            <ArrowLeft size={13} className="text-foreground/50" /><span>返回活跃{mode === 'agent' ? '会话' : '对话'}</span>
          </button>
        )}
      </div>

      {/* Agent 模式：工作区能力指示器 + 提交按钮
          PR 2026-05-13 sidebar-git-actions relocation: the MCP·Skills
          summary now shares this row with the `提交 ▾` affordance,
          separated by a hairline divider rendered inside
          `<SidebarGitActions />`. The git side renders `null` (divider
          included) when no workspace cwd is set, so the row degrades
          to MCP·Skills alone with no orphan line. */}
      {mode === 'agent' && capabilities && (
        <div className="px-3 pb-1">
          <div className="flex items-stretch gap-2">
            <Tooltip>
              <TooltipTrigger asChild>
                <button onClick={() => { setSettingsTab('intelligence'); setSettingsOpen(true) }} className="flex-1 min-w-0 flex items-center gap-3 px-3 py-2 rounded-[10px] text-[12px] text-foreground/50 hover:bg-foreground/[0.04] hover:text-foreground/70 transition-colors titlebar-no-drag">
                  <div className="flex items-center gap-2.5 flex-1 min-w-0">
                    <span className="flex items-center gap-1"><Plug size={13} className="text-foreground/40" /><span className="tabular-nums">{capabilities.mcpServers.filter((s) => s.enabled).length}</span><span className="text-foreground/30">MCP</span></span>
                    <span className="text-foreground/20">·</span>
                    <span className="flex items-center gap-1"><Zap size={13} className="text-foreground/40" /><span className="tabular-nums">{capabilities.skills.length}</span><span className="text-foreground/30">Skills</span></span>
                  </div>
                </button>
              </TooltipTrigger>
              <TooltipContent side="top">点击配置 MCP 与 Skills</TooltipContent>
            </Tooltip>
            <SidebarGitActions />
          </div>
        </div>
      )}

      {/* Per-workspace Automations entry. Visually grouped with the active
          workspace's content (above the cross-workspace switcher bar) to
          imply "this workspace's automations". */}
      {mode === 'agent' && (
        <div className="px-3 pb-1">
          <button
            type="button"
            onClick={() => { setKaleidoscopeModule('humans'); setTopLevelView('kaleidoscope') }}
            className="w-full flex items-center gap-2 px-3 py-1.5 rounded-md
                       text-[12px] text-foreground/60 hover:text-foreground
                       hover:bg-foreground/[0.04] transition-colors titlebar-no-drag"
            title="Automations"
          >
            <Bot className="size-3.5 shrink-0" />
            <span className="flex-1 text-left">Automations</span>
          </button>
        </div>
      )}
      {mode === 'agent' && (
        <div className="px-3 pb-1">
          <button
            type="button"
            onClick={() => setHomeOfficeOpen(true)}
            className="w-full flex items-center gap-2 px-3 py-1.5 rounded-md
                       text-[12px] text-foreground/60 hover:text-foreground
                       hover:bg-foreground/[0.04] transition-colors titlebar-no-drag"
            title="Home Office"
          >
            <Palmtree className="size-3.5 shrink-0" />
            <span className="flex-1 text-left">Home Office</span>
          </button>
        </div>
      )}

      {/* Phase 4b: workspace switcher bar sits ABOVE the user/settings row.
          Per spec §4.8 — workspace switcher is per-app context; user/settings
          is cross-workspace global identity (Apple Mail / macOS convention
          anchors global identity at the absolute bottom). */}
      {mode === 'agent' && <WorkspaceSwitcherBar />}

      {/* 底部：用户资料 + 设置入口 */}
      <div className="px-3 pb-3 pt-2">
        <button onClick={() => setSettingsOpen(true)} className="w-full flex items-center gap-3 px-3 py-2 rounded-[10px] transition-colors titlebar-no-drag text-foreground/70 hover:bg-foreground/[0.04] hover:text-foreground">
          <UserAvatar avatar={userProfile.avatar} size={28} />
          <span className="flex-1 text-sm truncate text-left">{userProfile.userName}</span>
          <div className="relative flex-shrink-0 text-foreground/40">
            <Settings size={16} />
            {(hasUpdate || hasEnvironmentIssues) && <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-red-500" />}
          </div>
        </button>
      </div>

      {deleteDialog}
      {moveDialog}

    </div>
  )
}

// ===== 对话列表项 =====
interface ConversationItemProps {
  conversation: ConversationMeta
  active: boolean
  hovered: boolean
  streaming: boolean
  showPinIcon: boolean
  onSelect: () => void
  onRequestDelete: () => void
  onRename: (id: string, newTitle: string) => Promise<void>
  onTogglePin: (id: string) => Promise<void>
  onToggleArchive: (id: string) => Promise<void>
  onMouseEnter: () => void
  onMouseLeave: () => void
}

function ConversationItem({ conversation, active, hovered, streaming, showPinIcon, onSelect, onRequestDelete, onRename, onTogglePin, onToggleArchive, onMouseEnter, onMouseLeave }: ConversationItemProps): React.ReactElement {
  const [editing, setEditing] = React.useState(false)
  const [editTitle, setEditTitle] = React.useState('')
  const inputRef = React.useRef<HTMLInputElement>(null)
  const justStartedEditing = React.useRef(false)

  const startEdit = (): void => {
    setEditTitle(conversation.title); setEditing(true); justStartedEditing.current = true
    setTimeout(() => { justStartedEditing.current = false; inputRef.current?.focus(); inputRef.current?.select() }, 300)
  }
  const saveTitle = async (): Promise<void> => {
    if (justStartedEditing.current) return
    const trimmed = editTitle.trim()
    if (!trimmed || trimmed === conversation.title) { setEditing(false); return }
    await onRename(conversation.id, trimmed); setEditing(false)
  }
  const handleKeyDown = (e: React.KeyboardEvent): void => { if (e.key === 'Enter') { e.preventDefault(); saveTitle() } else if (e.key === 'Escape') setEditing(false) }
  const isPinned = !!conversation.pinned

  return (
    <div role="button" tabIndex={0} onClick={onSelect} onDoubleClick={(e) => { e.stopPropagation(); startEdit() }} onMouseEnter={onMouseEnter} onMouseLeave={onMouseLeave}
      className={cn('relative w-full flex items-center gap-2 px-3 py-[7px] rounded-[10px] transition-colors duration-100 titlebar-no-drag text-left', active ? 'session-item-selected bg-primary/10 shadow-[0_1px_2px_0_rgba(0,0,0,0.05)]' : 'hover:bg-primary/5')}>
      {streaming && <span className="absolute left-1 top-1.5 bottom-1.5 w-[2px] rounded-full bg-emerald-500 animate-pulse pointer-events-none" aria-hidden="true" />}
      <div className="flex-1 min-w-0">
        {editing ? (
          <input ref={inputRef} value={editTitle} onChange={(e) => setEditTitle(e.target.value)} onKeyDown={handleKeyDown} onBlur={saveTitle} onClick={(e) => e.stopPropagation()} className="w-full bg-transparent text-[13px] leading-5 text-foreground border-b border-primary/50 outline-none px-0 py-0" maxLength={100} />
        ) : (
          <div className={cn('truncate text-[13px] leading-5 flex items-center gap-1.5', active ? 'text-foreground' : 'text-foreground/80')}>
            {showPinIcon && <Pin size={11} className="flex-shrink-0 text-primary/60" />}
            <span className="truncate">{conversation.title}</span>
          </div>
        )}
      </div>
      <div className={cn('flex items-center gap-0.5 flex-shrink-0 transition-all duration-100 overflow-hidden', hovered && !editing ? 'opacity-100' : 'opacity-0 w-0 pointer-events-none')}>
        <Tooltip><TooltipTrigger asChild><button onClick={(e) => { e.stopPropagation(); onTogglePin(conversation.id) }} className="p-1 rounded-md text-foreground/30 hover:bg-foreground/[0.08] hover:text-foreground/60 transition-colors">{isPinned ? <PinOff size={13} /> : <Pin size={13} />}</button></TooltipTrigger><TooltipContent side="top">{isPinned ? '取消置顶' : '置顶对话'}</TooltipContent></Tooltip>
        <Tooltip><TooltipTrigger asChild><button onClick={(e) => { e.stopPropagation(); startEdit() }} className="p-1 rounded-md text-foreground/30 hover:bg-foreground/[0.08] hover:text-foreground/60 transition-colors"><Pencil size={13} /></button></TooltipTrigger><TooltipContent side="top">重命名</TooltipContent></Tooltip>
        <Tooltip><TooltipTrigger asChild><button onClick={(e) => { e.stopPropagation(); onToggleArchive(conversation.id) }} className="p-1 rounded-md text-foreground/30 hover:bg-foreground/[0.08] hover:text-foreground/60 transition-colors">{conversation.archived ? <ArchiveRestore size={13} /> : <Archive size={13} />}</button></TooltipTrigger><TooltipContent side="top">{conversation.archived ? '取消归档' : '归档'}</TooltipContent></Tooltip>
        <Tooltip><TooltipTrigger asChild><button onClick={(e) => { e.stopPropagation(); onRequestDelete() }} className="p-1 rounded-md text-foreground/30 hover:bg-destructive/10 hover:text-destructive transition-colors"><Trash2 size={13} /></button></TooltipTrigger><TooltipContent side="top">删除对话</TooltipContent></Tooltip>
      </div>
    </div>
  )
}

// ===== Agent 会话列表项 =====
type SessionLeftAccent = 'orange' | 'blue' | 'green'
const SESSION_LEFT_ACCENT_CLASS: Record<SessionLeftAccent, string> = { orange: 'bg-orange-500', blue: 'bg-blue-500', green: 'bg-green-500' }

interface AgentSessionItemProps {
  session: AgentSessionMeta
  active: boolean
  hovered: boolean
  indicatorStatus: SessionIndicatorStatus
  showPinIcon?: boolean
  isInWorkingSection?: boolean
  leftAccent?: SessionLeftAccent
  workspaceName?: string
  onSelect: () => void
  onRequestDelete: () => void
  onRequestMove: () => void
  onRename: (id: string, newTitle: string) => Promise<void>
  onTogglePin: (id: string) => Promise<void>
  onToggleManualWorking: (id: string) => Promise<void>
  onToggleArchive: (id: string) => Promise<void>
  onMouseEnter: () => void
  onMouseLeave: () => void
}

function AgentSessionItem({ session, active, hovered, indicatorStatus, showPinIcon, isInWorkingSection, leftAccent, workspaceName, onSelect, onRequestDelete, onRequestMove, onRename, onTogglePin, onToggleManualWorking, onToggleArchive, onMouseEnter, onMouseLeave }: AgentSessionItemProps): React.ReactElement {
  const [editing, setEditing] = React.useState(false)
  const [editTitle, setEditTitle] = React.useState('')
  const inputRef = React.useRef<HTMLInputElement>(null)
  const justStartedEditing = React.useRef(false)

  const startEdit = (): void => {
    setEditTitle(session.title); setEditing(true); justStartedEditing.current = true
    setTimeout(() => { justStartedEditing.current = false; inputRef.current?.focus(); inputRef.current?.select() }, 300)
  }
  const saveTitle = async (): Promise<void> => {
    if (justStartedEditing.current) return
    const trimmed = editTitle.trim()
    if (!trimmed || trimmed === session.title) { setEditing(false); return }
    await onRename(session.id, trimmed); setEditing(false)
  }
  const handleKeyDown = (e: React.KeyboardEvent): void => { if (e.key === 'Enter') { e.preventDefault(); saveTitle() } else if (e.key === 'Escape') setEditing(false) }

  return (
    <div role="button" tabIndex={0} onClick={onSelect} onDoubleClick={(e) => { e.stopPropagation(); startEdit() }} onMouseEnter={onMouseEnter} onMouseLeave={onMouseLeave}
      className={cn('relative w-full flex items-center gap-2 px-3 py-[7px] rounded-[10px] transition-colors duration-100 titlebar-no-drag text-left', active ? 'session-item-selected bg-primary/10 shadow-[0_1px_2px_0_rgba(0,0,0,0.05)]' : 'hover:bg-primary/5')}>
      {leftAccent && <span className={cn('absolute left-1 top-1.5 bottom-1.5 w-[2px] rounded-full pointer-events-none', SESSION_LEFT_ACCENT_CLASS[leftAccent])} />}
      <div className="flex-1 min-w-0">
        {editing ? (
          <input ref={inputRef} value={editTitle} onChange={(e) => setEditTitle(e.target.value)} onKeyDown={handleKeyDown} onBlur={saveTitle} onClick={(e) => e.stopPropagation()} className="w-full bg-transparent text-[13px] leading-5 text-foreground border-b border-primary/50 outline-none px-0 py-0" maxLength={100} />
        ) : (
          <div className={cn('truncate text-[13px] leading-5 flex items-center gap-1.5', active ? 'text-foreground' : 'text-foreground/80')}>
            {showPinIcon && <Pin size={11} className="flex-shrink-0 text-primary/60" />}
            <span className="flex-shrink-0 inline-flex items-center justify-center text-primary" style={{ width: '18px' }}>
              {session.titlePending ? (
                <LoaderCircle size={14} strokeWidth={2} className="animate-spin" />
              ) : session.titleEmoji ? (
                <span className="text-[14px] leading-none" style={{ fontFamily: "'Noto Emoji', sans-serif" }}>{session.titleEmoji}</span>
              ) : null}
            </span>
            {session.titlePending ? (
              <span className="flex-1 h-3 rounded bg-foreground/10 animate-pulse" />
            ) : (
              <span className="truncate">{session.title}</span>
            )}
            {workspaceName && !session.titlePending && (
              <span className="flex-shrink-0 px-1.5 py-0 rounded-full bg-foreground/[0.06] text-[10px] leading-4 text-foreground/40 font-medium truncate max-w-[80px]">{workspaceName}</span>
            )}
          </div>
        )}
      </div>
      {/* Always-visible running indicator — pulsing primary dot when this
          session has an active agent loop. Lets the user spot in-flight
          tasks even when looking at a different session's tab. */}
      {indicatorStatus === 'running' && !editing && (
        <span
          className={cn(
            'flex-shrink-0 size-2 rounded-full bg-primary',
            'animate-pulse shadow-[0_0_8px_hsl(var(--primary))]',
            // hide when the action icons appear on hover, to avoid double-rendering
            hovered && 'opacity-0 transition-opacity',
          )}
          title="任务执行中"
        />
      )}
      <div className={cn('flex items-center gap-0.5 flex-shrink-0 transition-all duration-100 overflow-hidden', hovered && !editing ? 'opacity-100' : 'opacity-0 w-0 pointer-events-none')}>
        <Tooltip><TooltipTrigger asChild><button onClick={(e) => { e.stopPropagation(); onTogglePin(session.id) }} className="p-1 rounded-md text-foreground/30 hover:bg-foreground/[0.08] hover:text-foreground/60 transition-colors">{session.pinned ? <PinOff size={13} /> : <Pin size={13} />}</button></TooltipTrigger><TooltipContent side="top">{session.pinned ? '取消置顶' : '置顶会话'}</TooltipContent></Tooltip>
        <Tooltip><TooltipTrigger asChild><button onClick={(e) => { e.stopPropagation(); if (indicatorStatus !== 'running') onToggleManualWorking(session.id) }} disabled={indicatorStatus === 'running'} className={cn('p-1 rounded-md transition-colors', indicatorStatus === 'running' ? 'text-primary/40 cursor-not-allowed' : (isInWorkingSection || session.manualWorking) ? 'text-primary hover:bg-foreground/[0.08]' : 'text-foreground/30 hover:bg-foreground/[0.08] hover:text-foreground/60')}><Hammer size={13} className={(isInWorkingSection || session.manualWorking) ? 'fill-current' : ''} /></button></TooltipTrigger><TooltipContent side="top">{indicatorStatus === 'running' ? '运行中无法移出' : (isInWorkingSection || session.manualWorking) ? '取消工作中' : '标记为工作中'}</TooltipContent></Tooltip>
        {(indicatorStatus === 'idle' || indicatorStatus === 'completed') && (
          <Tooltip><TooltipTrigger asChild><button onClick={(e) => { e.stopPropagation(); onRequestMove() }} className="p-1 rounded-md text-foreground/30 hover:bg-foreground/[0.08] hover:text-foreground/60 transition-colors"><ArrowRightLeft size={13} /></button></TooltipTrigger><TooltipContent side="top">迁移到其他工作区</TooltipContent></Tooltip>
        )}
        <Tooltip><TooltipTrigger asChild><button onClick={(e) => { e.stopPropagation(); startEdit() }} className="p-1 rounded-md text-foreground/30 hover:bg-foreground/[0.08] hover:text-foreground/60 transition-colors"><Pencil size={13} /></button></TooltipTrigger><TooltipContent side="top">重命名</TooltipContent></Tooltip>
        <Tooltip><TooltipTrigger asChild><button onClick={(e) => { e.stopPropagation(); onToggleArchive(session.id) }} className="p-1 rounded-md text-foreground/30 hover:bg-foreground/[0.08] hover:text-foreground/60 transition-colors">{session.archived ? <ArchiveRestore size={13} /> : <Archive size={13} />}</button></TooltipTrigger><TooltipContent side="top">{session.archived ? '取消归档' : '归档'}</TooltipContent></Tooltip>
        <Tooltip><TooltipTrigger asChild><button onClick={(e) => { e.stopPropagation(); onRequestDelete() }} className="p-1 rounded-md text-foreground/30 hover:bg-destructive/10 hover:text-destructive transition-colors"><Trash2 size={13} /></button></TooltipTrigger><TooltipContent side="top">删除会话</TooltipContent></Tooltip>
      </div>
    </div>
  )
}
