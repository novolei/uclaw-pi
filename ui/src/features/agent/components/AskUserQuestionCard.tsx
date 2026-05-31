/**
 * AskUserQuestionCard — a single AskUser question rendered as a vertical
 * option list (extracted from AskUserBanner during the features/agent
 * migration to keep the banner shell thin). Presentation only: selection /
 * custom-text / submit all arrive as callback props from the banner shell;
 * the answer state machine lives in useAskUserBanner.
 */

import * as React from 'react'
import Markdown, { defaultUrlTransform } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import type { AskUserQuestion } from '@/lib/agent-types'
import type { QuestionAnswer } from '../hooks/useAskUserBanner'

const PREVIEW_REMARK_PLUGINS = [remarkGfm]

function safeUrlTransform(url: string): string {
  if (url.startsWith('http://') || url.startsWith('https://')) return url
  return defaultUrlTransform(url)
}

export interface AskUserQuestionCardProps {
  question: AskUserQuestion
  questionIndex: number
  answer: QuestionAnswer
  focusedIndex: number
  showBadge: boolean
  onToggleOption: (label: string) => void
  onToggleCustom: () => void
  onCustomTextChange: (text: string) => void
  onSubmit: () => void
}

/** 单个问题卡片(竖向选项) */
export function AskUserQuestionCard({
  question,
  questionIndex,
  answer,
  focusedIndex,
  showBadge,
  onToggleOption,
  onToggleCustom,
  onCustomTextChange,
  onSubmit,
}: AskUserQuestionCardProps): React.ReactElement {
  const optionCount = question.options.length
  const previewOption = focusedIndex >= 0 && focusedIndex < optionCount
    ? question.options[focusedIndex]
    : question.options.find((o) => answer.selected.includes(o.label))
  const previewContent = previewOption?.preview

  return (
    <div className="space-y-2">
      {/* 问题标签 + 文本(分行显示) */}
      <div className="space-y-1">
        {showBadge && (
          <span className="shrink-0 inline-flex items-center px-2.5 py-1 rounded-lg text-xs font-medium bg-primary text-primary-foreground shadow-sm">
            {`${questionIndex + 1}-${question.multiSelect ? '多选' : '单选'}${question.header ? `：${question.header}` : ''}`}
          </span>
        )}
        <p className="text-sm text-foreground">{question.question}</p>
      </div>

      {/* 竖向选项 */}
      <div className="flex flex-col gap-1">
        {question.options.map((option, idx) => {
          const isSelected = answer.selected.includes(option.label)
          const isFocused = focusedIndex === idx
          return (
            <button
              key={option.label}
              type="button"
              className={`
                flex items-center gap-2 px-3 py-2 rounded-lg text-xs transition-all outline-none text-left
                ${isSelected
                  ? 'bg-primary text-primary-foreground shadow-sm'
                  : 'bg-muted/50 text-foreground/80 hover:bg-muted'
                }
                ${isFocused ? 'ring-2 ring-primary/50 ring-offset-1 ring-offset-card' : ''}
              `}
              onClick={() => onToggleOption(option.label)}
            >
              <span className={`text-[10px] shrink-0 ${isSelected ? 'text-primary-foreground/60' : 'text-muted-foreground/50'}`}>
                {idx + 1}
              </span>
              <span className="font-medium">{option.label}</span>
              {option.description && (
                <span className={`text-[11px] ${isSelected ? 'text-primary-foreground/70' : 'text-muted-foreground'}`}>
                  {option.description}
                </span>
              )}
            </button>
          )
        })}

        {/* "其他" */}
        <button
          type="button"
          className={`
            flex items-center gap-2 px-3 py-2 rounded-lg text-xs transition-all outline-none text-left
            ${answer.showCustom
              ? 'bg-primary text-primary-foreground shadow-sm'
              : 'bg-muted/50 text-foreground/80 hover:bg-muted'
            }
            ${focusedIndex === optionCount ? 'ring-2 ring-primary/50 ring-offset-1 ring-offset-card' : ''}
          `}
          onClick={onToggleCustom}
        >
          <span className={`text-[10px] shrink-0 ${answer.showCustom ? 'text-primary-foreground/60' : 'text-muted-foreground/50'}`}>
            {optionCount + 1}
          </span>
          <span className="font-medium">其他...</span>
        </button>
      </div>

      {/* 自由文本输入 */}
      {answer.showCustom && (
        <input
          type="text"
          className="w-full px-3 py-2 rounded-lg text-xs bg-muted/40 focus:bg-muted/60 focus:outline-none focus:ring-2 focus:ring-primary/30 placeholder:text-muted-foreground/40 transition-colors"
          placeholder="输入自定义答案..."
          value={answer.customText}
          onChange={(e) => onCustomTextChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault()
              e.stopPropagation() // 阻止冒泡到 document handler，避免重复触发 setActiveTab
              onSubmit()
            }
          }}
          autoFocus
        />
      )}

      {/* 选项 Preview(聚焦或选中时展示) */}
      {previewContent && (
        <div className="mt-2 rounded-lg bg-muted/40 p-3 text-xs prose prose-sm dark:prose-invert max-w-none prose-p:my-0 prose-headings:my-0.5 prose-li:my-0 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0">
          <Markdown remarkPlugins={PREVIEW_REMARK_PLUGINS} urlTransform={safeUrlTransform}>
            {previewContent}
          </Markdown>
        </div>
      )}
    </div>
  )
}
