/**
 * AgentMessages.test.tsx — smoke test for the live agent message list after the
 * features/agent migration split.
 *
 * Two cases:
 *  1. empty + not streaming → the WelcomeEmptyState shell renders (no crash).
 *  2. one assistant message with a cost usage → the per-message AgentMessageItem
 *     renders its body AND the user-verified MessageMetaBar cost badge ($ + ¥),
 *     locking the end-to-end render path (list → item → meta bar).
 *
 * IPC is mocked (the attachment-image path routes through @/lib/bridge/agent;
 * raw tauri-apps is stubbed) so nothing reaches the native layer.
 */

import { describe, it, expect, vi } from 'vitest'
import * as React from 'react'
import { renderWithProviders } from '@/test-utils/render'
import { AgentMessages } from './AgentMessages'
import type { AgentMessage } from '@/lib/agent-types'

// ── Module mocks ────────────────────────────────────────────────────────
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }))
vi.mock('@/lib/bridge/agent', () => ({
  readAttachment: vi.fn(async () => ''),
  saveImageAs: vi.fn(async () => {}),
}))

function assistantMsg(overrides: Partial<AgentMessage> = {}): AgentMessage {
  return {
    id: 'a1',
    role: 'assistant',
    content: 'Hello from the agent.',
    createdAt: Date.now(),
    ...overrides,
  } as AgentMessage
}

describe('AgentMessages (list shell)', () => {
  it('renders the empty state when there are no messages and not streaming', () => {
    const { container } = renderWithProviders(
      <AgentMessages sessionId="s1" messages={[]} messagesLoaded streaming={false} />,
    )
    // The Conversation shell mounts; WelcomeEmptyState (or its scroll shell) is present.
    expect(container.firstChild).toBeTruthy()
  })

  it('renders an assistant message body + the user-verified cost badge', () => {
    const { container, getByText } = renderWithProviders(
      <AgentMessages
        sessionId="s1"
        messages={[
          assistantMsg({
            content: 'Hello from the agent.',
            durationMs: 1500,
            usage: { inputTokens: 1234, outputTokens: 56, costUsd: 0.0123 },
          }),
        ]}
        messagesLoaded
        streaming={false}
      />,
    )
    expect(container.textContent ?? '').toContain('Hello from the agent.')
    // The MessageMetaBar cost caption — $ primary + ¥ caption — must render.
    const badge = getByText(/\$0\.0123 \(¥0\.0879\)/)
    expect(badge).toBeTruthy()
    expect(badge.textContent).toContain('¥')
  })
})
