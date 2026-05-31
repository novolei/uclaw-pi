import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import * as React from 'react'
import { BashResultRenderer } from './bash-result'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

describe('BashResultRenderer', () => {
  it('renders command echo with $ prefix + stdout body', () => {
    render(
      <BashResultRenderer
        input={{ command: 'ls -la' }}
        result="total 8\ndrwxr-xr-x   2 root root 4096 May 18 10:00 ."
        isError={false}
      />,
    )
    expect(screen.getByText('$ ls -la')).toBeInTheDocument()
    expect(screen.getByText(/total 8/)).toBeInTheDocument()
  })

  it('highlights lines matching error patterns in red', () => {
    const { container } = render(
      <BashResultRenderer
        input={{ command: 'cargo build' }}
        result="compiling foo v0.1.0\nerror: cannot find module\nDone."
        isError={false}
      />,
    )
    const errorLine = container.querySelector('.text-red-400')
    expect(errorLine).toBeInTheDocument()
    expect(errorLine?.textContent).toContain('error: cannot find module')
  })

  it('marks all lines red when isError is true', () => {
    const { container } = render(
      <BashResultRenderer
        input={{ command: 'false' }}
        result="line 1\nline 2"
        isError={true}
      />,
    )
    const redLines = container.querySelectorAll('.text-red-400')
    expect(redLines.length).toBeGreaterThanOrEqual(2)
  })

  it('handles empty command gracefully', () => {
    render(<BashResultRenderer input={{}} result="output" isError={false} />)
    expect(screen.getByText('$')).toBeInTheDocument()
    expect(screen.getByText('output')).toBeInTheDocument()
  })

  it('shows the amber truncation banner + log button and strips the raw header', () => {
    const { container } = render(
      <BashResultRenderer
        input={{ command: 'cat big.txt' }}
        result={
          '[输出已截断：共 40001 字节，显示最后 32768 字节，完整输出已保存至 /tmp/uclaw/bash-abc.log]\n\nlast chunk of output'
        }
        isError={false}
      />,
    )
    // Amber banner + load button render
    expect(screen.getByText('⋯ 早期输出已截断')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '加载完整日志' })).toBeInTheDocument()
    // Tail body is shown, raw header text is NOT rendered as terminal output
    expect(screen.getByText(/last chunk of output/)).toBeInTheDocument()
    expect(container.textContent).not.toContain('[输出已截断')
  })

  it('renders no truncation banner for normal (untruncated) output', () => {
    render(
      <BashResultRenderer input={{ command: 'echo hi' }} result="hi" isError={false} />,
    )
    expect(screen.queryByText('⋯ 早期输出已截断')).not.toBeInTheDocument()
  })
})
