/**
 * AskUserBanner — Agent AskUserQuestion 交互式问答横幅
 *
 * 多问题用顶部 Tab 切换，选项竖向排列。
 * 键盘：↑↓ 选择选项，Enter 确认当前问题（最后一题提交，否则翻页）。
 *
 * Thin shell (features/agent migration): the answer state machine + keyboard /
 * submit / dismiss wiring lives in `useAskUserBanner`; each question renders via
 * `AskUserQuestionCard`. This file is presentation-only — IPC goes through the
 * hook (and thus the agent bridge).
 */

import * as React from 'react'
import { Send, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { useAskUserBanner } from '../hooks/useAskUserBanner'
import { AskUserQuestionCard } from './AskUserQuestionCard'

/** AskUserBanner 属性接口 */
interface AskUserBannerProps {
  sessionId: string
}

export function AskUserBanner({ sessionId }: AskUserBannerProps): React.ReactElement | null {
  const {
    request,
    requestCount,
    questions,
    activeTab,
    setActiveTab,
    focusedOptIdx,
    submitting,
    isLastTab,
    hasValidAnswers,
    getAnswer,
    toggleOption,
    toggleCustom,
    setCustomText,
    scheduleAutoAdvance,
    handleSubmit,
    handleDismiss,
    goNextTab,
  } = useAskUserBanner(sessionId)

  if (!request) return null

  const currentQuestion = questions[activeTab]
  if (!currentQuestion) return null

  return (
    <div
      className="mx-4 mb-3 rounded-xl bg-card shadow-lg overflow-hidden animate-in slide-in-from-bottom-2 duration-200"
      role="alertdialog"
      aria-modal="true"
      aria-label="Agent 在问"
    >
      {/* 头部 + Tab 栏 */}
      <div className="px-4 pt-3 pb-2">
        <div className="flex items-center justify-between mb-2">
          <span className="text-sm font-medium text-foreground">Agent 需要你的输入</span>
          <div className="flex items-center gap-1.5">
            {requestCount > 1 && (
              <span className="text-xs text-muted-foreground">(+{requestCount - 1})</span>
            )}
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
        </div>

        {/* Tab 栏（多问题时显示） */}
        {questions.length > 1 && (
          <div className="flex gap-1">
            {questions.map((q, idx) => {
              const isActive = idx === activeTab
              const hasAnswer = getAnswer(idx).selected.length > 0
                || (getAnswer(idx).showCustom && getAnswer(idx).customText.trim().length > 0)
              return (
                <button
                  key={idx}
                  type="button"
                  className={`
                    px-2.5 py-1 rounded-lg text-xs font-medium transition-all outline-none
                    ${isActive
                      ? 'bg-primary text-primary-foreground shadow-sm'
                      : hasAnswer
                        ? 'bg-primary/15 text-primary'
                        : 'bg-muted/60 text-muted-foreground hover:bg-muted hover:text-foreground'
                    }
                  `}
                  onClick={() => setActiveTab(idx)}
                >
                  {`${idx + 1}-${q.multiSelect ? '多选' : '单选'}：${q.header || `问题 ${idx + 1}`}`}
                </button>
              )
            })}
          </div>
        )}
      </div>

      {/* 当前问题内容 */}
      <div className="px-4 pb-2">
        <AskUserQuestionCard
          question={currentQuestion}
          questionIndex={activeTab}
          answer={getAnswer(activeTab)}
          focusedIndex={focusedOptIdx}
          showBadge={questions.length === 1}
          onToggleOption={(label) => {
            toggleOption(activeTab, currentQuestion, label)
            if (!currentQuestion.multiSelect && !isLastTab) {
              scheduleAutoAdvance()
            }
          }}
          onToggleCustom={() => toggleCustom(activeTab)}
          onCustomTextChange={(text) => setCustomText(activeTab, text)}
          onSubmit={isLastTab ? handleSubmit : goNextTab}
        />
      </div>

      {/* 底部 */}
      <div className="flex items-center justify-end gap-1.5 px-4 pb-3">
        <span className="text-[10px] text-muted-foreground/40 mr-auto">
          ↑↓ 选择 · Enter {isLastTab ? '确认' : '下一个'}
        </span>
        {isLastTab && (
          <Button
            variant="default"
            size="sm"
            onClick={handleSubmit}
            disabled={submitting || !hasValidAnswers}
            className="h-7 px-3 text-xs"
          >
            <Send className="size-3 mr-1" />
            确认
          </Button>
        )}
      </div>
    </div>
  )
}
