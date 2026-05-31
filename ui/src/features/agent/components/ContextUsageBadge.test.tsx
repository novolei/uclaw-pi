import { describe, it, expect, vi } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { ContextUsageBadge } from './ContextUsageBadge'

// The badge is pure props-in (no IPC), so no bridge mock is needed.

describe('ContextUsageBadge', () => {
  it('renders nothing when there is no usage data', () => {
    const { container } = renderWithProviders(
      <ContextUsageBadge isCompacting={false} isProcessing={false} onCompact={vi.fn()} />,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders a disabled spinner button while compacting', () => {
    const { getByRole } = renderWithProviders(
      <ContextUsageBadge isCompacting isProcessing={false} onCompact={vi.fn()} />,
    )
    const btn = getByRole('button')
    expect(btn).toBeTruthy()
    expect((btn as HTMLButtonElement).disabled).toBe(true)
  })

  it('renders the ring trigger once usage data is present', () => {
    const { getByRole, container } = renderWithProviders(
      <ContextUsageBadge
        inputTokens={50_000}
        contextWindow={200_000}
        isCompacting={false}
        isProcessing={false}
        onCompact={vi.fn()}
      />,
    )
    // Trigger button renders, and the ring SVG is inside it.
    expect(getByRole('button')).toBeTruthy()
    expect(container.querySelector('svg')).toBeTruthy()
  })

  it('opens the popover on hover and shows the token breakdown + compact button', async () => {
    const onCompact = vi.fn()
    const { getByRole, findByText, user } = renderWithProviders(
      <ContextUsageBadge
        inputTokens={50_000}
        outputTokens={1_200}
        cacheReadTokens={30_000}
        contextWindow={200_000}
        isCompacting={false}
        isProcessing={false}
        onCompact={onCompact}
      />,
    )
    await user.hover(getByRole('button'))
    // Popover body sections + the compact action.
    expect(await findByText('输入构成')).toBeTruthy()
    expect(await findByText('缓存效率')).toBeTruthy()
    const compactBtn = await findByText('手动压缩')
    await user.click(compactBtn)
    expect(onCompact).toHaveBeenCalledTimes(1)
  })

  it('disables the compact action while a turn is in progress', async () => {
    const { getByRole, findByText, user } = renderWithProviders(
      <ContextUsageBadge
        inputTokens={50_000}
        contextWindow={200_000}
        isCompacting={false}
        isProcessing
        onCompact={vi.fn()}
      />,
    )
    await user.hover(getByRole('button'))
    expect(await findByText('对话进行中')).toBeTruthy()
  })
})
