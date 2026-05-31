/**
 * useAskUserBanner — all the side-effect + state-machine wiring behind the
 * AskUserBanner (extracted in the features/agent migration so the banner shell
 * stays a thin presentational component ≤ ~300 lines).
 *
 * Owns: the per-question answer map, the active tab, keyboard navigation
 * (↑↓ to move/select, Enter to advance/submit), the auto-advance timer for
 * single-select, the submit + dismiss flows, and the reset effects that fire on
 * a new request / tab change. IPC (`respondAskUser`, `stopAgent`) routes through
 * the agent bridge — no `@tauri-apps/api` here.
 */

import * as React from 'react'
import { useAtom, useSetAtom } from 'jotai'
import {
  allPendingAskUserRequestsAtom,
  agentStreamingStatesAtom,
  finalizeStreamingActivities,
} from '@/atoms/agent-atoms'
import type { AskUserQuestion } from '@/lib/agent-types'
import { stopAgent, respondAskUser } from '@/lib/bridge/agent'

export interface QuestionAnswer {
  selected: string[]
  customText: string
  showCustom: boolean
}

export const EMPTY_ANSWER: QuestionAnswer = { selected: [], customText: '', showCustom: false }

export interface UseAskUserBanner {
  /** The first pending request for this session (FIFO), or null. */
  request: { requestId: string; sessionId: string; questions: AskUserQuestion[] } | null
  /** All pending requests for this session — used for the "+N" indicator. */
  requestCount: number
  questions: AskUserQuestion[]
  activeTab: number
  setActiveTab: React.Dispatch<React.SetStateAction<number>>
  focusedOptIdx: number
  submitting: boolean
  isLastTab: boolean
  hasValidAnswers: boolean
  getAnswer: (idx: number) => QuestionAnswer
  toggleOption: (qIdx: number, q: AskUserQuestion, label: string) => void
  toggleCustom: (qIdx: number) => void
  setCustomText: (qIdx: number, text: string) => void
  /** Schedule a single-select auto-advance to the next tab (150ms). */
  scheduleAutoAdvance: () => void
  handleSubmit: () => Promise<void>
  handleDismiss: () => void
  goNextTab: () => void
}

export function useAskUserBanner(sessionId: string): UseAskUserBanner {
  const [allRequests, setAllRequests] = useAtom(allPendingAskUserRequestsAtom)
  const setStreamingStates = useSetAtom(agentStreamingStatesAtom)
  const requests = allRequests.get(sessionId) ?? []
  const [answers, setAnswers] = React.useState<Map<number, QuestionAnswer>>(new Map())
  const [submitting, setSubmitting] = React.useState(false)
  const [activeTab, setActiveTab] = React.useState(0)
  const [focusedOptIdx, setFocusedOptIdx] = React.useState(-1)

  const request = requests[0] ?? null
  const questions = request?.questions ?? []
  const isLastTab = activeTab >= questions.length - 1

  // ===== Refs：确保 keydown handler 始终读取最新值，消除闭包过期问题 =====
  const activeTabRef = React.useRef(activeTab)
  activeTabRef.current = activeTab
  const questionsRef = React.useRef(questions)
  questionsRef.current = questions
  const focusedOptIdxRef = React.useRef(focusedOptIdx)
  focusedOptIdxRef.current = focusedOptIdx
  const submitRef = React.useRef<(() => void) | null>(null)
  const autoAdvanceTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null)

  const clearAutoAdvanceTimer = React.useCallback((): void => {
    if (autoAdvanceTimerRef.current != null) {
      clearTimeout(autoAdvanceTimerRef.current)
      autoAdvanceTimerRef.current = null
    }
  }, [])

  // 组件卸载时清理未触发的跳转定时器
  React.useEffect(() => clearAutoAdvanceTimer, [clearAutoAdvanceTimer])

  React.useEffect(() => {
    clearAutoAdvanceTimer()
    setActiveTab(0)
    setFocusedOptIdx(-1)
    const firstOpt = questions[0]?.options[0]
    setAnswers(firstOpt
      ? new Map([[0, { ...EMPTY_ANSWER, selected: [firstOpt.label] }]])
      : new Map())
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request?.requestId])

  // 切换 Tab 时重置焦点并默认选中第一个选项
  React.useEffect(() => {
    setFocusedOptIdx(-1)
    setAnswers((prev) => {
      if (prev.has(activeTab)) return prev
      const firstOpt = questions[activeTab]?.options[0]
      if (!firstOpt) return prev
      const map = new Map(prev)
      map.set(activeTab, { ...EMPTY_ANSWER, selected: [firstOpt.label] })
      return map
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTab])

  const getAnswer = React.useCallback(
    (idx: number): QuestionAnswer => answers.get(idx) ?? EMPTY_ANSWER,
    [answers],
  )

  const toggleOption = React.useCallback((qIdx: number, q: AskUserQuestion, label: string): void => {
    setAnswers((prev) => {
      const map = new Map(prev)
      const cur = map.get(qIdx) ?? EMPTY_ANSWER
      const selected = q.multiSelect
        ? (cur.selected.includes(label) ? cur.selected.filter((s) => s !== label) : [...cur.selected, label])
        : [label]
      map.set(qIdx, { ...cur, selected, showCustom: false, customText: '' })
      return map
    })
  }, [])

  const toggleCustom = React.useCallback((qIdx: number): void => {
    setAnswers((prev) => {
      const map = new Map(prev)
      const cur = map.get(qIdx) ?? EMPTY_ANSWER
      map.set(qIdx, { ...cur, showCustom: !cur.showCustom, selected: cur.showCustom ? cur.selected : [] })
      return map
    })
  }, [])

  const setCustomText = React.useCallback((qIdx: number, text: string): void => {
    setAnswers((prev) => {
      const map = new Map(prev)
      const cur = map.get(qIdx) ?? EMPTY_ANSWER
      map.set(qIdx, { ...cur, customText: text })
      return map
    })
  }, [])

  // 键盘导航：只在 requestId 变化时重建 handler，内部通过 ref 读取最新值
  React.useEffect(() => {
    if (!request || questions.length === 0) return

    const handleKeyDown = (e: KeyboardEvent): void => {
      const curTab = activeTabRef.current
      const qs = questionsRef.current
      const curFocusIdx = focusedOptIdxRef.current
      const q = qs[curTab]
      if (!q) return
      const itemCount = q.options.length + 1
      const lastTab = curTab >= qs.length - 1

      // 自由文本输入框内：仅 Enter 生效（输入法组合中跳过）
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
        if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
          e.preventDefault()
          if (lastTab) submitRef.current?.()
          else setActiveTab((prev) => prev + 1)
        }
        return
      }

      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault()
        const nextIdx = curFocusIdx === -1
          ? (e.key === 'ArrowDown' ? 0 : itemCount - 1)
          : e.key === 'ArrowDown'
            ? (curFocusIdx + 1) % itemCount
            : (curFocusIdx - 1 + itemCount) % itemCount
        setFocusedOptIdx(nextIdx)
        // 移动焦点同时选中
        if (nextIdx < q.options.length) {
          const opt = q.options[nextIdx]
          if (opt) toggleOption(curTab, q, opt.label)
        } else {
          toggleCustom(curTab)
        }
      } else if (e.key === 'Enter' && !e.isComposing) {
        e.preventDefault()
        if (lastTab) submitRef.current?.()
        else setActiveTab((prev) => prev + 1)
      }
    }

    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request?.requestId])

  /** 关闭问题 & 终止 Agent */
  const handleDismiss = React.useCallback((): void => {
    // 立即标记 streaming 停止，避免 UI 残留
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
    // 清除当前 session 所有待处理的 AskUser 请求
    setAllRequests((prev) => {
      const map = new Map(prev)
      map.delete(sessionId)
      return map
    })
    // 终止 Agent
    stopAgent(sessionId).catch(console.error)
  }, [sessionId, setAllRequests, setStreamingStates])

  const handleSubmit = React.useCallback(async (): Promise<void> => {
    if (submitting || !request) return
    setSubmitting(true)
    try {
      const answersRecord: Record<string, string> = {}
      for (let i = 0; i < questions.length; i++) {
        const q = questions[i]
        if (!q) continue
        const answer = answers.get(i) ?? EMPTY_ANSWER
        const key = q.question || String(i)
        if (answer.showCustom && answer.customText.trim()) {
          answersRecord[key] = answer.customText.trim()
        } else if (answer.selected.length > 0) {
          answersRecord[key] = answer.selected.join(', ')
        }
      }
      await respondAskUser({ requestId: request.requestId, answers: answersRecord })
      setAllRequests((prev) => {
        const map = new Map(prev)
        const current = map.get(sessionId) ?? []
        const newValue = current.filter((r) => r.requestId !== request.requestId)
        if (newValue.length === 0) map.delete(sessionId)
        else map.set(sessionId, newValue)
        return map
      })
    } catch (error) {
      console.error('[AskUserBanner] 响应失败:', error)
    } finally {
      setSubmitting(false)
    }
  }, [submitting, request, questions, answers, sessionId, setAllRequests])

  // Keep the keydown handler's submit ref pointed at the latest handleSubmit.
  submitRef.current = handleSubmit

  const scheduleAutoAdvance = React.useCallback((): void => {
    clearAutoAdvanceTimer()
    autoAdvanceTimerRef.current = setTimeout(() => {
      autoAdvanceTimerRef.current = null
      setActiveTab((prev) => prev + 1)
    }, 150)
  }, [clearAutoAdvanceTimer])

  const goNextTab = React.useCallback((): void => {
    if (!isLastTab) setActiveTab((prev) => prev + 1)
  }, [isLastTab])

  const hasValidAnswers = questions.some((_, idx) => {
    const a = answers.get(idx) ?? EMPTY_ANSWER
    return a.selected.length > 0 || (a.showCustom && a.customText.trim().length > 0)
  })

  return {
    request,
    requestCount: requests.length,
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
  }
}
