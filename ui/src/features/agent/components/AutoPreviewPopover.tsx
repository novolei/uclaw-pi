/**
 * AutoPreviewPopover — toggle for "auto-open preview when agent writes a file".
 *
 * Ported from Proma's auto-preview feature but scoped to Agent mode (Chat
 * mode has no tool calls). Mount in the AgentView composer footer.
 *
 * Two visual states: Eye (enabled) and EyeOff (disabled). The popover body
 * holds a labeled Switch + a one-line description so the user understands
 * what the toggle actually does.
 */

import * as React from 'react'
import { useAtom } from 'jotai'
import { Eye, EyeOff } from 'lucide-react'
import { autoPreviewEnabledAtom } from '@/atoms/preview-panel-atoms'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Switch } from '@/components/ui/switch'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'

export function AutoPreviewPopover(): React.ReactElement {
  const [enabled, setEnabled] = useAtom(autoPreviewEnabledAtom)
  const Icon = enabled ? Eye : EyeOff

  return (
    <Popover>
      <Tooltip>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="size-[36px] rounded-full text-foreground/60 hover:text-foreground"
              aria-label={enabled ? '自动预览：已开启' : '自动预览：已关闭'}
            >
              <Icon className="size-4" />
            </Button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent side="top">
          <p>{enabled ? '自动预览已开启' : '自动预览已关闭'}</p>
        </TooltipContent>
      </Tooltip>
      <PopoverContent align="end" className="w-[280px] p-0">
        <div className="px-3 py-2 border-b border-border/60">
          <div className="text-[11px] font-semibold uppercase tracking-widest text-muted-foreground/70">
            自动预览
          </div>
        </div>
        <div className="px-3 py-3 space-y-2">
          <div className="flex items-center justify-between gap-3">
            <label
              htmlFor="auto-preview-toggle"
              className="text-[13px] text-foreground/90 cursor-pointer select-none"
            >
              写入文件时自动打开预览
            </label>
            <Switch
              id="auto-preview-toggle"
              checked={enabled}
              onCheckedChange={setEnabled}
            />
          </div>
          <p className="text-[11px] leading-relaxed text-muted-foreground">
            Agent 调用 write_file / edit 等写工具时，预览面板会自动打开并定位到目标文件。
            手动关闭后，本轮不再弹出；下一轮用户消息会重新启用。
          </p>
        </div>
      </PopoverContent>
    </Popover>
  )
}
