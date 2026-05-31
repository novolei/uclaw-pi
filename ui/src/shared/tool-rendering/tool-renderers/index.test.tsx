import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import * as React from 'react'
import { ToolResultRenderer } from './index'

// Mock each specialized renderer so we can verify dispatch
vi.mock('./write-result', () => ({
  WriteResultRenderer: () => <div data-testid="write-renderer">write</div>,
}))
vi.mock('./edit-result', () => ({
  EditResultRenderer: () => <div data-testid="edit-renderer">edit</div>,
}))
vi.mock('./read-result', () => ({
  ReadResultRenderer: () => <div data-testid="read-renderer">read</div>,
}))
vi.mock('./bash-result', () => ({
  BashResultRenderer: () => <div data-testid="bash-renderer">bash</div>,
}))

const baseProps = { input: {}, result: '', isError: false }

describe('ToolResultRenderer dispatch', () => {
  it('dispatches write_file to WriteResultRenderer', () => {
    render(<ToolResultRenderer toolName="write_file" {...baseProps} />)
    expect(screen.getByTestId('write-renderer')).toBeInTheDocument()
  })

  it('dispatches edit to EditResultRenderer', () => {
    render(<ToolResultRenderer toolName="edit" {...baseProps} />)
    expect(screen.getByTestId('edit-renderer')).toBeInTheDocument()
  })

  it('dispatches read_file to ReadResultRenderer', () => {
    render(<ToolResultRenderer toolName="read_file" {...baseProps} />)
    expect(screen.getByTestId('read-renderer')).toBeInTheDocument()
  })

  it('dispatches bash to BashResultRenderer', () => {
    render(<ToolResultRenderer toolName="bash" {...baseProps} />)
    expect(screen.getByTestId('bash-renderer')).toBeInTheDocument()
  })

  it('falls back to DefaultResultRenderer for unknown tool', () => {
    render(<ToolResultRenderer toolName="some_mcp_tool" {...baseProps} result="result text" />)
    expect(screen.getByText('result text')).toBeInTheDocument()
  })

  it('renders structured gbrain errors with recovery hints', () => {
    render(
      <ToolResultRenderer
        toolName="mcp__gbrain__get_page"
        input={{ slug: 'aknowledge/openai-gpt5' }}
        isError={true}
        result={'Server error: {"ok":false,"source":"gbrain","tool":"get_page","kind":"page_not_found","status":"exit status: 1","message":"gbrain page not found","hint":"Pick an existing slug from the suggestions or retry with fuzzy=true/include_deleted=true.","nearest_slugs":["knowledge/openai-gpt5"]}'}
      />,
    )

    expect(screen.getByText(/页面不存在/)).toBeInTheDocument()
    expect(screen.getByText('knowledge/openai-gpt5')).toBeInTheDocument()
    expect(screen.getByText(/fuzzy=true/)).toBeInTheDocument()
  })
})
