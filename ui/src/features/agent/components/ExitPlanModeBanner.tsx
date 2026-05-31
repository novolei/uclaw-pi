/**
 * ExitPlanModeBanner — Agent ExitPlanMode 计划审批横幅
 *
 * Backend `exit_plan_mode` 工具触发 `agent:exit_plan_request` IPC 事件。
 * 本组件渲染计划 Markdown + 3 选 1 决策 UI：
 *
 *   1. 接受 + 切到 Auto 执行   → decision='accept_and_auto'
 *   2. 接受 + 留 plan          → decision='accept_keep_plan'
 *      （仅当 agent 在 exit_plan_mode 中声明了 allowed_prompts 时显示）
 *   3. 拒绝并反馈             → decision='reject', feedback=<textarea>
 *
 * 键盘：Enter 在反馈框内提交 reject；Escape 关闭反馈框。
 */

import * as React from 'react'
import { useAtom, useSetAtom } from 'jotai'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import {
  Check,
  ShieldCheck,
  X,
  MessageSquare,
  Send,
  FileText,
} from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  allPendingExitPlanRequestsAtom,
  agentStreamingStatesAtom,
  finalizeStreamingActivities,
} from '@/atoms/agent-atoms'
import { respondExitPlanMode, stopAgent } from '@/lib/bridge/agent'

const REMARK_PLUGINS = [remarkGfm]

interface ExitPlanModeBannerProps {
  sessionId: string
}

export function ExitPlanModeBanner({ sessionId }: ExitPlanModeBannerProps): React.ReactElement | null {
  const [allRequests, setAllRequests] = useAtom(allPendingExitPlanRequestsAtom)
  const setStreamingStates = useSetAtom(agentStreamingStatesAtom)
  const requests = allRequests.get(sessionId) ?? []
  const request = requests[0] ?? null

  const [showFeedback, setShowFeedback] = React.useState(false)
  const [feedbackText, setFeedbackText] = React.useState('')
  const [submitting, setSubmitting] = React.useState(false)

  // 重置状态：每次新请求出现时清空
  React.useEffect(() => {
    setShowFeedback(false)
    setFeedbackText('')
  }, [request?.requestId])

  const removeFromQueue = React.useCallback((requestId: string) => {
    setAllRequests((prev) => {
      const map = new Map(prev)
      const cur = map.get(sessionId) ?? []
      const next = cur.filter((r) => r.requestId !== requestId)
      if (next.length === 0) map.delete(sessionId)
      else map.set(sessionId, next)
      return map
    })
  }, [sessionId, setAllRequests])

  const handleAccept = async (autoMode: boolean): Promise<void> => {
    if (submitting || !request) return
    setSubmitting(true)
    try {
      await respondExitPlanMode({
        requestId: request.requestId,
        sessionId: request.sessionId,
        decision: autoMode ? 'accept_and_auto' : 'accept_keep_plan',
        allowedPrompts: request.allowedPrompts ?? [],
      })
      removeFromQueue(request.requestId)
    } catch (e) {
      console.error('[ExitPlanModeBanner] accept failed:', e)
    } finally {
      setSubmitting(false)
    }
  }

  const handleReject = async (): Promise<void> => {
    if (submitting || !request) return
    const feedback = feedbackText.trim()
    if (!feedback) return
    setSubmitting(true)
    try {
      await respondExitPlanMode({
        requestId: request.requestId,
        sessionId: request.sessionId,
        decision: 'reject',
        feedback,
        allowedPrompts: [],
      })
      removeFromQueue(request.requestId)
    } catch (e) {
      console.error('[ExitPlanModeBanner] reject failed:', e)
    } finally {
      setSubmitting(false)
    }
  }

  /** 关闭计划审批 & 终止 Agent */
  const handleDismiss = (): void => {
    if (!request) return
    setStreamingStates((prev) => {
      const current = prev.get(sessionId)
      if (!current || !current.running) return prev
      const map = new Map(prev)
      map.set(sessionId, {
        ...current,
        running: false,
        ...finalizeStreamingActivities(current.toolActivities, current.teammates),
      })
      return map
    })
    setAllRequests((prev) => {
      const map = new Map(prev)
      map.delete(sessionId)
      return map
    })
    stopAgent(sessionId).catch(console.error)
  }

  if (!request) return null

  const allowed = request.allowedPrompts ?? []
  const canKeepPlan = allowed.length > 0

  return (
    <div
      className="fixed bottom-4 left-1/2 -translate-x-1/2 z-40 w-[min(720px,calc(100vw-2rem))] rounded-xl bg-card shadow-2xl border border-border/60 overflow-hidden animate-in slide-in-from-bottom-2 duration-200"
      role="alertdialog"
      aria-modal="true"
      aria-label="Agent 计划待审批"
    >
      {/* 头部 */}
      <div className="px-4 pt-3 pb-2 border-b border-border/40">
        <div className="flex items-center gap-2">
          <FileText className="size-4 text-primary" />
          <span className="text-sm font-medium text-foreground flex-1">Agent 计划待审批</span>
          <button
            type="button"
            className="size-5 flex items-center justify-center rounded-md text-muted-foreground/50 hover:text-foreground hover:bg-muted/60 transition-colors"
            onClick={handleDismiss}
            title="关闭并终止 Agent"
            aria-label="关闭"
          >
            <X className="size-3.5" />
          </button>
        </div>
        <p className="text-[11px] text-muted-foreground mt-1">
          Agent 已制定计划，请选择如何继续
        </p>
      </div>

      {/* 计划 Markdown */}
      <div className="px-4 py-3 max-h-[40vh] overflow-y-auto">
        <div className="prose prose-sm dark:prose-invert max-w-none prose-headings:mt-2 prose-headings:mb-1 prose-p:my-1 prose-ul:my-1 prose-li:my-0">
          <Markdown remarkPlugins={REMARK_PLUGINS}>
            {request.plan}
          </Markdown>
        </div>
      </div>

      {/* allowedPrompts 展示 */}
      {allowed.length > 0 && (
        <div className="px-4 pb-2">
          <p className="text-[11px] text-muted-foreground mb-1">计划声明的允许操作：</p>
          <div className="flex flex-wrap gap-1">
            {allowed.map((p, idx) => (
              <span
                key={idx}
                className="inline-flex items-center px-2 py-0.5 rounded-full text-[10px] bg-primary/10 text-primary/80"
              >
                {p}
              </span>
            ))}
          </div>
        </div>
      )}

      {/* 决策按钮 */}
      {!showFeedback && (
        <div className="px-4 pb-3 pt-1 flex flex-wrap gap-2">
          <Button
            variant="default"
            size="sm"
            onClick={() => void handleAccept(true)}
            disabled={submitting}
            className="h-8 text-xs"
          >
            <Check className="size-3.5 mr-1" />
            接受 + 切到 Auto 执行
          </Button>

          {canKeepPlan && (
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void handleAccept(false)}
              disabled={submitting}
              className="h-8 text-xs"
              title="保持当前权限模式，将 allowed_prompts 写入会话规则"
            >
              <ShieldCheck className="size-3.5 mr-1" />
              接受 + 留 plan
            </Button>
          )}

          <Button
            variant="ghost"
            size="sm"
            onClick={() => setShowFeedback(true)}
            disabled={submitting}
            className="h-8 text-xs ml-auto text-destructive hover:text-destructive hover:bg-destructive/10"
          >
            <MessageSquare className="size-3.5 mr-1" />
            拒绝并反馈
          </Button>
        </div>
      )}

      {/* 反馈输入框（拒绝） */}
      {showFeedback && (
        <div className="px-4 pb-3 pt-1">
          <p className="text-[11px] text-muted-foreground mb-1.5">告诉 Agent 需要调整什么：</p>
          <div className="flex gap-2">
            <textarea
              className="flex-1 px-3 py-2 rounded-lg text-xs bg-muted/40 focus:bg-muted/60 focus:outline-none focus:ring-2 focus:ring-primary/30 placeholder:text-muted-foreground/40 transition-colors resize-none min-h-[64px]"
              placeholder="例如：先读一下 X 文件再做计划 / 跳过第 3 步 / ..."
              value={feedbackText}
              onChange={(e) => setFeedbackText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
                  e.preventDefault()
                  e.stopPropagation()
                  if (feedbackText.trim()) void handleReject()
                } else if (e.key === 'Escape') {
                  e.preventDefault()
                  setShowFeedback(false)
                  setFeedbackText('')
                }
              }}
              autoFocus
              disabled={submitting}
            />
            <div className="flex flex-col gap-1 shrink-0">
              <Button
                variant="default"
                size="sm"
                onClick={() => void handleReject()}
                disabled={submitting || !feedbackText.trim()}
                className="h-8 px-3 text-xs"
              >
                <Send className="size-3 mr-1" />
                发送
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => { setShowFeedback(false); setFeedbackText('') }}
                disabled={submitting}
                className="h-8 px-3 text-xs"
              >
                取消
              </Button>
            </div>
          </div>
          <p className="text-[10px] text-muted-foreground/60 mt-1">
            Enter 发送 · Shift+Enter 换行 · Esc 取消
          </p>
        </div>
      )}
    </div>
  )
}
