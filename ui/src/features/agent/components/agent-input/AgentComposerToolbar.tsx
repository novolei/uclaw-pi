/**
 * AgentComposerToolbar — the agent composer's footer toolbar (model / permission
 * / thinking / attach / context-usage / strategy / auto-preview / git / speech
 * selectors on the left, the send/stop button on the right), extracted VERBATIM
 * from AgentComposer during the features/agent migration split so the composer
 * shell stays ≤300 lines. Pure presentation — every callback arrives as a prop;
 * the JSX, classNames, and conditions are unchanged.
 */

import * as React from 'react'
import { CornerDownLeft, Square, Paperclip } from 'lucide-react'
import { ProviderModelSelector } from '@/components/chat/ProviderModelSelector'
import { SpeechButton } from '@/components/ai-elements/speech-button'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import { getActiveAccelerator, getAcceleratorDisplay } from '@/lib/shortcut-registry'
import { GitChipsRow } from '@/components/chat/git/GitChipsRow'
import type { AgentContextStatus } from '@/atoms/agent-atoms'
import { PermissionModeSelector } from '../PermissionModeSelector'
import { StrategyPresetSelector } from '../StrategyPresetSelector'
import { ContextUsageBadge } from '../ContextUsageBadge'
import { AutoPreviewPopover } from '../AutoPreviewPopover'
import { AgentThinkingPopover } from './AgentThinkingPopover'

export interface AgentComposerToolbarProps {
  sessionId: string
  streaming: boolean
  canSend: boolean
  hasTextInput: boolean
  agentThinking: import('@/lib/proma-types').ThinkingConfig | undefined
  onToggleThinking: () => void
  contextStatus: AgentContextStatus
  onOpenFileDialog: () => void
  onSubmit: () => void
  onStop: () => void
  onCompact: () => void
  onShowSttDownloadDialog: () => void
}

export function AgentComposerToolbar(props: AgentComposerToolbarProps): React.ReactElement {
  const {
    sessionId,
    streaming,
    canSend,
    hasTextInput,
    agentThinking,
    onToggleThinking,
    contextStatus,
    onOpenFileDialog,
    onSubmit,
    onStop,
    onCompact,
    onShowSttDownloadDialog,
  } = props

  return (
    <div className="flex items-center justify-between px-2 py-1 h-[48px] gap-4">
      <div className="flex items-center gap-1.5 flex-1 min-w-0">
        <ProviderModelSelector />
        <PermissionModeSelector sessionId={sessionId} />
        {/* 思考模式切换 + 展开偏好 */}
        <AgentThinkingPopover
          agentThinking={agentThinking}
          onToggle={onToggleThinking}
        />
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="size-[36px] rounded-full text-foreground/60 hover:text-foreground"
              onClick={onOpenFileDialog}
            >
              <Paperclip className="size-5" />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="top">
            <p>添加附件</p>
          </TooltipContent>
        </Tooltip>
        <ContextUsageBadge
          inputTokens={contextStatus.inputTokens}
          outputTokens={contextStatus.outputTokens}
          cacheReadTokens={contextStatus.cacheReadTokens}
          cacheCreationTokens={contextStatus.cacheCreationTokens}
          costUsd={contextStatus.costUsd}
          contextWindow={contextStatus.contextWindow}
          skillsTokens={contextStatus.skillsTokens}
          isCompacting={contextStatus.isCompacting}
          isProcessing={streaming}
          onCompact={onCompact}
        />
        <StrategyPresetSelector sessionId={sessionId} />
        <AutoPreviewPopover />
        {/* <FeishuNotifyToggle sessionId={sessionId} /> */}

        <GitChipsRow />
        <SpeechButton
          composer="agent"
          onShowDownloadDialog={onShowSttDownloadDialog}
        />
      </div>

      <div className="flex items-center gap-1.5">
        {streaming && !hasTextInput ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="size-[36px] rounded-full text-destructive hover:!text-[hsl(0,75%,55%)] hover:!bg-[var(--stop-hover-bg)]"
                onClick={onStop}
              >
                <Square className="size-[16px]" fill="currentColor" strokeWidth={0} />
              </Button>
            </TooltipTrigger>
            <TooltipContent side="top">
              <p>停止 Agent ({getAcceleratorDisplay(getActiveAccelerator('stop-generation'))})</p>
            </TooltipContent>
          </Tooltip>
        ) : (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className={cn(
              'size-[36px] rounded-full',
              canSend
                ? 'text-primary hover:bg-primary/10'
                : 'text-foreground/30 cursor-not-allowed'
            )}
            onClick={onSubmit}
            disabled={!canSend}
          >
            <CornerDownLeft className="size-[22px]" />
          </Button>
        )}
      </div>
    </div>
  )
}
