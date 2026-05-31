/**
 * ToolActivityItem.test.tsx
 *
 * Tests for the 预览 (Preview) button on tool activity rows.
 * Covers: eligibility logic (tool name + path) and click-to-open behavior.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as React from 'react'
import { fireEvent } from '@testing-library/react'
import { renderWithProviders, screen } from '@/test-utils/render'
import { ActivityRow } from './ToolActivityItem'
import { previewTabsAtom } from '@/atoms/preview-panel-atoms'
import type { ToolActivity } from '@/atoms/agent-atoms'

// ── Module mocks ────────────────────────────────────────────────────────

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }))

// Tool-result image IO now routes through the agent bridge (ActivityDetails /
// ToolResultImage). ActivityRow — the unit under test — doesn't touch it, but
// mock the bridge anyway so nothing reaches the real Tauri layer.
vi.mock('@/lib/bridge/agent', () => ({
  readAttachment: vi.fn(async () => ''),
  saveImageAs: vi.fn(async () => {}),
}))

// Keep the real tool metadata helpers (getToolIcon / getToolPhrase / formatElapsed /
// BashStreamView) and only stub the heavyweight result dispatcher.
vi.mock('@/shared/tool-rendering', async (importActual) => ({
  ...(await importActual<typeof import('@/shared/tool-rendering')>()),
  ToolResultRenderer: () => <div data-testid="tool-result-renderer">result</div>,
}))

// ── Fixture helpers ─────────────────────────────────────────────────────

function makeActivity(overrides: Partial<ToolActivity> = {}): ToolActivity {
  return {
    toolUseId: 'test-tool-use-id',
    toolName: 'write_file',
    input: { path: 'src/foo.ts', content: 'x' },
    done: true,
    result: 'ok',
    isError: false,
    ...overrides,
  }
}

function renderRow(activity: ToolActivity, onOpenDetails?: (a: ToolActivity) => void) {
  return renderWithProviders(
    <ActivityRow activity={activity} onOpenDetails={onOpenDetails} />,
  )
}

// ── Tests ───────────────────────────────────────────────────────────────

describe('ToolActivityItem 预览 button', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('shows 预览 button for write_file tool with a path', () => {
    renderRow(makeActivity({ toolName: 'write_file', input: { path: 'src/foo.ts' } }))
    expect(screen.getByRole('button', { name: /预览/ })).toBeInTheDocument()
  })

  it('shows 预览 button for edit tool with a path', () => {
    renderRow(makeActivity({ toolName: 'edit', input: { path: 'src/bar.ts' } }))
    expect(screen.getByRole('button', { name: /预览/ })).toBeInTheDocument()
  })

  it('shows 预览 button for plan_write tool with a path', () => {
    renderRow(makeActivity({ toolName: 'plan_write', input: { path: 'docs/plan.md' } }))
    expect(screen.getByRole('button', { name: /预览/ })).toBeInTheDocument()
  })

  it('does NOT show 预览 button for read_file tool', () => {
    renderRow(makeActivity({ toolName: 'read_file', input: { path: 'src/foo.ts' } }))
    expect(screen.queryByRole('button', { name: /预览/ })).not.toBeInTheDocument()
  })

  it('does NOT show 预览 button for bash tool', () => {
    renderRow(makeActivity({ toolName: 'bash', input: { command: 'ls' } }))
    expect(screen.queryByRole('button', { name: /预览/ })).not.toBeInTheDocument()
  })

  it('does NOT show 预览 button for write_file without a path', () => {
    renderRow(makeActivity({ toolName: 'write_file', input: { content: 'x' } }))
    expect(screen.queryByRole('button', { name: /预览/ })).not.toBeInTheDocument()
  })

  it('clicking 预览 opens the preview tab via openPreviewTabAction', () => {
    const { store } = renderRow(
      makeActivity({ toolName: 'write_file', input: { path: 'src/foo.ts', content: 'x' } }),
    )
    fireEvent.click(screen.getByRole('button', { name: /预览/ }))
    const tabs = store.get(previewTabsAtom)
    expect(tabs.length).toBeGreaterThanOrEqual(1)
    const tab = tabs.find((t) => t.relPath === 'src/foo.ts')
    expect(tab).toBeDefined()
    expect(tab?.source).toBe('agent')
    expect(tab?.name).toBe('foo.ts')
  })

  it('clicking 预览 does not trigger onOpenDetails (stopPropagation)', () => {
    // NOTE: when canExpand=true the row is a <button> and the preview button is
    // nested inside it (button-in-button is invalid HTML but browsers handle it by
    // breaking the inner button out of the outer one at parse time). jsdom preserves
    // the nesting so we identify the preview button by its specific aria-label to
    // avoid the "multiple elements" ambiguity from the outer row button's text content.
    const onOpenDetails = vi.fn()
    renderRow(
      makeActivity({ toolName: 'write_file', input: { path: 'src/foo.ts' } }),
      onOpenDetails,
    )
    const previewBtn = screen.getByLabelText('预览 src/foo.ts')
    fireEvent.click(previewBtn)
    expect(onOpenDetails).not.toHaveBeenCalled()
  })
})
