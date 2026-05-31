/**
 * StrategyPresetSelector — dropdown that biases the skill manifest re-ranker.
 *
 * Presets:
 *   🎯 平衡   (balanced) — default, no category bias
 *   🔧 修 bug (repair)   — surfaces repair-tagged learned skills first
 *   ⚡ 优化   (optimize) — surfaces optimize-tagged learned skills first
 *   🔭 探索   (innovate) — surfaces innovate-tagged learned skills first
 *
 * State is persisted in `agentSessionStrategyMapAtom` (Map<sessionId, AgentStrategy>).
 * The selected value is forwarded in the `strategy` field of sendAgentMessage.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { agentSessionStrategyMapAtom, type AgentStrategy } from '@/atoms/agent-atoms'

interface Preset {
  value: AgentStrategy
  label: string
  description: string
}

const PRESETS: Preset[] = [
  { value: 'balanced',  label: '🎯 平衡',   description: '默认排序，不偏向特定类别' },
  { value: 'repair',    label: '🔧 修 bug',  description: '优先显示 repair 类技能' },
  { value: 'optimize',  label: '⚡ 优化',    description: '优先显示 optimize 类技能' },
  { value: 'innovate',  label: '🔭 探索',    description: '优先显示 innovate 类技能' },
]

interface StrategyPresetSelectorProps {
  sessionId: string
}

export function StrategyPresetSelector({ sessionId }: StrategyPresetSelectorProps): React.ReactElement {
  const [strategyMap, setStrategyMap] = useAtom(agentSessionStrategyMapAtom)
  const currentStrategy: AgentStrategy = strategyMap.get(sessionId) ?? 'balanced'

  const current = PRESETS.find((p) => p.value === currentStrategy) ?? PRESETS[0]!

  const handleSelect = React.useCallback((value: AgentStrategy) => {
    setStrategyMap((prev) => {
      const next = new Map(prev)
      if (value === 'balanced') {
        next.delete(sessionId)
      } else {
        next.set(sessionId, value)
      }
      return next
    })
  }, [sessionId, setStrategyMap])

  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-[36px] rounded-full px-2.5 text-xs font-normal text-foreground/60 hover:text-foreground"
            >
              {current.label}
            </Button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent side="top">
          <p>技能优先策略：{current.description}</p>
        </TooltipContent>
      </Tooltip>
      <DropdownMenuContent side="top" align="start" sideOffset={6} className="w-44">
        {PRESETS.map((preset) => (
          <DropdownMenuItem
            key={preset.value}
            onSelect={() => handleSelect(preset.value)}
            className="flex items-center justify-between gap-2"
          >
            <span className="text-sm">{preset.label}</span>
            {preset.value === currentStrategy && (
              <span className="ml-auto text-xs text-primary">✓</span>
            )}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
