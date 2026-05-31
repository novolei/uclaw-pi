import { describe, it, expect } from 'vitest'
import { renderWithProviders, screen } from '@/test-utils/render'
import { AutoPreviewPopover } from './AutoPreviewPopover'

describe('AutoPreviewPopover', () => {
  it('renders the toggle trigger (enabled by default)', () => {
    // autoPreviewEnabledAtom defaults to true → "已开启" label + Eye icon.
    renderWithProviders(<AutoPreviewPopover />)
    expect(
      screen.getByRole('button', { name: '自动预览：已开启' }),
    ).toBeInTheDocument()
  })
})
