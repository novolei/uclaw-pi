import { useAtomValue } from 'jotai'
import { resolvedThemeAtom } from '@/atoms/theme'

/**
 * Map uClaw's resolved theme ('light' | 'dark') to Pierre's theme prop name.
 * Pierre ships 'one-light' and 'one-dark-pro' out of the box (Shiki themes).
 * Per-uClaw-theme Pierre customization (warm-paper / qingye / etc.) is
 * deferred to Phase 2 per spec.
 */
export function usePierreTheme(): 'one-light' | 'one-dark-pro' {
  const theme = useAtomValue(resolvedThemeAtom)
  return theme === 'dark' ? 'one-dark-pro' : 'one-light'
}

/**
 * Infer Shiki/Pierre language identifier from a file path's extension.
 * Falls back to 'text' for unknown / extensionless paths.
 */
export function detectLang(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase() ?? ''
  const map: Record<string, string> = {
    ts: 'typescript', tsx: 'tsx',
    js: 'javascript', jsx: 'jsx',
    json: 'json', md: 'markdown', mdx: 'markdown',
    py: 'python', rs: 'rust', go: 'go',
    java: 'java', kt: 'kotlin', swift: 'swift',
    rb: 'ruby', php: 'php', sh: 'shell', bash: 'shell',
    yaml: 'yaml', yml: 'yaml', toml: 'toml',
    html: 'html', css: 'css', scss: 'scss', sass: 'sass',
    sql: 'sql', xml: 'xml', svg: 'xml',
    dockerfile: 'docker', dockerignore: 'text',
    c: 'c', h: 'c', cpp: 'cpp', hpp: 'cpp', cc: 'cpp',
    cs: 'csharp', vue: 'vue', svelte: 'svelte',
    lua: 'lua', r: 'r', dart: 'dart', zig: 'zig',
  }
  return map[ext] ?? 'text'
}
