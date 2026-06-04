/**
 * agent-message-helpers.test.ts — pins the persisted→events conversion that lets
 * reloaded assistant messages render their tool-call history (#94). The backend
 * hydrates `message.toolActivities` (chat format: a `start`+`result` entry per
 * call, keyed by `toolCallId`); the renderer keys off `events`, so this mapping
 * (start→tool_start, result→tool_result, toolCallId→toolUseId) is the seam.
 */

import { describe, it, expect } from 'vitest'
import { persistedToolActivitiesToEvents } from './agent-message-helpers'
import type { ChatToolActivity } from '@/lib/chat-types'

describe('persistedToolActivitiesToEvents', () => {
  it('maps chat-format start/result activities → events with toolUseId', () => {
    const activities = [
      { toolCallId: 'c1', type: 'start', toolName: 'edit', input: { path: 'a.md' } },
      { toolCallId: 'c1', type: 'result', toolName: 'edit', result: 'ok', isError: false },
    ] as ChatToolActivity[]

    const events = persistedToolActivitiesToEvents(activities)

    expect(events).toHaveLength(2)
    expect(events[0]).toMatchObject({
      type: 'tool_start',
      toolUseId: 'c1',
      toolName: 'edit',
      input: { path: 'a.md' },
    })
    expect(events[1]).toMatchObject({
      type: 'tool_result',
      toolUseId: 'c1',
      result: 'ok',
      isError: false,
    })
  })

  it('returns [] for empty/undefined input', () => {
    expect(persistedToolActivitiesToEvents(undefined)).toEqual([])
    expect(persistedToolActivitiesToEvents([])).toEqual([])
  })
})
