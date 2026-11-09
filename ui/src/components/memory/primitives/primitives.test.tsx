import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { KindBadge, MEMORY_KINDS, MEMORY_KIND_CLASSES, ProgressBar, StatPill, MemorySkeleton } from './index'

const RAW_PALETTE = /(blue|green|red|amber|purple|cyan|orange|emerald|zinc|rose|violet)-\d/

function allClassNames(root: Element): string {
  return [root, ...Array.from(root.querySelectorAll('*'))]
    .map((el) => el.getAttribute('class') ?? '')
    .join(' ')
}

describe('KindBadge', () => {
  it.each(MEMORY_KINDS)('renders %s with its token classes and no raw palette', (kind) => {
    const { container } = render(<KindBadge kind={kind} />)
    const badge = container.firstElementChild!
    const classes = allClassNames(badge)
    expect(badge).toHaveAttribute('data-kind', kind)
    expect(classes).toContain(`bg-memory-${kind}-bg`)
    expect(classes).toContain(`text-memory-${kind}`)
    expect(classes).toContain(`bg-memory-${kind}`)
    expect(classes).not.toMatch(RAW_PALETTE)
  })

  it('dot variant keeps the label muted and only colors the dot', () => {
    const { container } = render(<KindBadge kind="lesson" variant="dot" size="md" />)
    const badge = container.firstElementChild!
    expect(badge.className).toContain('text-muted-foreground')
    expect(badge.className).not.toContain('bg-memory-lesson-bg')
    expect(badge.querySelector('[aria-hidden]')!.className).toContain('bg-memory-lesson')
    expect(screen.getByText('经验')).toBeInTheDocument()
  })

  it('accepts a label override', () => {
    render(<KindBadge kind="procedure" label="学得技能" />)
    expect(screen.getByText('学得技能')).toBeInTheDocument()
  })

  it('kind class map has no raw palette colors', () => {
    expect(JSON.stringify(MEMORY_KIND_CLASSES)).not.toMatch(RAW_PALETTE)
  })
})

describe('ProgressBar', () => {
  it('exposes determinate progress via aria-valuenow', () => {
    render(<ProgressBar value={0.42} label="下载记忆模型" />)
    const bar = screen.getByRole('progressbar', { name: '下载记忆模型' })
    expect(bar).toHaveAttribute('aria-valuenow', '42')
    expect(bar).toHaveAttribute('aria-valuemin', '0')
    expect(bar).toHaveAttribute('aria-valuemax', '100')
    expect(bar).toHaveAttribute('data-state', 'determinate')
    const fill = bar.querySelector('[data-part="indicator"]') as HTMLElement
    expect(fill.style.transform).toBe('translateX(-58%)')
  })

  it('clamps out-of-range and NaN values', () => {
    const { rerender } = render(<ProgressBar value={1.7} label="x" />)
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '100')
    rerender(<ProgressBar value={Number.NaN} label="x" />)
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '0')
  })

  it('switches to indeterminate: no valuenow, sweep is motion-safe only', () => {
    const { rerender } = render(<ProgressBar value={0.3} label="索引中" />)
    rerender(<ProgressBar value={0.3} indeterminate label="索引中" />)
    const bar = screen.getByRole('progressbar', { name: '索引中' })
    expect(bar).not.toHaveAttribute('aria-valuenow')
    expect(bar).toHaveAttribute('data-state', 'indeterminate')
    expect(bar).toHaveAttribute('aria-busy', 'true')
    const ind = bar.querySelector('[data-part="indicator"]')!
    expect(ind.className).toContain('motion-safe:animate-memory-indeterminate')
    expect(ind.className).not.toMatch(/(^|\s)animate-/)
    expect(allClassNames(bar)).not.toMatch(RAW_PALETTE)
  })
})

describe('ProgressBar accessible name', () => {
  it('accepts aria-labelledby instead of label', () => {
    render(
      <>
        <span id="cap">重建索引</span>
        <ProgressBar value={0.5} aria-labelledby="cap" />
      </>,
    )
    expect(screen.getByRole('progressbar', { name: '重建索引' })).toBeInTheDocument()
  })

  it('requires a name at the type level', () => {
    // @ts-expect-error — neither label nor aria-labelledby
    const el = <ProgressBar value={0.5} />
    expect(el).toBeTruthy()
  })
})

describe('StatPill', () => {
  it.each([
    ['neutral', 'text-foreground'],
    ['success', 'text-success-strong'],
    ['warning', 'text-warning-strong'],
    ['danger', 'text-danger-strong'],
  ] as const)('%s tone colors the value with semantic tokens', (tone, cls) => {
    const { container } = render(<StatPill label="记忆" value={1204} tone={tone} />)
    const pill = container.firstElementChild!
    expect(pill).toHaveAttribute('data-tone', tone)
    expect(screen.getByText('1204').className).toContain(cls)
    // Label sits on a tinted pill: foreground/80 keeps ≥ 4.5:1 where muted-foreground doesn't.
    expect(screen.getByText('记忆').className).toContain('text-foreground/80')
    expect(allClassNames(pill)).not.toMatch(RAW_PALETTE)
  })
})

describe('MemorySkeleton', () => {
  it('is decorative and only shimmers when motion is allowed', () => {
    const { container } = render(<MemorySkeleton className="h-3 w-24" />)
    const el = container.firstElementChild!
    expect(el).toHaveAttribute('aria-hidden')
    expect(el.className).toContain('motion-safe:animate-memory-shimmer')
  })
})
