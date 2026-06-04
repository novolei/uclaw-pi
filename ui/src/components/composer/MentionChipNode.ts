/**
 * MentionChipNode — TipTap inline atom node for `/<skill>` and `@<file>`
 * mentions in the composer.
 *
 * Atomicity is the load-bearing property:
 *   - Cursor can be **before** or **after** the chip, never **inside**.
 *   - Backspace deletes the whole chip in one keystroke.
 *   - Selection includes the whole chip or nothing.
 *
 * Wire-format contract (intentional, see spec §"Wire format compatibility"):
 *   - `editor.getText({ blockSeparator: '\n' })` walks the doc and for each
 *     chip emits `renderText(node)` which produces `/<name>` or `@<absPath>`.
 *   - This keeps `agent_messages.content` as plain TEXT — backend doesn't
 *     need to know chips exist.
 *
 * Chip is a UI sugar layer on top of the same plain-string wire format the
 * pre-PR #130 textarea + popover path produced.
 */
import { Node, mergeAttributes } from '@tiptap/core'

/** Chip kinds — matches the two trigger characters they originate from. */
export type MentionChipKind = 'skill' | 'file'

export interface MentionChipAttrs {
  kind: MentionChipKind
  /** What to display in the chip body — for skills this is the slash name,
   *  for files this is the bare filename (the popup row title). */
  display: string
  /** What to emit in wire-format text — for skills `name` from
   *  list_invocable_skills, for files the `absolutePath` from
   *  search_workspace_files_for_mention. */
  value: string
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    mentionChip: {
      /** Insert a mention chip at the current selection, replacing any
       *  active query span (caller passes `from`/`to` to wipe). */
      insertMentionChip: (attrs: MentionChipAttrs & { from?: number; to?: number }) => ReturnType
    }
  }
}

/** Wire-format serialization for a single chip. Public so the doc walker
 *  in `composer-serialize.ts` can reuse the exact same rule. */
export function chipToWireText(attrs: MentionChipAttrs): string {
  return attrs.kind === 'skill' ? `/${attrs.value}` : `@${attrs.value}`
}

export const MentionChipNode = Node.create({
  name: 'mentionChip',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,

  addAttributes() {
    return {
      kind: {
        default: 'skill' as MentionChipKind,
        parseHTML: (el) => (el.getAttribute('data-kind') as MentionChipKind) ?? 'skill',
        renderHTML: (attrs) => ({ 'data-kind': attrs.kind }),
      },
      display: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-display') ?? '',
        renderHTML: (attrs) => ({ 'data-display': attrs.display }),
      },
      value: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-value') ?? '',
        renderHTML: (attrs) => ({ 'data-value': attrs.value }),
      },
    }
  },

  parseHTML() {
    return [{ tag: 'span[data-mention-chip]' }]
  },

  renderHTML({ node, HTMLAttributes }) {
    const attrs = node.attrs as MentionChipAttrs
    // contenteditable=false makes the chip a true atom in DOM too, matching
    // ProseMirror's atom:true — otherwise the user can click inside and type.
    if (attrs.kind === 'skill') {
      // Skill → a rounded "command" pill: a leading hexagon glyph + the bare
      // skill name (the leading `/` is implied by the icon; wire-format still
      // emits `/name` via renderText, so the backend is unchanged). Neutral
      // pill bg + blue text reads as a distinct, tappable command token.
      return [
        'span',
        mergeAttributes(HTMLAttributes, {
          'data-mention-chip': '',
          class: [
            'inline-flex items-center gap-1 px-2 py-[1px] rounded-full',
            'text-[12px] leading-[1.5] align-baseline font-medium',
            'bg-muted text-blue-600 dark:text-blue-400 border border-border/50',
          ].join(' '),
          contenteditable: 'false',
        }),
        [
          'svg',
          {
            xmlns: 'http://www.w3.org/2000/svg',
            width: '12',
            height: '12',
            viewBox: '0 0 24 24',
            fill: 'none',
            stroke: 'currentColor',
            'stroke-width': '2',
            'stroke-linecap': 'round',
            'stroke-linejoin': 'round',
            'aria-hidden': 'true',
            class: 'shrink-0',
          },
          [
            'path',
            {
              d: 'M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z',
            },
          ],
        ],
        attrs.display,
      ]
    }
    // File → the @-prefixed tint chip (unchanged).
    return [
      'span',
      mergeAttributes(HTMLAttributes, {
        'data-mention-chip': '',
        class: [
          'inline-flex items-center gap-0.5 px-1.5 py-0 rounded',
          'text-[12px] leading-[1.5] align-baseline',
          'bg-blue-500/10 text-blue-700 dark:text-blue-300 border border-blue-500/20',
        ].join(' '),
        contenteditable: 'false',
      }),
      `@${attrs.display}`,
    ]
  },

  /** Live editor view. `renderHTML`'s array spec is serialized via
   *  `document.createElement`, which builds the `<svg>`/`<path>` in the HTML
   *  namespace → they don't render (the icon was invisible). The NodeView builds
   *  the chip with `createElementNS` so the hexagon actually paints. Classes
   *  mirror `renderHTML` so copy/serialize and the live view match. */
  addNodeView() {
    return ({ node }) => {
      const attrs = node.attrs as MentionChipAttrs
      const dom = document.createElement('span')
      dom.setAttribute('data-mention-chip', '')
      dom.setAttribute('data-kind', attrs.kind)
      dom.contentEditable = 'false'

      if (attrs.kind !== 'skill') {
        dom.className = [
          'inline-flex items-center gap-0.5 px-1.5 py-0 rounded',
          'text-[12px] leading-[1.5] align-baseline',
          'bg-blue-500/10 text-blue-700 dark:text-blue-300 border border-blue-500/20',
        ].join(' ')
        dom.textContent = `@${attrs.display}`
        return { dom }
      }

      dom.className = [
        'inline-flex items-center gap-1 px-2 py-[1px] rounded-full',
        'text-[12px] leading-[1.5] align-baseline font-medium',
        'bg-muted text-blue-600 dark:text-blue-400 border border-border/50',
      ].join(' ')

      const svgNS = 'http://www.w3.org/2000/svg'
      const svg = document.createElementNS(svgNS, 'svg')
      svg.setAttribute('width', '12')
      svg.setAttribute('height', '12')
      svg.setAttribute('viewBox', '0 0 24 24')
      svg.setAttribute('fill', 'none')
      svg.setAttribute('stroke', 'currentColor')
      svg.setAttribute('stroke-width', '2')
      svg.setAttribute('stroke-linecap', 'round')
      svg.setAttribute('stroke-linejoin', 'round')
      svg.setAttribute('aria-hidden', 'true')
      svg.style.flex = '0 0 auto'
      const path = document.createElementNS(svgNS, 'path')
      path.setAttribute(
        'd',
        'M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z',
      )
      svg.appendChild(path)
      dom.appendChild(svg)
      dom.appendChild(document.createTextNode(attrs.display))
      return { dom }
    }
  },

  /** TipTap calls this when computing `editor.getText()`. Emits the chip's
   *  wire-format inline form so the resulting plain string matches what a
   *  pre-PR #130 textarea would have contained. */
  renderText({ node }) {
    return chipToWireText(node.attrs as MentionChipAttrs)
  },

  addCommands() {
    return {
      insertMentionChip:
        (attrs) =>
        ({ chain }) => {
          const { from, to, ...nodeAttrs } = attrs
          let c = chain()
          // If the caller provided a span to wipe (the trigger char + query),
          // delete it first so the chip lands where the `/` or `@` was.
          if (from != null && to != null) {
            c = c.deleteRange({ from, to })
          }
          return c
            .insertContent({ type: 'mentionChip', attrs: nodeAttrs })
            // Trailing space so the user can immediately keep typing without
            // accidentally re-triggering on the next character. Mirrors the
            // PR #130 popover commit behavior.
            .insertContent(' ')
            .run()
        },
    }
  },
})
