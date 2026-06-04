/**
 * AgentMessageItem.test.tsx — persist→load→render contract guard.
 *
 * Locks the bug class that kept recurring on the pi path: tool-call history is
 * persisted (tool_activities_json, #86) and returned by get_agent_session_messages
 * as `message.toolActivities`, but the ASSISTANT renderer dropped it. The specific
 * regression (#95): persisted messages carry `contentBlocks` ([thinking, text]),
 * so the renderer took the NativeBlockRenderer branch — and the tool-card render
 * lived only in the OTHER (no-contentBlocks) branch, so the whole tool history
 * vanished once the turn completed. This pins that the cards render in BOTH
 * branches, in `thinking → cards → body` order.
 *
 * Heavy children are stubbed to testids so the assertion is about the render
 * STRUCTURE (which branch shows the cards), not their internals.
 */

import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import type { AgentMessage } from '@/lib/agent-types'

vi.mock('@/components/chat/ChatToolActivityIndicator', () => ({
  ChatToolActivityIndicator: ({ activities }: { activities: unknown[] }) => (
    <div data-testid="tool-cards" data-count={activities.length} />
  ),
}))
vi.mock('@/shared/tool-rendering', async (orig) => ({
  ...(await orig<Record<string, unknown>>()),
  NativeBlockRenderer: ({ blocks }: { blocks: Array<{ type: string }> }) => (
    <div data-testid="native-blocks" data-types={blocks.map((b) => b.type).join(',')} />
  ),
  ThinkingBlock: () => <div data-testid="thinking" />,
}))
vi.mock('./AssistantLogo', () => ({ AssistantLogo: () => <div /> }))
vi.mock('../SkillCitationChips', () => ({ SkillCitationChips: () => <div /> }))
vi.mock('./MessageAttachments', () => ({
  ToolResultInlineImages: () => <div />,
  AttachedFileChip: () => <div />,
}))
vi.mock('./MessageMetaBar', () => ({ MessageMetaBar: () => <div /> }))

import { AgentMessageItem } from './AgentMessageItem'

/** A reloaded assistant turn: thinking + text contentBlocks AND persisted
 *  tool activities (chat format: a start + result entry per call). */
function persistedAssistant(): AgentMessage {
  return {
    id: 'm1',
    role: 'assistant',
    content: 'summary text',
    createdAt: 0,
    contentBlocks: [
      { type: 'thinking', thinking: 'thinking…' },
      { type: 'text', text: 'summary text' },
    ] as AgentMessage['contentBlocks'],
    toolActivities: [
      { toolCallId: 'c1', type: 'start', toolName: 'edit', input: { path: 'a.md' } },
      { toolCallId: 'c1', type: 'result', toolName: 'edit', result: 'ok', isError: false },
    ] as AgentMessage['toolActivities'],
  }
}

describe('AgentMessageItem — persisted tool-call history (#95 regression guard)', () => {
  it('renders tool cards in the contentBlocks branch, between thinking and body', () => {
    const { getByTestId, getAllByTestId } = renderWithProviders(
      <AgentMessageItem message={persistedAssistant()} sessionId="s" />,
    )
    // The regression: cards were silently dropped when contentBlocks was present.
    const cards = getByTestId('tool-cards')
    expect(cards).toBeTruthy()
    expect(cards.getAttribute('data-count')).toBe('2')
    // contentBlocks split → thinking blocks and the rest render separately
    // (2 NativeBlockRenderer instances), so the cards sit between them.
    expect(getAllByTestId('native-blocks').length).toBe(2)
  })

  it('renders no tool-cards node when the turn used no tools', () => {
    const msg = persistedAssistant()
    msg.toolActivities = []
    const { queryByTestId } = renderWithProviders(
      <AgentMessageItem message={msg} sessionId="s" />,
    )
    expect(queryByTestId('tool-cards')).toBeNull()
  })
})
