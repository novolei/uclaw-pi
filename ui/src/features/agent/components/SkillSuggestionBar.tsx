/**
 * SkillSuggestionBar — 聊天输入框下方的技能建议 chip 条。
 *
 * 监听输入文本变化，debounce 500ms 后在本地搜索已有技能
 * (listSkills + listLearnedSkills)，按名称/描述/场景模糊匹配，
 * 显示 top-3 命中结果。点击 chip 触发 onSkillSelect('/<name>')。
 *
 * Phase 4 (G9): 让用户在打字时发现可用技能。
 */
import * as React from 'react'
import { Sparkles } from 'lucide-react'
import { cn } from '@/lib/utils'
import { useSkillSuggestions } from '../hooks/useSkillSuggestions'

interface SkillSuggestionBarProps {
  inputText: string
  onSkillSelect: (skillName: string) => void
}

export function SkillSuggestionBar({ inputText, onSkillSelect }: SkillSuggestionBarProps): React.ReactElement | null {
  // Debounced local skill search (catalog load + cache + matching in the hook).
  const suggestions = useSkillSuggestions(inputText)

  if (suggestions.length === 0) return null

  return (
    <div className="flex items-center gap-1.5 px-4 pb-1.5">
      <Sparkles className="size-3 text-muted-foreground shrink-0" />
      {suggestions.map((s) => (
        <button
          key={`${s.provenance}-${s.name}`}
          type="button"
          onClick={() => onSkillSelect(`/${s.name}`)}
          className={cn(
            'inline-flex items-center gap-1 px-2 py-0.5 rounded-full',
            'text-[10.5px] leading-tight',
            'bg-accent/10 text-accent-foreground border border-accent/25',
            'hover:bg-accent/20 hover:border-accent/40',
            'transition-colors truncate max-w-[200px]',
          )}
        >
          <span className="truncate">{s.name}</span>
          {s.description && (
            <span className="text-muted-foreground truncate max-w-[100px]">
              · {s.description}
            </span>
          )}
        </button>
      ))}
    </div>
  )
}
