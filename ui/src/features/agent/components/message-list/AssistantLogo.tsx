/**
 * AssistantLogo — the assistant avatar (provider logo or Bot fallback). Shared
 * between AgentMessageItem and the streaming bubble; extracted from
 * AgentMessages.tsx during the features/agent migration split. Unchanged.
 */

import * as React from 'react'
import { Bot } from 'lucide-react'
import { getModelLogo } from '@/lib/model-logo'

export function AssistantLogo({ model }: { model?: string }): React.ReactElement {
  // getModelLogo returns '' for unknown providers — fall back to Bot icon so we
  // never render <img src=""> (broken image).
  const logoUrl = model ? getModelLogo(model) : ''
  if (logoUrl) {
    return (
      <img
        src={logoUrl}
        alt={model}
        className="size-[35px] rounded-[25%] object-cover bg-muted/30"
      />
    )
  }
  return (
    <div className="size-[35px] rounded-[25%] bg-primary/10 flex items-center justify-center">
      <Bot size={18} className="text-primary" />
    </div>
  )
}
