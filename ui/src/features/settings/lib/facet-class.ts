// Facet-class taxonomy + state-badge styling for the LearnedProfileTab. Extracted
// out of legacy settings/LearnedProfileTab.tsx during the features/settings
// migration (code-organization ADR 2026-05-31). Pure constants/helpers — no React.
// Behavior preserved verbatim.

/** Render order matches the Rust `CLASS_RENDER_ORDER` in
 *  `learning::prompt_section`. Stable ordering means the user always
 *  sees the same sections in the same place even when an earlier
 *  class is empty (it shows "(none yet)" instead of disappearing). */
export const CLASS_RENDER_ORDER: ReadonlyArray<string> = [
  'identity',
  'style',
  'tooling',
  'veto',
  'goal',
  'channel',
]

export const CLASS_LABEL: Record<string, string> = {
  identity: '身份 (Identity)',
  style: '风格 (Style)',
  tooling: '工具 (Tooling)',
  veto: '禁忌 (Veto)',
  goal: '目标 (Goal)',
  channel: '渠道 (Channel)',
}

export const CLASS_DESCRIPTION: Record<string, string> = {
  identity: '你是谁 — 名字、职位、角色',
  style: '语言、长度、语气偏好',
  tooling: '常用工具、库、编辑器',
  veto: '不要做的事、不要用的工具',
  goal: '当前在做的事 / 关心的项目',
  channel: '消息渠道偏好（IM、邮件等）',
}

export function stateBadgeTone(state: string): string {
  switch (state.toLowerCase()) {
    case 'active':
      return 'bg-green-500/15 text-green-700 dark:text-green-300 border-green-500/30'
    case 'provisional':
      return 'bg-amber-500/15 text-amber-700 dark:text-amber-300 border-amber-500/30'
    case 'candidate':
      return 'bg-muted/40 text-muted-foreground border-border/50'
    case 'forgotten':
      return 'bg-muted/20 text-muted-foreground/60 border-border/30 line-through'
    default:
      return 'bg-muted/40 text-muted-foreground border-border/50'
  }
}
