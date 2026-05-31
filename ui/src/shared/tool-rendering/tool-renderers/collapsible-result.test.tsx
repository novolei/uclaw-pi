import { describe, it, expect } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import * as React from 'react'
import { CollapsibleResult } from './collapsible-result'

const SHORT = 'short content'
const LONG = 'x'.repeat(4000)  // exceeds default 3000 threshold

describe('CollapsibleResult', () => {
  it('renders children directly when below threshold', () => {
    render(<CollapsibleResult>{SHORT}</CollapsibleResult>)
    expect(screen.getByText(SHORT)).toBeInTheDocument()
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
  })

  it('renders collapse toggle when over threshold', () => {
    render(<CollapsibleResult><pre>{LONG}</pre></CollapsibleResult>)
    expect(screen.getByRole('button', { name: /展开全部/ })).toBeInTheDocument()
  })

  it('toggle button expands and collapses content', () => {
    render(<CollapsibleResult><pre>{LONG}</pre></CollapsibleResult>)
    const btn = screen.getByRole('button')
    expect(btn).toHaveTextContent('展开全部')
    fireEvent.click(btn)
    expect(btn).toHaveTextContent('收起')
    fireEvent.click(btn)
    expect(btn).toHaveTextContent('展开全部')
  })

  it('respects custom charThreshold prop', () => {
    render(<CollapsibleResult charThreshold={10}>{SHORT}</CollapsibleResult>)
    expect(screen.getByRole('button')).toBeInTheDocument()  // 13 chars > 10
  })
})
