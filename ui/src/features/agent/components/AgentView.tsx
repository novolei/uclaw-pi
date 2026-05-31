/**
 * AgentView — Agent 模式主视图容器
 *
 * 职责：
 * - 加载当前 Agent 会话消息
 * - 发送/停止/压缩 Agent 消息
 * - 附件上传处理
 * - AgentHeader 支持标题编辑 + 文件浏览器切换
 *
 * 注意：IPC 流式事件监听已提升到全局 useGlobalAgentListeners，
 * 本组件为纯展示 + 交互组件。
 *
 * 布局：AgentHeader | AgentMessages | AgentComposer + 可选 FileBrowser 侧面板
 *
 * features/agent migration split (was 1926 lines — the single biggest file in
 * the app). This shell is now layout/composition ONLY: it calls
 * hooks/useAgentViewState (which owns every atom read, derivation, callback, and
 * the four sub-hooks — useAgentSession / useAgentActions / useAgentComposer /
 * useAgentQueue) and renders the same layout tree as before. The composer card
 * lives in components/agent-input/AgentComposer (+ AgentThinkingPopover); the
 * critical paste/drop/submit/attachment user path lives VERBATIM in
 * hooks/useAgentComposer. Behavior is unchanged — same JSX, same conditions,
 * same wiring. All IPC routes through the agent bridge.
 */

import * as React from 'react'
import { Map as MapIcon, AlertTriangle } from 'lucide-react'
import { Button } from '@/components/ui/button'
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
import { SttModal } from '@/components/stt/SttModal'
import { FirstRunDialog } from '@/components/stt/FirstRunDialog'
import { AgentSessionProvider } from '@/contexts/session-context'

import { AgentMessages } from './AgentMessages'
import { AgentHeader } from './AgentHeader'
import { BrowserPreviewOverlay } from './BrowserPreviewOverlay'
import { PermissionBanner } from './PermissionBanner'
import { AgentHeartbeatBanner } from './AgentHeartbeatBanner'
import { AgentStatusBar } from './AgentStatusBar'
import { AskUserBanner } from './AskUserBanner'
import { ExitPlanModeBanner } from './ExitPlanModeBanner'
import { PlanModeSuggestBanner } from './PlanModeSuggestBanner'
import { AutomationRunBanner } from './AutomationRunBanner'
import { AgentComposer } from './agent-input/AgentComposer'

import { useAgentViewState } from '../hooks/useAgentViewState'

export function AgentView({ sessionId }: { sessionId: string }): React.ReactElement {
  // [FLASH-DEBUG] 渲染计数器
  const renderCountRef = React.useRef(0)
  renderCountRef.current++
  if (renderCountRef.current % 50 === 0) {
    console.log(`[FLASH-DEBUG] AgentView(${sessionId.slice(0, 8)}) render #${renderCountRef.current}`)
  }

  const vm = useAgentViewState(sessionId)
  const { agentError } = vm

  return (
    <>
    <AgentSessionProvider sessionId={sessionId}>
      {/* 主内容区域 */}
      <div className="flex flex-col h-full flex-1 min-w-0 relative">
        {/* Agent Header */}
        <AgentHeader sessionId={sessionId} />

        {/* Automation run banner — shows when the session was started by an
            automation trigger (origin starts with "automation:"). */}
        <AutomationRunBanner
          metadataJson={vm.sessions.find((s) => s.id === sessionId)?.metadataJson}
        />

        {/* 消息区域 */}
        <AgentMessages
          sessionId={sessionId}
          /* sessionModelId 回退链：per-session map → 全局默认 → localStorage 活跃模型。
             历史消息 (M2-J 之前) 没在 DB 存 model，必须靠这里兜底显示头像 + caption。 */
          sessionModelId={vm.sessionModelId}
          messages={vm.messages}
          messagesLoaded={vm.messagesLoaded}
          streaming={vm.streaming}
          streamState={vm.streamState}
          liveMessages={vm.liveMessages}
          sessionPath={vm.sessionPath}
          attachedDirs={vm.attachedDirs}
          stoppedByUser={vm.stoppedByUser}
          onRetry={vm.handleRetry}
          onRetryInNewSession={vm.handleRetryInNewSession}
          onFork={vm.handleFork}
          onRewind={vm.handleRewindRequest}
          onCompact={vm.handleCompact}
        />

        {/* Browser preview overlay — positioned absolute within the relative outer container
            so it floats over the scroll area without scrolling with content */}
        <BrowserPreviewOverlay sessionId={sessionId} />

        {/* outer_timeout 内联错误块：显示超时提示 + 重试按钮 */}
        {agentError && agentError.kind === 'outer_timeout' && (
          <div className="mx-4 mb-2 rounded-md border border-destructive/40 bg-destructive/[0.04] p-3 animate-in fade-in slide-in-from-bottom-1 duration-200">
            <div className="flex items-start gap-2">
              <AlertTriangle className="size-4 text-destructive shrink-0 mt-0.5" />
              <div className="flex-1 text-sm text-foreground/85">
                <div>{agentError.message}</div>
                {agentError.timeoutSecs != null && (
                  <div className="mt-1 text-xs text-muted-foreground">
                    提示：可在 设置 → 高级 中调整 Agent 循环超时（当前 {agentError.timeoutSecs}s）。
                  </div>
                )}
              </div>
              <Button variant="outline" size="sm" onClick={vm.handleRetry}>
                重试
              </Button>
            </div>
          </div>
        )}

        {/* 权限请求横幅 */}
        <PermissionBanner sessionId={sessionId} />

        {/* Bundle 27-A — Heartbeat / stall / interrupted-reply recovery.
            Self-contained: renders nothing when there's no live run,
            no stall, and no recovery payload pending. Owns its own
            event listeners (agent:heartbeat / agent:stalled /
            agent:stall-recovered / agent:interrupted-recovered). */}
        <AgentHeartbeatBanner sessionId={sessionId} />

        {/* Plan 模式自动建议横幅 — advisory (not blocking) */}
        <PlanModeSuggestBanner sessionId={sessionId} />

        {/* AskUserQuestion 交互式问答横幅 */}
        <AskUserBanner sessionId={sessionId} />

        {/* Plan 模式指示条 */}
        {vm.isPlanMode && (
          <div className="mx-4 mb-2 flex items-center gap-2 px-3 py-2 rounded-lg bg-primary/5 text-primary text-sm animate-in fade-in slide-in-from-bottom-1 duration-200">
            <MapIcon className="size-4 animate-pulse" />
            <span className="font-medium">Agent 正在规划中...</span>
            <span className="text-xs text-muted-foreground">完成后将请求你的审批</span>
          </div>
        )}

        {/* ExitPlanMode 计划审批横幅 */}
        <ExitPlanModeBanner sessionId={sessionId} />

        {/* 任务执行状态条 — sticky 在输入栏正上方，agent 跑任务时常驻可见。
            默认关闭，可在 设置 → 外观 中开启。 */}
        {vm.agentStatusBarEnabled && <AgentStatusBar sessionId={sessionId} />}

        {/* 输入区域 — 交互横幅显示时隐藏，由横幅替代 */}
        {!vm.hasBannerOverlay && (
          <AgentComposer
            sessionId={sessionId}
            inputContent={vm.inputContent}
            inputHtmlContent={vm.inputHtmlContent}
            onComposerChange={vm.handleComposerChange}
            onComposerFocus={vm.handleComposerFocus}
            onComposerBlur={vm.handleComposerBlur}
            onHtmlChange={vm.setInputHtmlContent}
            setInputContent={vm.setInputContent}
            activeProviderModel={vm.activeProviderModel}
            sendWithCmdEnter={vm.sendWithCmdEnter}
            isPlanMode={vm.isPlanMode}
            streaming={vm.streaming}
            canSend={vm.canSend}
            hasTextInput={vm.hasTextInput}
            sessionPath={vm.sessionPath}
            workspaceSlug={vm.workspaceSlug}
            allAttachedDirs={vm.allAttachedDirs}
            pendingFiles={vm.pendingFiles}
            suggestion={vm.suggestion}
            onRemoveFile={vm.handleRemoveFile}
            onDismissSuggestion={vm.handleDismissSuggestion}
            composerEditorRef={vm.composerEditorRef}
            mentionControllerRef={vm.mentionControllerRef}
            agentThinking={vm.agentThinking}
            onToggleThinking={vm.handleToggleThinking}
            contextStatus={vm.contextStatus}
            currentQueue={vm.currentQueue}
            onSteerQueued={vm.handleSteerQueued}
            onEditQueued={vm.handleEditQueued}
            onDeleteQueued={vm.handleDeleteQueued}
            onSubmit={vm.handleSend}
            onPasteFiles={vm.handlePasteFiles}
            onPasteLongText={vm.handlePasteLongText}
            onDragOver={vm.handleDragOver}
            onDragLeave={vm.handleDragLeave}
            onDrop={vm.handleDrop}
            isDragOver={vm.isDragOver}
            onOpenFileDialog={vm.handleOpenFileDialog}
            onStop={vm.handleStop}
            onCompact={vm.handleCompact}
            onShowSttDownloadDialog={() => vm.setFirstRunOpen(true)}
          />
        )}
      </div>
    </AgentSessionProvider>

    {/* 回退确认弹窗 */}
    <AlertDialog
      open={vm.rewindTargetUuid !== null}
      onOpenChange={(v) => { if (!v) vm.setRewindTargetUuid(null) }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>确认回退</AlertDialogTitle>
          <AlertDialogDescription>
            回退将截断该消息之后的所有对话，并恢复文件到该时刻的状态。此操作不可撤销，确定要回退吗？
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>取消</AlertDialogCancel>
          <AlertDialogAction
            onClick={vm.handleRewindConfirm}
            className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
          >
            回退
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
    <SttModal composer="agent" onSegmentFinalized={vm.handleSegmentFinalized} />
    <FirstRunDialog
      open={vm.firstRunOpen}
      onOpenChange={vm.setFirstRunOpen}
      onReady={() => { window.dispatchEvent(new CustomEvent('uclaw:stt-start-after-ready')) }}
    />
    </>
  )
}
