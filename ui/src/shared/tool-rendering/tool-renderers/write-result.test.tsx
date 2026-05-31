import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { Provider, createStore } from 'jotai'
import * as React from 'react'
import { themeModeAtom } from '@/atoms/theme'
import { WriteResultRenderer } from './write-result'

// Mock Pierre — we don't need its real rendering, just verify props pass through.
vi.mock('@pierre/diffs/react', () => ({
  MultiFileDiff: (props: Record<string, unknown>) => (
    <div data-testid="pierre-multifile" data-props={JSON.stringify(props)}>
      pierre stub
    </div>
  ),
}))

function renderWithTheme(theme: 'light' | 'dark', el: React.ReactElement) {
  const store = createStore()
  // resolvedThemeAtom is derived — set themeModeAtom directly ('light' | 'dark' are valid ThemeModes)
  store.set(themeModeAtom, theme)
  return render(<Provider store={store}>{el}</Provider>)
}

describe('WriteResultRenderer', () => {
  it('renders Pierre MultiFileDiff with new file content', () => {
    renderWithTheme('light',
      <WriteResultRenderer
        input={{ path: 'src/foo.ts', content: 'console.log("hi")' }}
        result=""
        isError={false}
      />,
    )
    const pierre = screen.getByTestId('pierre-multifile')
    expect(pierre).toBeInTheDocument()
    const props = JSON.parse(pierre.getAttribute('data-props') ?? '{}')
    // newFile.contents should equal the input.content
    expect(props.newFile.contents).toBe('console.log("hi")')
    // newFile.name should equal the input.path
    expect(props.newFile.name).toBe('src/foo.ts')
    // oldFile.contents should be empty string (new file diff)
    expect(props.oldFile.contents).toBe('')
    // options.theme should reflect light theme → 'one-light'
    expect(props.options.theme).toBe('one-light')
  })

  it('uses dark theme when resolved theme is dark', () => {
    renderWithTheme('dark',
      <WriteResultRenderer
        input={{ path: 'a.md', content: '# Hello' }}
        result=""
        isError={false}
      />,
    )
    const pierre = screen.getByTestId('pierre-multifile')
    const props = JSON.parse(pierre.getAttribute('data-props') ?? '{}')
    expect(props.options.theme).toBe('one-dark-pro')
  })

  it('renders error state when isError', () => {
    renderWithTheme('light',
      <WriteResultRenderer
        input={{ path: 'a.ts', content: 'x' }}
        result="permission denied"
        isError={true}
      />,
    )
    expect(screen.getByText(/permission denied/)).toBeInTheDocument()
    expect(screen.queryByTestId('pierre-multifile')).not.toBeInTheDocument()
  })

  it('gracefully handles missing path/content (empty input)', () => {
    renderWithTheme('light',
      <WriteResultRenderer input={{}} result="" isError={false} />,
    )
    expect(screen.getByText(/missing path|无路径/i)).toBeInTheDocument()
  })
})
