# Skill Marketplace — Popularity Sort + Result Card Redesign

Date: 2026-06-02
Status: Approved (brainstorming → implementation)

## Problem

When the agent searches the skill marketplace (`skill_marketplace_search`,
e.g. for "ppt"), the returned skills are **not ordered by popularity** — the
results surface obscure, near-zero-install skills first. Separately, the
in-chat result card is visually flat: a minimal bordered row list with two
ambiguous install buttons (`全局` / `本工作区`) and no ranking emphasis or
trust signals.

## Root cause (sort)

No sorting is applied anywhere along the path:

- **API call drops the sort param.** Both clients hit
  `/api/v1/skills/search?q={q}&limit={limit}` and nothing else, even though
  the APIs support ranking:
  - skillsmp supports `sortBy=stars|recent` (`skillsmp.rs`) — never sent.
  - skills.sh `list` supports `view=trending|hot|all-time`; its `search`
    endpoint has no documented sort param.
- **Backend tool** `to_result_json()` (`skill_marketplace.rs`) maps results in
  raw API order — no `.sort_by()`.
- **Card** (`skill-marketplace-search-result.tsx`) renders rows in received
  order — it even *displays* `installs` but never sorts by it.

Note: for skillsmp, `installs` is populated from the row's `stars` field (the
closest popularity analog). So sorting by `installs` descending is the unified
"popularity" order for both providers.

## Part A — Popularity sort (robust, two-layer)

1. **Request ranked results from the API where supported.**
   - skillsmp (`skillsmp.rs::search_inner`): append `&sortBy=stars` to the
     search URL.
   - skills.sh (`client.rs::search`): no documented search sort param — left
     unchanged; covered by layer 2.
2. **Defensive sort in the backend tool** (`skill_marketplace.rs`): sort the
   `Vec<SkillSummary>` by `installs` **descending** before mapping to JSON.
   This guarantees popularity order for both the LLM and the card regardless
   of what the API returns. A stable sort preserves the API's relevance order
   among equal-install ties.
3. **Frontend final fallback** (`skill-marketplace-search-result.tsx`): sort
   `rows` by `installs` desc before render — cheap, handles older/cached
   results that predate the backend fix.

## Part B — Card redesign

Rebuild `skill-marketplace-search-result.tsx`, reusing existing Radix
primitives in `ui/src/components/ui/` (`dropdown-menu`, `badge`, `tooltip`,
`spinner`, `button`). Design language from ui-ux-pro-max: Dark/OLED, Inter,
emerald CTA accent, WCAG-AA contrast, 150–300ms transitions.

Per-skill card:

- **Rank badge** — numbered chip (1, 2, 3…) on the left; #1 gets a faint
  emerald "top" accent.
- **Name** — `text-sm font-medium`, primary foreground, truncates with a
  full-text tooltip.
- **Trust row** — author/source + a formatted install count
  (`Intl.NumberFormat`, e.g. `12.4k`, tabular figures) shown as a muted metric
  pill with an icon, distinct from body text.
- **Description** — always visible, clamped to 2 lines (`line-clamp-2`)
  instead of single-line truncate.
- **Primary action** — one **Install** button (emerald, single CTA). Opens a
  dropdown with *Install globally* / *Install to this workspace* (latter
  disabled with a tooltip when no active workspace). Replaces the two
  ambiguous buttons.
- **States** — hover (border + subtle bg lift, transitioned), installing
  (spinner + disabled), installed (emerald check pill `已安装（全局/本工作区）`),
  error (inline red message).
- **Empty / error** states polished with an icon + helper text.

Accessibility: `aria-label`s on icon-only bits, visible focus rings,
`cursor-pointer`, respects `prefers-reduced-motion`. Bilingual labels kept
(全局 / 本工作区 / 已安装) matching the current convention.

## Out of scope / unchanged

- No new Tauri commands, no migrations.
- `installSkillFromMarketplace` bridge signature and tool result JSON shape
  unchanged.
- Audit badges (skills.sh §5) remain a deferred follow-up.

## Verification

- Backend: `cd src-tauri && cargo build` (errors only) + `cargo test --lib
  skill_marketplace skillsmp`.
- Frontend: `cd ui && npx tsc --noEmit`.

## Commits (bisectable)

1. `fix(skill_marketplace): rank search results by popularity (API sortBy + defensive install-desc sort)`
2. `feat(ui): redesign skill marketplace result card (rank, trust signals, install scope menu, states)`
