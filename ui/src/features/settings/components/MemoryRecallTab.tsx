/**
 * MemoryRecallTab — Settings › 记忆
 *
 * The local retrieval engine card on top (models toggle, download progress,
 * index status); the recall tuning form lives under a collapsible 高级,
 * collapsed by default (it mounts — and loads its config — only when opened).
 */
import * as React from 'react'
import { ChevronRight } from 'lucide-react'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { MemoryRecallSettings } from './MemoryRecallSettings'
import { MemoryEngineCard } from './memory/MemoryEngineCard'

export function MemoryRecallTab(): React.ReactElement {
  const [advancedOpen, setAdvancedOpen] = React.useState(false)
  return (
    <div className="space-y-6">
      <section data-settings-section="本地检索引擎">
        <MemoryEngineCard />
      </section>

      <section data-settings-section="记忆召回配置">
        <Collapsible open={advancedOpen} onOpenChange={setAdvancedOpen}>
          <CollapsibleTrigger
            className={
              'group -mx-2 flex w-[calc(100%+1rem)] items-center gap-1.5 rounded-md px-2 py-1.5 text-left ' +
              'text-[13px] font-medium leading-5 text-muted-foreground transition-colors duration-[120ms] ' +
              'hover:bg-muted/50 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring'
            }
          >
            <ChevronRight
              aria-hidden
              size={14}
              className="shrink-0 transition-transform duration-[180ms] ease-out group-data-[state=open]:rotate-90 motion-reduce:transition-none"
            />
            <span className="text-foreground">高级</span>
            <span className="truncate text-[12px] font-normal text-muted-foreground">
              召回数量、融合策略与阈值
            </span>
          </CollapsibleTrigger>
          <CollapsibleContent className="pt-3">
            <MemoryRecallSettings />
          </CollapsibleContent>
        </Collapsible>
      </section>
    </div>
  )
}
