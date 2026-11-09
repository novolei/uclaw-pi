/**
 * Guards the memory design tokens in globals.css (docs/design/memory-ui.md):
 * every theme block defines every token, kind badge text stays ≥ 4.5:1 on its
 * tinted background (WCAG AA, small text) and on the theme's card, and the
 * progress fill stays ≥ 3:1 on its track (WCAG non-text contrast). Kind hues
 * must also stay apart from each other (≥ 30°) and from the semantic status
 * colors (≥ 25°) so a 偏好 badge never reads as an error next to a danger pill.
 */
import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const css = readFileSync(resolve(__dirname, 'globals.css'), 'utf8')

const KINDS = ['fact', 'preference', 'episode', 'lesson', 'entity', 'procedure']
const BLOCKS = [
  ':root',
  '.dark',
  '.theme-ocean-light',
  '.theme-ocean-dark',
  '.theme-forest-light',
  '.theme-forest-dark',
  '.theme-slate-light',
  '.theme-slate-dark',
  '.theme-warm-paper',
  '.theme-qingye',
  '.theme-black',
  '.theme-the-finals',
]

type Hsl = [number, number, number]

function block(selector: string): string {
  const start = css.indexOf(`  ${selector} {`)
  if (start < 0) throw new Error(`missing block ${selector}`)
  return css.slice(start, css.indexOf('\n  }', start))
}

function token(body: string, name: string): Hsl | null {
  const m = body.match(new RegExp(`--${name}:\\s*([\\d.]+)\\s+([\\d.]+)%\\s+([\\d.]+)%\\s*;`))
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null
}

function luminance([h, s, l]: Hsl): number {
  const S = s / 100
  const L = l / 100
  const a = S * Math.min(L, 1 - L)
  const f = (n: number) => {
    const k = (n + h / 30) % 12
    return L - a * Math.max(-1, Math.min(k - 3, 9 - k, 1))
  }
  const lin = (c: number) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4)
  return 0.2126 * lin(f(0)) + 0.7152 * lin(f(8)) + 0.0722 * lin(f(4))
}

function contrast(a: Hsl, b: Hsl): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (hi + 0.05) / (lo + 0.05)
}

const MIN_KIND_GAP = 30
const MIN_SEMANTIC_GAP = 25

function hueDistance(a: number, b: number): number {
  const d = Math.abs(a - b) % 360
  return Math.min(d, 360 - d)
}

describe('memory design tokens', () => {
  it.each(BLOCKS)('%s: kind text ≥ 4.5:1 on its bg and on the card', (sel) => {
    const body = block(sel)
    const card = token(body, 'card')
    expect(card, `${sel} --card`).not.toBeNull()
    for (const k of KINDS) {
      const fg = token(body, `memory-kind-${k}`)
      const bg = token(body, `memory-kind-${k}-bg`)
      expect(fg, `${sel} --memory-kind-${k}`).not.toBeNull()
      expect(bg, `${sel} --memory-kind-${k}-bg`).not.toBeNull()
      expect(contrast(fg!, bg!), `${sel} ${k} on bg`).toBeGreaterThanOrEqual(4.5)
      expect(contrast(fg!, card!), `${sel} ${k} on card`).toBeGreaterThanOrEqual(4.5)
    }
  })

  it.each(BLOCKS)('%s: progress fill ≥ 3:1 on track', (sel) => {
    const body = block(sel)
    const track = token(body, 'progress-track')
    const fill = token(body, 'progress-fill')
    expect(track && fill, `${sel} progress tokens`).toBeTruthy()
    expect(contrast(fill!, track!)).toBeGreaterThanOrEqual(3)
  })

  it.each(BLOCKS)('%s: kind hues are distinct and clear of semantic colors', (sel) => {
    const body = block(sel)
    const kinds = KINDS.map((k) => [k, token(body, `memory-kind-${k}`)!] as const)
    for (let i = 0; i < kinds.length; i++) {
      for (let j = i + 1; j < kinds.length; j++) {
        const [a, ha] = kinds[i]
        const [b, hb] = kinds[j]
        expect(hueDistance(ha[0], hb[0]), `${sel} ${a}↔${b}`).toBeGreaterThanOrEqual(MIN_KIND_GAP)
      }
    }
    for (const sem of ['success', 'warning', 'danger']) {
      const hs = token(body, sem)
      expect(hs, `${sel} --${sem}`).not.toBeNull()
      for (const [k, hk] of kinds) {
        // Near-greys have no perceivable hue; only then is a collision harmless.
        if (hk[1] < 15 && hs![1] < 15) continue
        expect(hueDistance(hk[0], hs![0]), `${sel} ${k}↔${sem}`).toBeGreaterThanOrEqual(MIN_SEMANTIC_GAP)
      }
    }
  })

  it('elevation tokens exist for light and dark', () => {
    for (const sel of [':root', '.dark']) {
      expect(block(sel)).toMatch(/--elevation-1:/)
      expect(block(sel)).toMatch(/--elevation-2:/)
    }
  })
})
