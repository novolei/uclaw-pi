import { describe, it, expect } from 'vitest'
import { renderWithProviders } from '@/test-utils/render'
import { PlanModeDashedBorder } from './PlanModeDashedBorder'

describe('PlanModeDashedBorder', () => {
  it('renders the positioned overlay container', () => {
    // The ResizeObserver stub never fires, so size stays {0,0} and the inner
    // <svg> is not rendered — the outer absolutely-positioned div always is.
    const { container } = renderWithProviders(<PlanModeDashedBorder />)
    const overlay = container.querySelector('div.absolute.pointer-events-none')
    expect(overlay).toBeTruthy()
    expect(container.querySelector('svg')).toBeNull()
  })
})
