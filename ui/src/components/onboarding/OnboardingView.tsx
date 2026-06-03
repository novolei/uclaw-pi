/**
 * OnboardingView — 首次使用引导
 *
 * 首次打开应用时的引导流程：
 * 1. 欢迎页 → 2. API Key 配置 → 3. 本地模型(可选) → 4. 主题选择 → 5. 完成
 * 从 Proma 迁移，适配 Tauri。
 *
 * S3: 在 API Key 步骤后插入可选的「本地模型」步骤（LocalModelStep），完成 /
 * 跳过的结果持久化到 localStorage（`ONBOARDING_LOCAL_MODEL_KEY`），与 S2 本地模型
 * atoms 的 localStorage 键约定一致，使再次进入引导时不会重复提示。
 */

import * as React from 'react'
import { ChevronRight, ChevronLeft, Sparkles, Key, Palette, Check } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { LocalModelStep } from './steps/LocalModelStep'

interface OnboardingViewProps {
  onComplete: () => void
}

type OnboardingStep = 'welcome' | 'api-key' | 'local-model' | 'theme' | 'done'

const STEPS: OnboardingStep[] = ['welcome', 'api-key', 'local-model', 'theme', 'done']

/**
 * Persisted outcome of the optional local-model onboarding step. Mirrors the S2
 * `uclaw.localModel.*` localStorage key convention so re-entering onboarding (or
 * the Settings re-entry) can tell whether the user already chose. `null` = not
 * yet prompted.
 */
export const ONBOARDING_LOCAL_MODEL_KEY = 'uclaw.onboarding.localModel'

export function readLocalModelOnboardingFlag(): 'done' | 'skipped' | null {
  try {
    const v = localStorage.getItem(ONBOARDING_LOCAL_MODEL_KEY)
    return v === 'done' || v === 'skipped' ? v : null
  } catch {
    return null
  }
}

function persistLocalModelOnboardingFlag(outcome: 'done' | 'skipped'): void {
  try {
    localStorage.setItem(ONBOARDING_LOCAL_MODEL_KEY, outcome)
  } catch {
    // localStorage unavailable — best-effort only.
  }
}

/** 步骤指示器 */
function StepIndicator({ current, total }: { current: number; total: number }): React.ReactElement {
  return (
    <div className="flex items-center gap-1.5">
      {Array.from({ length: total }).map((_, i) => (
        <div
          key={i}
          className={cn(
            'h-1.5 rounded-full transition-all',
            i === current ? 'w-6 bg-primary' : i < current ? 'w-1.5 bg-primary/40' : 'w-1.5 bg-muted-foreground/20',
          )}
        />
      ))}
    </div>
  )
}

/** 欢迎步骤 */
function WelcomeStep(): React.ReactElement {
  return (
    <div className="flex flex-col items-center text-center gap-6 py-8">
      <div className="size-20 rounded-3xl bg-primary/10 flex items-center justify-center">
        <Sparkles className="size-10 text-primary" />
      </div>
      <div>
        <h2 className="text-2xl font-bold text-foreground mb-2">欢迎使用 uClaw</h2>
        <p className="text-muted-foreground max-w-sm">
          uClaw 是一款本地优先的 AI 助手，帮助你更高效地完成日常任务。
          让我们花一分钟完成基本设置。
        </p>
      </div>
    </div>
  )
}

/** API Key 步骤 */
function ApiKeyStep(): React.ReactElement {
  return (
    <div className="flex flex-col items-center text-center gap-6 py-8">
      <div className="size-16 rounded-2xl bg-amber-500/10 flex items-center justify-center">
        <Key className="size-8 text-amber-500" />
      </div>
      <div>
        <h2 className="text-xl font-bold text-foreground mb-2">配置 AI 模型</h2>
        <p className="text-muted-foreground max-w-sm text-sm">
          你可以稍后在设置中配置 API Key 和模型提供商。
          支持 OpenAI、Anthropic、本地模型等多种选择。
        </p>
      </div>
      <div className="w-full max-w-sm rounded-lg border border-border/60 bg-muted/30 p-4 text-left text-sm text-muted-foreground">
        <p className="flex items-center gap-2">
          <span className="text-primary">💡</span>
          你可以随时在 <kbd className="px-1.5 py-0.5 rounded bg-muted text-[11px] font-mono">设置 → 模型</kbd> 中添加或修改。
        </p>
      </div>
    </div>
  )
}

/** 主题步骤 */
function ThemeStep(): React.ReactElement {
  return (
    <div className="flex flex-col items-center text-center gap-6 py-8">
      <div className="size-16 rounded-2xl bg-violet-500/10 flex items-center justify-center">
        <Palette className="size-8 text-violet-500" />
      </div>
      <div>
        <h2 className="text-xl font-bold text-foreground mb-2">选择你的风格</h2>
        <p className="text-muted-foreground max-w-sm text-sm">
          uClaw 内置了 8 种精心设计的主题，你可以随时在设置中切换。
        </p>
      </div>
      <div className="flex gap-3">
        {['bg-zinc-900', 'bg-slate-800', 'bg-emerald-900', 'bg-violet-900'].map((color, i) => (
          <div
            key={i}
            className={cn('size-10 rounded-lg border-2 border-transparent', color)}
          />
        ))}
      </div>
    </div>
  )
}

/** 完成步骤 */
function DoneStep(): React.ReactElement {
  return (
    <div className="flex flex-col items-center text-center gap-6 py-8">
      <div className="size-16 rounded-full bg-emerald-500/10 flex items-center justify-center">
        <Check className="size-8 text-emerald-500" />
      </div>
      <div>
        <h2 className="text-xl font-bold text-foreground mb-2">一切就绪！</h2>
        <p className="text-muted-foreground max-w-sm text-sm">
          你已经完成了基本设置。现在可以开始使用 uClaw 了。
        </p>
      </div>
    </div>
  )
}

export function OnboardingView({ onComplete }: OnboardingViewProps): React.ReactElement {
  const [stepIndex, setStepIndex] = React.useState(0)
  const currentStep = STEPS[stepIndex]!

  const canGoBack = stepIndex > 0
  const isLast = stepIndex === STEPS.length - 1

  const handleNext = React.useCallback(() => {
    if (isLast) {
      onComplete()
    } else {
      setStepIndex((i) => Math.min(i + 1, STEPS.length - 1))
    }
  }, [isLast, onComplete])

  const handleBack = React.useCallback(() => {
    setStepIndex((i) => Math.max(i - 1, 0))
  }, [])

  // Persist the local-model outcome (done/skipped) when the step settles, then
  // auto-advance to the next step so the user isn't stuck on a terminal step.
  const handleLocalModelSettled = React.useCallback((outcome: 'done' | 'skipped') => {
    persistLocalModelOnboardingFlag(outcome)
    setStepIndex((i) => Math.min(i + 1, STEPS.length - 1))
  }, [])

  return (
    <div className="flex-1 flex flex-col items-center justify-center p-8 bg-background">
      <div className="w-full max-w-lg">
        {/* Step Content */}
        {currentStep === 'welcome' && <WelcomeStep />}
        {currentStep === 'api-key' && <ApiKeyStep />}
        {currentStep === 'local-model' && <LocalModelStep onSettled={handleLocalModelSettled} />}
        {currentStep === 'theme' && <ThemeStep />}
        {currentStep === 'done' && <DoneStep />}

        {/* Navigation */}
        <div className="flex items-center justify-between mt-8">
          <div>
            {canGoBack ? (
              <Button variant="ghost" size="sm" onClick={handleBack}>
                <ChevronLeft className="size-4 mr-1" />
                上一步
              </Button>
            ) : (
              <Button variant="ghost" size="sm" onClick={onComplete} className="text-muted-foreground/60">
                跳过
              </Button>
            )}
          </div>

          <StepIndicator current={stepIndex} total={STEPS.length} />

          <Button size="sm" onClick={handleNext}>
            {isLast ? '开始使用' : '下一步'}
            {!isLast && <ChevronRight className="size-4 ml-1" />}
          </Button>
        </div>
      </div>
    </div>
  )
}
