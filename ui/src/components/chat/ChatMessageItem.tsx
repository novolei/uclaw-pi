/**
 * ChatMessageItem - 单条消息渲染
 *
 * 使用 ai-elements 原语组合渲染消息。
 * 支持复制、删除、重新发送、原地编辑操作，并排模式。
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { AlertCircle, Pencil, RotateCcw, Trash2 } from 'lucide-react'
import {
  Message,
  MessageHeader,
  MessageContent,
  MessageActions,
  MessageAction,
  MessageResponse,
  UserMessageContent,
  MessageStopped,
  StreamingIndicator,
  MessageAttachments,
} from '@/components/ai-elements/message'
import {
  Reasoning,
  ReasoningTrigger,
  ReasoningContent,
} from '@/components/ai-elements/reasoning'
import { CopyButton } from './CopyButton'
import { MigrateToAgentButton } from './MigrateToAgentButton'
import { DeleteMessageDialog } from './DeleteMessageDialog'
import { InlineEditForm } from './InlineEditForm'
import { UserAvatar } from './UserAvatar'
import { resolveModelDisplayName } from '@/lib/model-logo'
import { ProviderAvatar } from '@/components/ai-elements/provider-avatar'
import { userProfileAtom } from '@/atoms/user-profile'
import { channelsAtom } from '@/atoms/chat-atoms'
import { agentDisplayNameForAtom } from '@/atoms/agent-display-name'
import type { ChatMessage } from '@/lib/chat-types'
import type { InlineEditSubmitPayload } from './InlineEditForm'
import { ChatToolActivityIndicator } from './ChatToolActivityIndicator'
import { NativeBlockRenderer } from '@/shared/tool-rendering'

// 重导出供外部使用
export type { InlineEditSubmitPayload } from './InlineEditForm'

/**
 * 格式化消息时间（简略写法）
 */
export function formatMessageTime(timestamp: number): string {
  const date = new Date(timestamp)
  const now = new Date()

  const hh = date.getHours().toString().padStart(2, '0')
  const mm = date.getMinutes().toString().padStart(2, '0')
  const month = (date.getMonth() + 1).toString().padStart(2, '0')
  const day = date.getDate().toString().padStart(2, '0')
  const time = `${hh}:${mm}`

  if (date.getFullYear() === now.getFullYear()) {
    return `${month}/${day} ${time}`
  }

  return `${date.getFullYear()}/${month}/${day} ${time}`
}

interface ChatMessageItemProps {
  message: ChatMessage
  conversationId?: string
  isStreaming?: boolean
  isLastAssistant?: boolean
  allMessages?: ChatMessage[]
  messageIndex?: number
  onDeleteMessage?: (messageId: string) => Promise<void>
  onResendMessage?: (message: ChatMessage) => Promise<void>
  onStartInlineEdit?: (message: ChatMessage) => void
  onSubmitInlineEdit?: (message: ChatMessage, payload: InlineEditSubmitPayload) => Promise<void>
  onCancelInlineEdit?: () => void
  isInlineEditing?: boolean
  isParallelMode?: boolean
}

export const ChatMessageItem = React.memo(function ChatMessageItem({
  message,
  conversationId,
  isStreaming = false,
  isLastAssistant = false,
  onDeleteMessage,
  onResendMessage,
  onStartInlineEdit,
  onSubmitInlineEdit,
  onCancelInlineEdit,
  isInlineEditing = false,
  isParallelMode = false,
}: ChatMessageItemProps): React.ReactElement {
  const [deleteDialogOpen, setDeleteDialogOpen] = React.useState(false)
  const [isDeleting, setIsDeleting] = React.useState(false)
  const userProfile = useAtomValue(userProfileAtom)
  const agentNameLookup = useAtomValue(agentDisplayNameForAtom)
  const channels = useAtomValue(channelsAtom)

  const handleDeleteConfirm = async (): Promise<void> => {
    if (!onDeleteMessage) return
    setIsDeleting(true)
    try {
      await onDeleteMessage(message.id)
    } finally {
      setIsDeleting(false)
      setDeleteDialogOpen(false)
    }
  }

  const handleInlineEditSubmit = React.useCallback((payload: InlineEditSubmitPayload): void => {
    if (!onSubmitInlineEdit) return
    void onSubmitInlineEdit(message, payload)
  }, [message, onSubmitInlineEdit])

  const messageFrom = (isParallelMode ? 'assistant' : message.role) as 'user' | 'assistant'

  return (
    <>
      <Message from={messageFrom}>
        {message.role === 'assistant' && (
          <MessageHeader
            name={agentNameLookup(conversationId)}
            model={message.model ? resolveModelDisplayName(message.model, channels) : undefined}
            time={formatMessageTime(message.createdAt)}
            logo={<ProviderAvatar model={message.model} />}
          />
        )}

        {message.role === 'user' && (
          <div className="flex items-start gap-2.5 mb-2.5">
            <UserAvatar avatar={userProfile.avatar} size={35} />
            <div className="flex flex-col justify-between h-[35px]">
              <span className="text-sm font-semibold text-foreground/60 leading-none">{userProfile.userName}</span>
              <span className="text-[10px] text-foreground/[0.38] leading-none">{formatMessageTime(message.createdAt)}</span>
            </div>
          </div>
        )}

        <MessageContent className={isInlineEditing ? 'w-full' : undefined}>
          {message.role === 'assistant' ? (
            message.contentBlocks && message.contentBlocks.length > 0 ? (
              <NativeBlockRenderer
                blocks={message.contentBlocks}
                conversationId={conversationId}
              />
            ) : (
            <>
              {message.toolActivities && message.toolActivities.length > 0 && (
                <ChatToolActivityIndicator activities={message.toolActivities} />
              )}

              {message.reasoning && (
                <Reasoning
                  isStreaming={isStreaming && !message.content}
                  defaultOpen={isStreaming && !message.content}
                >
                  <ReasoningTrigger />
                  <ReasoningContent>{message.reasoning}</ReasoningContent>
                </Reasoning>
              )}

              {message.content ? (
                <>
                  <MessageResponse>{message.content}</MessageResponse>
                  {isStreaming && isLastAssistant && !message.stopped && (
                    <StreamingIndicator />
                  )}
                </>
              ) : message.error ? (
                null
              ) : message.stopped ? (
                <MessageStopped />
              ) : null}

              {message.error && (
                <div className="mt-1 px-3 py-2 rounded-md bg-destructive/10 text-destructive text-sm flex items-center gap-2">
                  <AlertCircle className="size-4 shrink-0" />
                  <span className="break-all">{message.error}</span>
                </div>
              )}

              {message.attachments && message.attachments.length > 0 && (
                <MessageAttachments attachments={message.attachments} />
              )}
            </>
            )
          ) : (
            <>
              {!isInlineEditing && message.attachments && message.attachments.length > 0 && (
                <MessageAttachments attachments={message.attachments} />
              )}
              {isInlineEditing ? (
                <InlineEditForm
                  message={message}
                  onSubmit={handleInlineEditSubmit}
                  onCancel={() => onCancelInlineEdit?.()}
                />
              ) : message.content && (
                <UserMessageContent>{message.content}</UserMessageContent>
              )}
            </>
          )}
        </MessageContent>

        {(message.content || message.error || (message.attachments && message.attachments.length > 0)) && !isStreaming && !isInlineEditing && (
          <MessageActions className="pl-[46px] mt-0.5 min-h-[28px]">
            <CopyButton content={message.content} />
            {message.role === 'assistant' && conversationId && (
              <MigrateToAgentButton conversationId={conversationId} />
            )}
            {message.role === 'user' && onResendMessage && (
              <MessageAction
                tooltip="重新发送"
                onClick={() => { void onResendMessage(message) }}
              >
                <RotateCcw className="size-3.5" />
              </MessageAction>
            )}
            {message.role === 'user' && onStartInlineEdit && (
              <MessageAction
                tooltip="编辑后重发"
                onClick={() => onStartInlineEdit(message)}
              >
                <Pencil className="size-3.5" />
              </MessageAction>
            )}
            {onDeleteMessage && (
              <MessageAction
                tooltip="删除"
                onClick={() => setDeleteDialogOpen(true)}
              >
                <Trash2 className="size-3.5" />
              </MessageAction>
            )}
            {message.role === 'assistant' && message.error && (
              <span className="text-[11px] text-destructive ml-1 flex items-center gap-0.5">
                <AlertCircle className="size-3" />
                生成失败
              </span>
            )}
            {message.role === 'assistant' && message.stopped && !message.error && (
              <span className="text-[11px] text-foreground/40 ml-1">（已中止）</span>
            )}
          </MessageActions>
        )}
      </Message>

      <DeleteMessageDialog
        open={deleteDialogOpen}
        onOpenChange={setDeleteDialogOpen}
        onConfirm={handleDeleteConfirm}
        isDeleting={isDeleting}
      />
    </>
  )
})
