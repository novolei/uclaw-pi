# Memory UI — visual language

Applies to every memory surface: Settings › 记忆 card, the chat recall chip, and
the Memory Center (Kaleidoscope › 记忆). It refines the app's existing calm,
Linear/Raycast-style chrome — it does not introduce a new style. Reference
surfaces it builds on: `ui/src/features/settings/components/SettingsNav.tsx`
(muted selection + 2 px primary bar), `ui/src/features/settings/components/primitives/*`
(rows, cards, toggles), `ui/src/components/chat/MemoryRecallChip.tsx`,
`ui/src/components/memory/FragmentCard.tsx`.

## Principles

1. **Calm density.** Many small facts on screen, little chrome. Hairlines over boxes,
   whitespace over dividers, one accent per view.
2. **Information first.** The memory's text is the hero; kind, source and time are
   quiet metadata on one line under it.
3. **Provenance visible.** Every memory shows where it came from (conversation, capture,
   import) and when — never a bare claim.
4. **Quiet motion.** Motion explains a change (appear, progress, expand); it never
   decorates. Everything respects `prefers-reduced-motion`.

## Type scale

| Token | px / line-height | Weight | Use |
|---|---|---|---|
| caption | 11 / 16 | 400–500 | metadata, badges, timestamps |
| body-sm | 12 / 18 | 400 | secondary text, pills, helper copy |
| body | 13 / 20 | 400; 500 for item titles | memory text, rows |
| title | 15 / 22 | 600 | card / section titles |
| display | 20 / 28 | 600 | Memory Center page title, one big number |

Use `text-[11px]` … `text-[20px]` with `leading-*`; no other sizes (retire ad-hoc
`text-[10px]` / `[10.5px]`). Numbers use `tabular-nums`. Chinese text: no letter-spacing;
`uppercase tracking-wider` only for Latin eyebrow labels.

## Spacing, radius, elevation

- **Spacing:** 4 / 8 / 12 / 16 / 24 (`1 / 2 / 3 / 4 / 6`). Inside a row 8; between rows
  12; card padding 16; between sections 24.
- **Radius** (from `--radius: 0.5rem`): badges/pills `rounded-full`; inputs/small
  controls `rounded-md` (6 px); cards and list items `rounded-lg` (8 px); dialogs and
  the Memory Center panes `rounded-xl` (12 px). Never mix within one component.
- **Elevation:** two levels only, theme-aware CSS variables.
  - `shadow-elevation-1` (`--elevation-1`) — resting card / chip. Light: a faint
    foreground-tinted 1–2 px shadow (picks up each theme's hue). Dark: an inset 1 px
    top highlight + tight black shadow (Linear-style lift, no grey haze).
  - `shadow-elevation-2` (`--elevation-2`) — popovers, sheets, the detail panel.
  - Flat lists use hairlines (`border-border/50`), not shadows.

## Motion

| Duration | Use |
|---|---|
| 120 ms | hover / press, color & border changes |
| 180 ms | enter of chips, rows, popovers (opacity 0→1 + `y: 4→0`) |
| 240 ms | progress fill glide, panel/sheet enter, expand/collapse |

Ease-out (`[0.16, 1, 0.3, 1]` in `motion/react`, `ease-out` in CSS) for enters; exits
are ~70 % of the enter duration with ease-in. Animate `transform`/`opacity` only. Lists
stagger 30 ms per item, max 8 items. Under reduced motion: no translate, no shimmer, no
sweep — fades ≤120 ms or instant (`motion-safe:` / `motion-reduce:` variants;
`useReducedMotion()` in `motion/react`).

## State patterns

- **Loading — skeleton.** `MemorySkeleton` blocks sized to the real content (title line
  `h-3 w-2/3`, body `h-3 w-full`, meta `h-2.5 w-24`). Faint sheen (`--foreground` at 5 %)
  sweeping every 1.6 s; static under reduced motion. Mark the container `aria-busy`.
  Show skeletons only after ~150 ms to avoid flashes; never a centered spinner for lists.
- **Empty.** Centered in the pane: a 20 px lucide icon in `text-muted-foreground/60`,
  one 13 px/500 line saying what's missing ("还没有记忆"), one 12 px muted line with
  why/what next, at most one secondary button. No illustrations.
- **Error.** Inline callout, not a toast: `bg-danger-bg`, 12 px `text-danger` icon +
  short cause ("索引失败：磁盘空间不足"), and a concrete action button (重试 / 打开设置).
  Keep the stale data visible underneath when there is any.
- **Progress.** `ProgressBar` — 4 px hairline, `--progress-track` / `--progress-fill`.
  Determinate glides 240 ms; indeterminate (unknown total) sweeps; label it for screen
  readers ("下载记忆模型") and show the numbers as text next to it (`12.4 / 90 MB`).
- **Stats.** `StatPill` — muted label, toned value (`neutral | success | warning |
  danger` on the existing semantic tokens). A row of pills should read as data.

## Kind colors

Six kinds, each with an ink (`--memory-kind-<k>`) and a tinted surface
(`--memory-kind-<k>-bg`), defined in `ui/src/styles/globals.css` for `:root`, `.dark`
and all 10 named themes; mapped in `tailwind.config.js` as `bg-memory-<k>`,
`text-memory-<k>`, `bg-memory-<k>-bg`, `border-memory-<k>/20`.

| Kind | 中文 | Base hue | Rationale |
|---|---|---|---|
| fact | 事实 | 215 slate-blue | steady, neutral knowledge |
| preference | 偏好 | 340 rose | personal, warm |
| episode | 事件 | 32 ochre | time, lived moments |
| lesson | 经验 | 150 sage | growth |
| entity | 实体 | 262 violet | people, things — the "who" |
| procedure | 技能 | 188 teal | doing, skills |

Rules: **restrained chroma** (28–45 % saturation; 55 % only on the high-contrast black /
the-finals themes) — premium means quiet. Each theme pulls the hues ≤10° toward its own
hue (ocean → blue, forest/qingye → green, slate/warm-paper → earth, the-finals → warm)
and sets its own saturation/lightness, so kinds harmonize instead of looking pasted on.
Badge text is solved per theme for **≥ 4.5:1 on its `-bg` and on the theme card**
(guarded by `ui/src/styles/memory-tokens.test.ts`, which also requires progress
fill ≥ 3:1 on track). Color is never the only signal: badges always carry the label.
Components never use raw palette classes (`blue-500` …) — only these tokens or the
existing semantic ones (`success`, `warning`, `danger`, `muted`).

## Primitives (`ui/src/components/memory/primitives/`)

`<KindBadge kind variant="label|dot" size="sm|md" />`,
`<ProgressBar value={0..1} indeterminate? label? />`,
`<StatPill label value tone />`, `<MemorySkeleton className="h-3 w-24" />`,
plus `MEMORY_KINDS` / `MEMORY_KIND_LABELS` / `MEMORY_KIND_CLASSES`.

## Copy tone

Chinese, short, plain. Primary text names the thing the user cares about ("记住了你偏好
简洁回答"), not the mechanism — no "embedding", "FTS", "bucket", "vector" in primary
text; put technical detail in secondary/tooltip text or 高级. Status in 2–6 characters
(已就绪 / 下载中 / 未启用). Errors: cause + next step. No exclamation marks, no emoji.
