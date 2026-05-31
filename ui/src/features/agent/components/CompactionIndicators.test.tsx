import { describe, it, expect } from 'vitest'
import { render } from '@testing-library/react'

import { CompactBoundaryDivider, CompactingIndicator } from './CompactionIndicators'

describe('CompactionIndicators', () => {
  it('CompactBoundaryDivider renders the settled label', () => {
    const { getByText } = render(<CompactBoundaryDivider />)
    expect(getByText('上下文已压缩')).toBeInTheDocument()
  })

  it('CompactBoundaryDivider shows counts when both provided and removed > 0', () => {
    const { getByText } = render(<CompactBoundaryDivider removed={12} remaining={3} />)
    expect(getByText(/12 条已压缩 · 保留 3 条/)).toBeInTheDocument()
  })

  it('CompactingIndicator renders the in-progress label', () => {
    const { getByText } = render(<CompactingIndicator />)
    expect(getByText('正在压缩...')).toBeInTheDocument()
  })
})
