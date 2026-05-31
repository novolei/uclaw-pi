/**
 * AgentComposer — the agent-mode input bar (the composer the user drives every
 * session), extracted VERBATIM from AgentView during the features/agent
 * migration split. This is the presentational shell: the queued-messages banner,
 * the drag-drop card, the attachment preview, the suggestion chip, the
 * RichTextInput + mention controller, and the footer toolbar (model / permission
 * / thinking / attach / context / strategy / auto-preview / git / speech
 * selectors + the send/stop button).
 *
 * All behavior (paste / drop / submit / attachment) lives in
 * hooks/useAgentComposer and the action hooks; this component only wires the
 * callbacks + state it receives as props into the same JSX as before. The exact
 * markup, classNames, and conditions are unchanged.
 */

import * as React from 'react'
import type { Editor } from '@tiptap/core'
import { Settings, X, Sparkles } from 'lucide-react'
import { AttachmentPreviewItem } from '@/components/chat/AttachmentPreviewItem'
import { RichTextInput } from '@/components/ai-elements/rich-text-input'
import {
  ComposerMentionController,
  type ComposerMentionControllerHandle,
} from '@/components/composer/ComposerMentionController'
import { cn } from '@/lib/utils'
import type { AgentPendingFile } from '@/lib/agent-types'
import type { ActiveProviderModel } from '@/atoms/active-model'
import type { QueuedAgentMessage } from '@/atoms/agent-queue-messages'
import type { AgentContextStatus } from '@/atoms/agent-atoms'
import { PlanModeDashedBorder } from '../PlanModeDashedBorder'
import { PetWidget } from '../PetWidget'
import { QueuedMessagesBanner } from '../QueuedMessagesBanner'
import { AgentComposerToolbar } from './AgentComposerToolbar'

export interface AgentComposerProps {
  sessionId: string
  // composer value + state
  inputContent: string
  inputHtmlContent: string
  onComposerChange: (v: string) => void
  onComposerFocus: () => void
  onComposerBlur: () => void
  onHtmlChange: (html: string) => void
  setInputContent: (v: string) => void
  // model / mode
  activeProviderModel: ActiveProviderModel | null
  sendWithCmdEnter: boolean
  isPlanMode: boolean
  streaming: boolean
  canSend: boolean
  hasTextInput: boolean
  // paths
  sessionPath: string | null
  workspaceSlug: string | null
  allAttachedDirs: string[]
  // attachments + suggestion
  pendingFiles: AgentPendingFile[]
  suggestion: string | null
  onRemoveFile: (id: string) => void
  onDismissSuggestion: () => void
  // refs
  composerEditorRef: React.MutableRefObject<Editor | null>
  mentionControllerRef: React.MutableRefObject<ComposerMentionControllerHandle | null>
  // thinking toggle
  agentThinking: import('@/lib/proma-types').ThinkingConfig | undefined
  onToggleThinking: () => void
  // context badge
  contextStatus: AgentContextStatus
  // queue
  currentQueue: QueuedAgentMessage[]
  onSteerQueued: (msg: QueuedAgentMessage) => void
  onEditQueued: (msg: QueuedAgentMessage) => void
  onDeleteQueued: (msg: QueuedAgentMessage) => void
  // composer handlers
  onSubmit: () => void
  onPasteFiles: (files: File[]) => void
  onPasteLongText: (text: string) => void
  onDragOver: (e: React.DragEvent) => void
  onDragLeave: (e: React.DragEvent) => void
  onDrop: (e: React.DragEvent) => void
  isDragOver: boolean
  onOpenFileDialog: () => void
  onStop: () => void
  onCompact: () => void
  onShowSttDownloadDialog: () => void
}

export function AgentComposer(props: AgentComposerProps): React.ReactElement {
  const {
    sessionId,
    inputContent,
    inputHtmlContent,
    onComposerChange,
    onComposerFocus,
    onComposerBlur,
    onHtmlChange,
    setInputContent,
    activeProviderModel,
    sendWithCmdEnter,
    isPlanMode,
    streaming,
    canSend,
    hasTextInput,
    sessionPath,
    workspaceSlug,
    allAttachedDirs,
    pendingFiles,
    suggestion,
    onRemoveFile,
    onDismissSuggestion,
    composerEditorRef,
    mentionControllerRef,
    agentThinking,
    onToggleThinking,
    contextStatus,
    currentQueue,
    onSteerQueued,
    onEditQueued,
    onDeleteQueued,
    onSubmit,
    onPasteFiles,
    onPasteLongText,
    onDragOver,
    onDragLeave,
    onDrop,
    isDragOver,
    onOpenFileDialog,
    onStop,
    onCompact,
    onShowSttDownloadDialog,
  } = props

  return (
    <div className="px-2.5 pb-2.5 md:px-[18px] md:pb-[18px]" data-input-mode="agent">
      {/* Bundle 2-A — Codex / Claude App style queued-messages banner.
          Sits as a SIBLING above the composer card (not inside it),
          so the queue visually stacks with the input like in the
          Claude app and codex CLI. The component itself returns null
          when no messages are queued so this branch has zero cost on
          the regular hot path. */}
      <QueuedMessagesBanner
        messages={currentQueue}
        onSteer={onSteerQueued}
        onEdit={onEditQueued}
        onDelete={onDeleteQueued}
      />

      <div
        className={cn(
          'relative rounded-[17px] border-[0.5px] border-border bg-background/70 backdrop-blur-sm transition-all duration-200',
          isPlanMode && !isDragOver && 'plan-mode-border',
          isDragOver && 'border-[2px] border-dashed border-[#2ecc71] bg-[#2ecc71]/[0.03]'
        )}
        onDragOver={onDragOver}
        onDragLeave={onDragLeave}
        onDrop={onDrop}
      >
        {/* Pet anchored to the entire composer card's top — sits above all
            inner banners (model warning, attachment preview, agent suggestion,
            sticky user message, etc.). bottom:100% references this card's top. */}
        <PetWidget />
        {isPlanMode && !isDragOver && <PlanModeDashedBorder />}
        {/* 未配置模型提示 */}
        {!activeProviderModel && (
          <div className="flex items-center gap-2 px-4 py-2 text-sm text-amber-600 dark:text-amber-400">
            <Settings size={14} />
            <span>请在下方工具栏选择模型</span>
          </div>
        )}

        {/* 附件预览区域 */}
        {pendingFiles.length > 0 && (
          <div className="flex flex-wrap gap-2 px-3 pt-2.5 pb-1.5">
            {pendingFiles.map((file) => (
              <AttachmentPreviewItem
                key={file.id}
                filename={file.filename}
                mediaType={file.mediaType}
                previewUrl={file.previewUrl}
                onRemove={() => onRemoveFile(file.id)}
              />
            ))}
          </div>
        )}

        {/* Agent 建议提示 */}
        {suggestion && !streaming && (
          <div className="px-3 pt-2.5 pb-1.5">
            <button
              type="button"
              className="group flex items-start gap-2 w-full rounded-lg border border-dashed border-primary/30 bg-primary/[0.03] px-3 py-2.5 text-left text-sm transition-colors hover:border-primary/50 hover:bg-primary/[0.06]"
              onClick={onSubmit}
            >
              <Sparkles className="size-4 shrink-0 mt-0.5 text-primary/60 group-hover:text-primary/80" />
              <span className="flex-1 min-w-0 text-foreground/80 group-hover:text-foreground line-clamp-3">{suggestion}</span>
              <X
                className="size-3.5 shrink-0 mt-0.5 text-muted-foreground/40 hover:text-foreground transition-colors"
                onClick={(e) => {
                  e.stopPropagation()
                  onDismissSuggestion()
                }}
              />
            </button>
          </div>
        )}

        <div className="relative">
          <RichTextInput
            value={inputContent}
            onChange={onComposerChange}
            onFocus={onComposerFocus}
            onBlur={onComposerBlur}
            onSubmit={onSubmit}
            onPasteFiles={onPasteFiles}
            onPasteLongText={onPasteLongText}
            placeholder={
              activeProviderModel
                ? sendWithCmdEnter
                  ? '输入消息... (⌘/Ctrl+Enter 发送，Enter 换行，@ 引用文件，/ 调用 Skill，# 调用 MCP)'
                  : '输入消息... (Enter 发送，Shift+Enter 换行，@ 引用文件，/ 调用 Skill，# 调用 MCP)'
                : '请先在下方工具栏选择模型...'
            }
            disabled={!activeProviderModel}
            autoFocusTrigger={sessionId}
            collapsible
            workspacePath={sessionPath}
            workspaceSlug={workspaceSlug}
            attachedDirs={allAttachedDirs}
            htmlValue={inputHtmlContent}
            onHtmlChange={onHtmlChange}
            sendWithCmdEnter={sendWithCmdEnter}
            editorRef={composerEditorRef}
            onKeyDownIntercept={(e) =>
              mentionControllerRef.current?.handleKeyDown(e) ?? false}
          />
          <ComposerMentionController
            ref={mentionControllerRef}
            editorRef={composerEditorRef}
            value={inputContent}
            setValue={setInputContent}
            sessionId={sessionId}
            disabled={!activeProviderModel}
          />
        </div>

        {/* Footer 工具栏 */}
        <AgentComposerToolbar
          sessionId={sessionId}
          streaming={streaming}
          canSend={canSend}
          hasTextInput={hasTextInput}
          agentThinking={agentThinking}
          onToggleThinking={onToggleThinking}
          contextStatus={contextStatus}
          onOpenFileDialog={onOpenFileDialog}
          onSubmit={onSubmit}
          onStop={onStop}
          onCompact={onCompact}
          onShowSttDownloadDialog={onShowSttDownloadDialog}
        />
      </div>
    </div>
  )
}
