/**
 * Memory kinds — the six semantic categories every memory surface colors by.
 *
 * Class strings are spelled out literally (not built with template strings)
 * so Tailwind's JIT scanner can see them. Colors resolve through the
 * `--memory-kind-*` tokens in styles/globals.css, which are tuned per theme.
 * See docs/design/memory-ui.md › Kind colors.
 */

export const MEMORY_KINDS = [
  'fact',
  'preference',
  'episode',
  'lesson',
  'entity',
  'procedure',
] as const

export type MemoryKind = (typeof MEMORY_KINDS)[number]

export const MEMORY_KIND_LABELS: Record<MemoryKind, string> = {
  fact: '事实',
  preference: '偏好',
  episode: '事件',
  lesson: '经验',
  entity: '实体',
  procedure: '技能',
}

export interface MemoryKindClasses {
  /** Ink as a fill — dots, bars, the left rule on a card. */
  dot: string
  /** Ink as text — badge labels (≥4.5:1 on `surface` in every theme). */
  text: string
  /** Tinted surface behind a badge / chip. */
  surface: string
  /** Hairline border in the kind's hue. */
  border: string
}

export const MEMORY_KIND_CLASSES: Record<MemoryKind, MemoryKindClasses> = {
  fact: {
    dot: 'bg-memory-fact',
    text: 'text-memory-fact',
    surface: 'bg-memory-fact-bg',
    border: 'border-memory-fact/20',
  },
  preference: {
    dot: 'bg-memory-preference',
    text: 'text-memory-preference',
    surface: 'bg-memory-preference-bg',
    border: 'border-memory-preference/20',
  },
  episode: {
    dot: 'bg-memory-episode',
    text: 'text-memory-episode',
    surface: 'bg-memory-episode-bg',
    border: 'border-memory-episode/20',
  },
  lesson: {
    dot: 'bg-memory-lesson',
    text: 'text-memory-lesson',
    surface: 'bg-memory-lesson-bg',
    border: 'border-memory-lesson/20',
  },
  entity: {
    dot: 'bg-memory-entity',
    text: 'text-memory-entity',
    surface: 'bg-memory-entity-bg',
    border: 'border-memory-entity/20',
  },
  procedure: {
    dot: 'bg-memory-procedure',
    text: 'text-memory-procedure',
    surface: 'bg-memory-procedure-bg',
    border: 'border-memory-procedure/20',
  },
}

export function isMemoryKind(value: unknown): value is MemoryKind {
  return typeof value === 'string' && (MEMORY_KINDS as readonly string[]).includes(value)
}
