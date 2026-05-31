/**
 * MessageMetaBar.test.tsx — locks the USER-VERIFIED token/cost/duration badge.
 *
 * The cost caption (`$USD (¥CNY)`, CNY_PER_USD = 7.15, with the <0.1 → 4dp vs
 * ≥0.1 → 2dp split) is a surface the user has personally checked, so this test
 * pins its exact rendering after the features/agent migration split. Pure
 * component — only needs the Tooltip provider (renderWithProviders).
 */

import { describe, it, expect } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { MessageMetaBar, DurationBadge, formatDuration, buildUsageTooltip } from './MessageMetaBar'

describe('MessageMetaBar (user-verified cost badge)', () => {
  it('renders the $USD cost with the ¥CNY caption (sub-0.1 → 4 decimals)', () => {
    // 0.0123 × 7.15 = 0.087945 → < 0.1 → ¥ shown to 4 decimals.
    const { getByText } = renderWithProviders(
      <MessageMetaBar
        durationMs={1500}
        usage={{ inputTokens: 1234, outputTokens: 56, costUsd: 0.0123 }}
      />,
    )
    const badge = getByText(/\$0\.0123/)
    expect(badge).toBeTruthy()
    // The $ primary and the ¥ caption live in the same cost part string.
    expect(badge.textContent).toContain('$')
    expect(badge.textContent).toContain('¥')
    expect(badge.textContent).toContain('$0.0123 (¥0.0879)')
  })

  it('renders the ¥CNY caption to 2 decimals when ≥ 0.1', () => {
    // 0.05 × 7.15 = 0.3575 → ≥ 0.1 → ¥ shown to 2 decimals.
    const { getByText } = renderWithProviders(
      <MessageMetaBar usage={{ inputTokens: 1000, outputTokens: 200, costUsd: 0.05 }} />,
    )
    expect(getByText(/\$0\.0500 \(¥0\.36\)/)).toBeTruthy()
  })

  it('renders duration + input/output parts', () => {
    const { container } = renderWithProviders(
      <MessageMetaBar durationMs={2300} usage={{ inputTokens: 1234, outputTokens: 56 }} />,
    )
    const text = container.textContent ?? ''
    expect(text).toContain('2.3s')
    expect(text).toContain('1,234 输入')
    expect(text).toContain('56 输出')
  })

  it('omits the cost part when costUsd is absent or 0', () => {
    const { container } = renderWithProviders(
      <MessageMetaBar usage={{ inputTokens: 10, outputTokens: 5, costUsd: 0 }} />,
    )
    expect(container.textContent ?? '').not.toContain('¥')
  })

  it('returns null when there is nothing to show', () => {
    const { container } = renderWithProviders(<MessageMetaBar />)
    expect(container.firstChild).toBeNull()
  })

  it('DurationBadge renders the formatted duration', () => {
    const { getByText } = renderWithProviders(<DurationBadge durationMs={2300} />)
    expect(getByText('2.3s')).toBeTruthy()
  })

  it('formatDuration formats ms / s / m+s', () => {
    expect(formatDuration(500)).toBe('500ms')
    expect(formatDuration(2300)).toBe('2.3s')
    expect(formatDuration(65000)).toBe('1m 5s')
  })

  it('buildUsageTooltip lists pure-input / output / cache lines', () => {
    const text = buildUsageTooltip(1500, {
      inputTokens: 1000,
      outputTokens: 200,
      cacheReadTokens: 300,
      cacheCreationTokens: 100,
    })
    expect(text).toContain('耗时: 1.5s')
    expect(text).toContain('输入: 600') // 1000 - 300 - 100
    expect(text).toContain('输出: 200')
    expect(text).toContain('缓存写入: 100')
    expect(text).toContain('缓存读取: 300')
  })
})
