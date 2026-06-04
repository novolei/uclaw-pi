/**
 * AgentMessageItem — the per-message renderer (user bubble / assistant bubble /
 * compaction-fold card). Extracted from AgentMessages.tsx during the
 * features/agent migration split. Behavior is identical to the original — the
 * only changes are import paths (helpers/sub-components now live in sibling
 * files) and the meta bar / cost badge are imported from MessageMetaBar.
 */

import * as React from 'react'
import { useAtomValue } from 'jotai'
import { UserMessageContent } from '@/components/ai-elements/message'
import {
  Message,
  MessageHeader,
  MessageContent,
  MessageActions,
  MessageResponse,
} from '@/components/ai-elements/message'
import { UserAvatar } from '@/components/chat/UserAvatar'
import { CopyButton } from '@/components/chat/CopyButton'
import { formatMessageTime } from '@/components/chat/ChatMessageItem'
import { resolveModelDisplayName } from '@/lib/model-logo'
import { NativeBlockRenderer, ThinkingBlock } from '@/shared/tool-rendering'
import { ChatToolActivityIndicator } from '@/components/chat/ChatToolActivityIndicator'
import { userProfileAtom } from '@/atoms/user-profile'
import { channelsAtom } from '@/atoms/chat-atoms'
import { agentDisplayNameForAtom } from '@/atoms/agent-display-name'
import { parseSkillCitations } from '@/lib/skill-citation'
import { normalizeAgentMarkdown } from '@/lib/normalize-agent-markdown'
import { SkillCitationChips } from '../SkillCitationChips'
import type { AgentMessage } from '@/lib/agent-types'
import {
  agentActivitiesToChatActivities,
  extractToolActivities,
  persistedToolActivitiesToEvents,
  parseAttachedFiles,
  formatRelativeShort,
} from '../../lib/agent-message-helpers'
import { AssistantLogo } from './AssistantLogo'
import { CompactionFoldCard } from './CompactionFoldCard'
import { AttachedFileChip, ToolResultInlineImages } from './MessageAttachments'
import { MessageMetaBar } from './MessageMetaBar'

/** AgentMessageItem 属性接口 */
export interface AgentMessageItemProps {
  message: AgentMessage
  sessionPath?: string | null
  attachedDirs?: string[]
  /** 外层会话 ID — message.sessionId 可能未设置（DB 未填充），作为回退 */
  sessionId: string
  /** 会话当前选用的模型 ID — 当历史消息 message.model 未持久化时作为兜底 */
  sessionModelId?: string
}

export function AgentMessageItem({ message, sessionPath, attachedDirs, sessionId, sessionModelId }: AgentMessageItemProps): React.ReactElement | null {
  const userProfile = useAtomValue(userProfileAtom)
  const channels = useAtomValue(channelsAtom)
  const agentNameLookup = useAtomValue(agentDisplayNameForAtom)
  const isCompacted = message.compacted === true

  if (message.role === 'user') {
    const { files: attachedFiles, text: messageText } = parseAttachedFiles(message.content)

    if (attachedFiles.length === 0 && !messageText.trim()) {
      return null;
    }

    // M2-G fold placeholder synth user message produced by /compact
    // intercept (tauri_commands.rs) starts with this header. Render as a
    // collapsible "context fold card" rather than a regular user bubble
    // — the boundary divider above and the markdown body below were
    // showing up as "two compactions" in the user's E2E feedback.
    if (messageText.startsWith('## Earlier conversation (compacted)')) {
      return <CompactionFoldCard markdown={messageText} createdAt={message.createdAt} />
    }

    return (
      <div className={isCompacted ? 'opacity-40 grayscale-[0.3]' : undefined}>
      <Message from="user">
        <div className="flex items-start gap-2.5 mb-2.5">
          <UserAvatar avatar={userProfile.avatar} size={35} />
          <div className="flex flex-col justify-between h-[35px]">
            <span className="text-sm font-semibold text-foreground/60 leading-none">{userProfile.userName}</span>
            <span className="text-[10px] text-foreground/[0.38] leading-none">{formatMessageTime(message.createdAt)}</span>
          </div>
        </div>
        <MessageContent>
          {attachedFiles.length > 0 && (
            <div className="flex flex-wrap gap-1.5 mb-2">
              {attachedFiles.map((file) => (
                <AttachedFileChip key={file.path} file={file} />
              ))}
            </div>
          )}
          {messageText && (
            <UserMessageContent>{messageText}</UserMessageContent>
          )}
        </MessageContent>
        {/* 操作按钮（hover 时可见） */}
        {messageText && (
          <MessageActions className="pl-[46px] mt-0.5">
            <CopyButton content={messageText} />
          </MessageActions>
        )}
      </Message>
      </div>
    )
  }

  if (message.role === 'assistant') {
    // Live messages carry `events`; persisted/reloaded messages carry
    // `toolActivities` (ChatToolActivity[]) and no `events` — convert so the
    // tool-call process renders after the turn / on reload, not just live.
    const toolActivities = extractToolActivities(
      message.events ?? persistedToolActivitiesToEvents(message.toolActivities),
    )
    // Parse skill citations once — used both to clean the body for
    // markdown render and to drive the chip row below MessageContent.
    const parsed = message.content
      ? parseSkillCitations(normalizeAgentMarkdown(message.content))
      : { cleanedContent: '', citations: [] }

    // model fallback chain: 持久化字段 → 会话当前选用模型；保证表头始终能显示 model caption + logo
    const resolvedModelId = message.model ?? sessionModelId
    return (
      <div className={isCompacted ? 'opacity-40 grayscale-[0.3]' : undefined}>
      <Message from="assistant">
        <MessageHeader
          name={agentNameLookup(message.sessionId ?? sessionId)}
          model={resolvedModelId ? resolveModelDisplayName(resolvedModelId, channels) : undefined}
          time={formatMessageTime(message.createdAt)}
          logo={<AssistantLogo model={resolvedModelId} />}
        />
        <MessageContent>
          {message.contentBlocks && message.contentBlocks.length > 0 ? (
            <NativeBlockRenderer
              blocks={message.contentBlocks}
              conversationId={message.sessionId ?? sessionId}
            />
          ) : (
            <>
              {/* 历史消息的 thinking block — 从持久化的 reasoning 字段渲染 */}
              {message.reasoning && (
                <div className="mb-3">
                  <ThinkingBlock
                    block={{ type: 'thinking', thinking: message.reasoning } as any}
                    dimmed={false}
                    sessionId={message.sessionId ?? sessionId ?? null}
                  />
                </div>
              )}
              {/* 历史消息工具调用 — 优先用 message.toolActivities（chat 格式，PR #5 持久化的）；
                  若为空则把从 events 提取的 agent 格式转换为 chat 格式，统一用
                  ChatToolActivityIndicator 渲染（🔧 工具名 + 折叠结果卡片）。 */}
              {(message.toolActivities && message.toolActivities.length > 0) ? (
                <div className="mb-3">
                  <ChatToolActivityIndicator activities={message.toolActivities} />
                </div>
              ) : toolActivities.length > 0 ? (
                <div className="mb-3">
                  <ChatToolActivityIndicator activities={agentActivitiesToChatActivities(toolActivities)} />
                </div>
              ) : null}
              <ToolResultInlineImages activities={toolActivities} />
              {message.content && (
                <MessageResponse sessionId={message.sessionId ?? sessionId ?? null}>{parsed.cleanedContent}</MessageResponse>
              )}
            </>
          )}
        </MessageContent>
        {/* Skill citation chips — sibling of MessageActions so the
            pl-[46px] indent aligns with the avatar gutter. */}
        <SkillCitationChips citations={parsed.citations} messageKey={message.id} />
        {/* 操作栏：左侧靠左排列 */}
        {(message.durationMs != null || message.usage || message.content) && (
          <MessageActions className="pl-[46px] mt-0.5 justify-start gap-2.5">
            <MessageMetaBar durationMs={message.durationMs ?? undefined} usage={message.usage ?? undefined} />
            {message.content && <CopyButton content={message.content} />}
            <span className="text-[12px] text-muted-foreground/40 tabular-nums">
              {formatRelativeShort(message.createdAt)}
            </span>
          </MessageActions>
        )}
      </Message>
      </div>
    )
  }

  return null
}
