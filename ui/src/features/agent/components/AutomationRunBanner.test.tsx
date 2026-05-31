import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { AutomationRunBanner } from './AutomationRunBanner'

describe('AutomationRunBanner', () => {
  it('renders nothing for a non-automation session', () => {
    const { container } = render(
      <AutomationRunBanner metadataJson={'{"origin":"human"}'} />,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders the trigger origin for an automation run session', () => {
    render(
      <AutomationRunBanner
        metadataJson={'{"origin":"automation:schedule","spec_id":"s1"}'}
      />,
    )
    expect(screen.getByText(/automation run/i)).toBeInTheDocument()
    expect(screen.getByText(/schedule/i)).toBeInTheDocument()
  })

  it('renders nothing for null/garbage metadata', () => {
    const { container: c1 } = render(<AutomationRunBanner metadataJson={null} />)
    expect(c1.firstChild).toBeNull()
    const { container: c2 } = render(<AutomationRunBanner metadataJson={'not json'} />)
    expect(c2.firstChild).toBeNull()
  })
})
