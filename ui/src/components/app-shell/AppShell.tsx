/**
 * AppShell - 应用主布局容器
 *
 * 布局结构：[LeftSidebar 可折叠] | [MainArea: TabBar + TabContent] | [RightSidePanel 可折叠]
 *
 * MainArea 支持多标签页，Settings 视图为独立覆盖。
 */

import * as React from 'react'
import { useAtomValue, useAtom, useSetAtom } from 'jotai'
import { getDefaultStore } from 'jotai'
import { LeftSidebar } from './LeftSidebar'
import { RightSidePanel } from './RightSidePanel'
import { MainArea } from '@/components/tabs/MainArea'
import { AppShellProvider, type AppShellContextType } from '@/contexts/AppShellContext'
import { ApprovalModal } from '@/components/ApprovalModal'
import { SettingsDialog } from '@/features/settings'
import { ConnectionsPanel } from '@/components/dock/ConnectionsPanel'
import { AlertPanel } from '@/components/dock/AlertPanel'
import { ModeBanner } from '@/components/agent/ModeBanner'
import { appModeAtom } from '@/atoms/app-mode'
import {
  agentSessionsAtom,
  allPendingAskUserRequestsAtom,
  allPendingExitPlanRequestsAtom,
  currentAgentSessionIdAtom,
  currentAgentWorkspaceIdAtom,
  currentSessionSidePanelOpenAtom,
  installAskUserListener,
  installExitPlanListener,
} from '@/atoms/agent-atoms'
import { currentConversationIdAtom } from '@/atoms/chat-atoms'
import { tabsAtom, activeTabIdAtom, openTab } from '@/atoms/tab-atoms'
import { activeWorkspaceIdAtom, selectWorkspaceAtom } from '@/atoms/workspace'
import {
  settingsOpenAtom,
  settingsTabAtom,
  type SettingsTab,
} from '@/atoms/settings-tab'
import { SearchPalette } from '@/components/search/SearchPalette'
import { cn } from '@/lib/utils'
import { installScrollToMessage } from '@/lib/scroll-to-message'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { toast } from 'sonner'
import { attachWorkspaceDirectory, pathIsDirectory, copyFileIntoWorkspace, onBudgetThreshold } from '@/lib/tauri-bridge'
import { workspaceFilesVersionAtom, workspaceAttachedDirsMapAtom } from '@/atoms/agent-atoms'
import { firedBudgetThresholdsAtom } from '@/atoms/cost'
import { TabSessionSyncer } from './TabSessionSyncer'
import { VersionWatermark } from './VersionWatermark'
import { useWorkspaceArrowSwitch } from '@/hooks/useWorkspaceSwipe'
import { WorkspaceTabCleaner } from './WorkspaceTabCleaner'
import { focusModeAtom } from '@/atoms/focus-mode-atoms'
import { useFocusModeShortcut } from '@/hooks/useFocusModeShortcut'
import { FocusModeOverlay } from '@/components/focus-mode/FocusModeOverlay'
import { pendingEscalationsAtom } from '@/atoms/automation'
import { resolveEscalation } from '@/lib/tauri-bridge'
import { useAutomationEvents } from '@/hooks/useAutomationEvents'
import { EscalationModal } from '@/components/automation/EscalationModal'
import { topLevelViewAtom } from '@/atoms/top-level-view'
import { KaleidoscopeShell } from '@/views/Kaleidoscope/KaleidoscopeShell'
import { MemoryVoiceCapture } from '@/components/memory/MemoryVoiceCapture'
import { QuickCaptureDialog } from '@/components/memory/QuickCaptureDialog'
import { BottomDockHoverRegion, type BottomDockHoverRegionHandle } from '@/components/dock/BottomDockHoverRegion'
import { bottomDockEnabledAtom } from '@/atoms/dock-atoms'
import { useDockBounce } from '@/hooks/useDockBounce'
import { useMemuConsolidation } from '@/hooks/useMemuConsolidation'

export interface AppShellProps {
  /** Context 值，用于传递给子组件 */
  contextValue: AppShellContextType
}

export function AppShell({ contextValue }: AppShellProps): React.ReactElement {
  const appMode = useAtomValue(appModeAtom)
  const topLevelView = useAtomValue(topLevelViewAtom)
  const currentSessionId = useAtomValue(currentAgentSessionIdAtom)
  const isPanelOpen = useAtomValue(currentSessionSidePanelOpenAtom)
  const activeWorkspaceId = useAtomValue(activeWorkspaceIdAtom)
  const showRightPanel = appMode === 'agent' && !!currentSessionId
  const focusMode = useAtomValue(focusModeAtom)
  const isDockEnabled = useAtomValue(bottomDockEnabledAtom)
  // Ref wired to BottomDockHoverRegion's imperative handle. Used by Task 5
  // (useDockBounce) to call forceReveal() / holdRevealed() from IPC events.
  const dockHoverRef = React.useRef<BottomDockHoverRegionHandle>(null)
  useDockBounce(dockHoverRef)
  useMemuConsolidation()
  useFocusModeShortcut()  // global Alt+F binding

  // Escalation modal: subscribe to pending escalations and show one at a time.
  // Mounted here (alongside ApprovalModal) so it's always present regardless
  // of which view or workspace is active.
  useAutomationEvents()
  const [escalations, setEscalations] = useAtom(pendingEscalationsAtom)
  const firstEscalation = escalations[0]

  // Shift+ArrowLeft / Shift+ArrowRight to cycle workspaces (Windows /
  // external-keyboard fallback for users without a touchpad).
  // Trackpad swipe is scoped to the LeftSidebar — see useWorkspaceSwipe
  // mount inside LeftSidebar.tsx.
  useWorkspaceArrowSwitch()

  // Tab navigation atoms — used by handleSearchResultSelect
  const [tabs, setTabs] = useAtom(tabsAtom)
  const setActiveTabId = useSetAtom(activeTabIdAtom)
  const setAppMode = useSetAtom(appModeAtom)
  const setCurrentConversationId = useSetAtom(currentConversationIdAtom)
  const setCurrentAgentSessionId = useSetAtom(currentAgentSessionIdAtom)
  const agentSessions = useAtomValue(agentSessionsAtom)
  const setCurrentAgentWorkspaceId = useSetAtom(currentAgentWorkspaceIdAtom)
  const setAllPendingAskUserRequests = useSetAtom(allPendingAskUserRequestsAtom)
  const setAllPendingExitPlanRequests = useSetAtom(allPendingExitPlanRequestsAtom)
  const selectWorkspace = useSetAtom(selectWorkspaceAtom)
  const setSettingsOpen = useSetAtom(settingsOpenAtom)
  const setSettingsTab = useSetAtom(settingsTabAtom)

  React.useEffect(() => {
    const dispose = installScrollToMessage()
    return dispose
  }, [])

  React.useEffect(() => {
    let dispose: (() => void) | undefined
    installAskUserListener((updater) => setAllPendingAskUserRequests(updater)).then((d) => { dispose = d })
    return () => { dispose?.() }
  }, [setAllPendingAskUserRequests])

  React.useEffect(() => {
    let dispose: (() => void) | undefined
    installExitPlanListener((updater) => setAllPendingExitPlanRequests(updater)).then((d) => { dispose = d })
    return () => { dispose?.() }
  }, [setAllPendingExitPlanRequests])

  const [fired, setFired] = useAtom(firedBudgetThresholdsAtom)
  const firedRef = React.useRef(fired)
  React.useEffect(() => { firedRef.current = fired }, [fired])

  React.useEffect(() => {
    let unlisten: (() => void) | undefined
    onBudgetThreshold((payload) => {
      if (firedRef.current.has(payload.threshold)) return
      setFired(new Set([...firedRef.current, payload.threshold]))
      const currentStr = `$${payload.current.toFixed(2)}`
      const budgetStr = `$${payload.budget.toFixed(2)}`
      if (payload.threshold === 80) {
        toast.warning(`本月已使用预算 ${currentStr} / ${budgetStr} (80%)`)
      } else {
        toast.error(`本月已超出预算: ${currentStr} / ${budgetStr}。AI 调用将继续，但请留意。`)
      }
    }).then((fn) => { unlisten = fn })
    return () => { unlisten?.() }
  }, [setFired])

  // Phase 3: single window-level native drag-drop listener at app root.
  // Tabs each mount their own AgentView, but only one listener should
  // process OS drops. Routes folders → attach to active workspace;
  // files → copy bytes into active workspace folder.
  //
  // StrictMode safety: onDragDropEvent returns a Promise; if cleanup
  // runs before the Promise resolves (which happens on the first mount
  // in StrictMode), we'd miss the unlisten handle and end up with two
  // listeners stacked. The `cancelled` flag closes that gap.
  React.useEffect(() => {
    const win = getCurrentWindow()
    let cancelled = false
    let unlisten: (() => void) | undefined
    void win.onDragDropEvent(async (evt) => {
      const payload = evt.payload as { type?: string; paths?: string[] }
      if (payload.type !== 'drop' || !Array.isArray(payload.paths) || payload.paths.length === 0) return

      // Resolve current workspace at drop time (not at register time) so
      // workspace switching mid-session lands files in the right place.
      const ws = getDefaultStore().get(currentAgentWorkspaceIdAtom)
      if (!ws) {
        toast.error('请先选择工作区')
        return
      }

      const folderResults: string[][] = []
      for (const p of payload.paths) {
        try {
          const isDir = await pathIsDirectory(p)
          if (isDir) {
            const updated = await attachWorkspaceDirectory(ws, p)
            folderResults.push(updated)
            toast.success(`已附加目录: ${p}`)
          } else {
            const writtenPath = await copyFileIntoWorkspace(ws, p)
            const basename = writtenPath.split('/').pop() ?? writtenPath
            toast.success(`已上传文件: ${basename}`)
          }
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err)
          toast.error(`处理 ${p} 失败: ${msg}`)
        }
      }
      // Push the most-recent attached_dirs list to the atom so the
      // SidePanel 附加目录 section reflects the change without restart.
      if (folderResults.length > 0) {
        const latest = folderResults[folderResults.length - 1]
        getDefaultStore().set(workspaceAttachedDirsMapAtom, (prev) => {
          const m = new Map(prev)
          m.set(ws, latest)
          return m
        })
      }
      getDefaultStore().set(workspaceFilesVersionAtom, (v) => v + 1)
    }).then((u) => {
      // If the effect already cleaned up while the Promise was pending,
      // immediately remove the listener we just registered. Otherwise
      // keep the handle for the real cleanup.
      if (cancelled) {
        u()
      } else {
        unlisten = u
      }
    })
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [])

  const handleSearchResultSelect = React.useCallback((payload:
    | { kind: 'thread'; thread: { id: string; kind: 'chat' | 'agent'; workspaceId: string } }
    | { kind: 'workspace'; workspace: { id: string; name: string } }
    | { kind: 'settings'; settings: { id: string; settingsTab?: SettingsTab } }
    | { kind: 'search_hit'; hit: { source: string; sourceId: string; messageId?: string } }
  ) => {
    switch (payload.kind) {
      case 'thread': {
        const t = payload.thread
        // Open the right tab type — chat or agent
        const tabType = t.kind === 'agent' ? 'agent' : 'chat'
        // Tag the tab with the thread's own workspaceId — search/recents
        // are cross-workspace, so the user may be viewing workspace A
        // while clicking a thread that lives in B. Without this the tab
        // gets tagged A but visibleTabsAtom for A filters it out (the
        // thread isn't an A session), surfacing as "标签页不存在".
        const ws = t.workspaceId ?? activeWorkspaceId ?? 'default'
        const result = openTab(tabs, { type: tabType, sessionId: t.id, title: '', workspaceId: ws })
        setTabs(result.tabs)
        setActiveTabId(result.activeTabId)
        // Update the per-domain "current" atoms so the view focuses correctly.
        setAppMode(t.kind === 'agent' ? 'agent' : 'chat')
        if (t.kind === 'agent') setCurrentAgentSessionId(t.id)
        else setCurrentConversationId(t.id)
        setCurrentAgentWorkspaceId(t.workspaceId)
        // Same rationale as 'search_hit': flip activeWorkspaceIdAtom when
        // the recent thread lives in a different workspace.
        if (ws !== activeWorkspaceId) {
          void selectWorkspace(ws)
        }
        break
      }
      case 'workspace': {
        // Switch to that workspace; don't open a thread automatically.
        setCurrentAgentWorkspaceId(payload.workspace.id)
        break
      }
      case 'settings': {
        if (payload.settings.settingsTab) {
          setSettingsTab(payload.settings.settingsTab)
        }
        setSettingsOpen(true)
        break
      }
      case 'search_hit': {
        const h = payload.hit
        // existing PR #29 behavior — open the session and scroll to the message
        const tabType = (h.source === 'agent_turn' || h.source === 'agent_message') ? 'agent' : 'chat'
        // Look up workspace from agent sessions (search hits are
        // cross-workspace). Same rationale as the 'thread' case.
        const session = agentSessions.find((s) => s.id === h.sourceId)
        const ws = session?.workspaceId ?? activeWorkspaceId ?? 'default'
        const result = openTab(tabs, { type: tabType, sessionId: h.sourceId, title: '', workspaceId: ws })
        setTabs(result.tabs)
        setActiveTabId(result.activeTabId)
        setAppMode((h.source === 'agent_turn' || h.source === 'agent_message') ? 'agent' : 'chat')
        if ((h.source === 'agent_turn' || h.source === 'agent_message')) setCurrentAgentSessionId(h.sourceId)
        else setCurrentConversationId(h.sourceId)
        if (session?.workspaceId) {
          setCurrentAgentWorkspaceId(session.workspaceId)
        }
        // Phase 6-B: auto-switch the active workspace when the user clicks a
        // cross-workspace hit. The tab is already tagged with `ws` via openTab
        // above (PR #83), but visibleTabsAtom for the *current* active
        // workspace filters it out — we need to flip activeWorkspaceIdAtom
        // too so the new tab actually appears.
        if (ws && ws !== activeWorkspaceId) {
          void selectWorkspace(ws)
        }
        if (h.messageId) {
          setTimeout(() => {
            window.dispatchEvent(new CustomEvent('uclaw:scroll-to-message', {
              detail: { sessionId: h.sourceId, messageId: h.messageId },
            }))
          }, 200)
        }
        break
      }
    }
  }, [
    tabs,
    setTabs,
    setActiveTabId,
    setAppMode,
    setCurrentConversationId,
    setCurrentAgentSessionId,
    agentSessions,
    setCurrentAgentWorkspaceId,
    activeWorkspaceId,
    selectWorkspace,
    setSettingsOpen,
    setSettingsTab,
  ])

  return (
    <AppShellProvider value={contextValue}>
      {/* Effect-only: keeps currentAgentSessionIdAtom / currentConversationIdAtom /
          appModeAtom in sync with the active workspace's active tab on workspace switch. */}
      <TabSessionSyncer />

      {/* Effect-only: drops orphan tabs when a workspace is deleted. */}
      <WorkspaceTabCleaner />

      {/* macOS 全宽标题栏拖拽区（z-50）：覆盖窗口顶部 50px，让面板间隙也可拖动。
          各 panel 以 z-[60] 叠在其上，拖动只在 panel 之外的可见间隙生效。 */}
      <div className="titlebar-drag-region fixed top-0 left-0 right-0 h-[50px] z-50" />

      <div className="shell-bg h-screen w-screen flex overflow-hidden bg-gradient-to-br from-zinc-50 to-zinc-100 dark:from-zinc-950 dark:to-zinc-900">
        {/* 顶层 surface 切换：kaleidoscope 接管整个窗口（不渲染左/右边栏）；
            workspace 恢复标准三栏布局。always-mounted 的全局组件（SearchPalette、
            ApprovalModal 等）留在 switch 外，两个 surface 下均可用。 */}
        {topLevelView === 'kaleidoscope' ? (
          // relative z-[60] so Kaleidoscope content beats the z-50 titlebar
          // drag-region overlay (AppShell.tsx:296) — the workspace panels below
          // use the same pattern; without it the overlay swallows all pointer
          // events in the top ~50px of every Kaleidoscope module.
          // `flex` is required (not just flex-1): KaleidoscopeShell's root is
          // `flex flex-1 min-h-0` and needs a flex parent to actually fill the
          // height — without it the shell sizes to content and leaves a blank
          // strip at the bottom of the window.
          <div className="relative z-[60] flex flex-1 min-w-0 min-h-0">
            <KaleidoscopeShell />
          </div>
        ) : (
          <>
            {/* 左侧边栏：focus mode 下隐藏，由 FocusModeOverlay 接管 */}
            {!focusMode && (
              <div className="sidebar-wrapper p-2 pr-0 relative z-[60]">
                <LeftSidebar />
              </div>
            )}

            {/* 中间容器：只让顶部窄边和 TabBar/header 拖动窗口。
                不把整个 panel 标成 drag-region，否则 WKWebView 会把
                user-select:none / window-drag 语义扩散到聊天内容和浮层按钮。 */}
            <div className="main-panel flex-1 min-w-0 p-2 relative z-[60]">
              {/* 主题背景图层（仅特殊主题如 THE FINALS 使用，其他主题下为空） */}
              <div aria-hidden="true" className="main-panel-bg pointer-events-none absolute inset-0 z-0" />
              <div data-tauri-drag-region aria-hidden="true" className="absolute top-0 left-0 right-0 h-2 z-10 titlebar-drag-region" />
              {/* 主内容区域（ModeBanner + MainArea — 没有 titlebar-no-drag 包裹,
                  让 TabBar 的 drag 区域不被父级 no-drag 吞掉) */}
              <div className="relative z-10 flex flex-col h-full min-h-0 min-w-0">
                <ModeBanner />
                <MainArea />
              </div>
            </div>

            {/* 右侧边栏：Agent 文件面板，带圆角和内边距；focus mode 下同样隐藏 */}
            {!focusMode && showRightPanel && (
              <div className={cn('right-panel-wrapper relative z-[60] transition-[padding] duration-300 ease-in-out', isPanelOpen ? 'p-2 pl-0' : 'p-0')}>
                <RightSidePanel />
              </div>
            )}
          </>
        )}

        {/* Global ⌘K search palette — mounts at root so it works from any view */}
        <SearchPalette onSelect={handleSearchResultSelect} />

        {/* Bottom-right build watermark (version + git commit). pointer-events-none. */}
        <VersionWatermark />

        {/* Focus Mode overlay — null when off, otherwise renders the two
            floating-island re-mounts of LeftSidebar / RightSidePanel + the
            two edge glow indicators. */}
        <FocusModeOverlay />

        {/* Global tool-approval modal — listens for `agent:need_approval`
            IPC events. Must be mounted exactly once at the root, otherwise
            the agent loop's oneshot channel never resolves and the agent
            hangs forever waiting on the user. (This was missing for a long
            time; the bug was hidden because `bash` was in the global
            auto-approve whitelist short-circuiting the resolver.) */}
        <ApprovalModal />

        {/* Settings dialog — mounted at AppShell level (outside the surface
            switch) so it's reachable from BOTH the workspace surface AND the
            Kaleidoscope surface (KaleidoscopeRail has a settings button). */}
        <SettingsDialog />

        {/* Dock placeholder panels — Connections + Alert. Real views are
            scheduled later; these dialogs keep the dock icons functional
            today and live alongside SettingsDialog so they're reachable
            regardless of the current surface. */}
        <ConnectionsPanel />
        <AlertPanel />

        {/* Global escalation modal — shows when a humane spec requests a
            human decision. Dequeues one at a time; resolves via
            resolve_escalation Tauri command then removes from local queue. */}
        {firstEscalation && (
          <EscalationModal
            escalation={firstEscalation}
            onResolve={async (choiceId) => {
              try {
                await resolveEscalation(firstEscalation.id, choiceId)
                setEscalations((rest) => rest.filter((e) => e.id !== firstEscalation.id))
              } catch (err) {
                console.error('[EscalationModal] resolve_escalation failed:', err)
              }
            }}
          />
        )}

        {/* Global voice memory capture — triggered by Cmd+Shift+M via
            uclaw:memory-voice-start event. Renders as right-bottom floating
            panel with pink pulse ring, independent of STT modal. */}
        <MemoryVoiceCapture />

        {/* Global text quick-capture dialog — triggered by Cmd+Shift+.
            via quickCaptureOpenAtom. Complementary to MemoryVoiceCapture. */}
        <QuickCaptureDialog />

        {/* macOS Dock 风格底部导航栏 — 触底滑出，离开后自动收回 */}
        {isDockEnabled && <BottomDockHoverRegion ref={dockHoverRef} />}
      </div>
    </AppShellProvider>
  )
}
