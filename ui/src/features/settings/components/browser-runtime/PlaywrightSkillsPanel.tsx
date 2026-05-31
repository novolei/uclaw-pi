import * as React from 'react'
import { Sparkles } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { SettingsCard, SettingsRow, SettingsSection } from '../primitives'

interface PlaywrightSkillsPanelProps {
  enabled: boolean
}

export function PlaywrightSkillsPanel({
  enabled,
}: PlaywrightSkillsPanelProps): React.ReactElement {
  return (
    <SettingsSection title="Built-in Playwright Skills">
      <SettingsCard>
        <SettingsRow
          label="Agent discovery"
          icon={<Sparkles size={16} />}
          description="uClaw Agent can discover Playwright CLI skills, while browser actions still route through the Browser Runtime Adapter."
        >
          <Badge variant={enabled ? 'default' : 'secondary'}>
            {enabled ? 'Enabled' : 'Waiting'}
          </Badge>
        </SettingsRow>
        <SettingsRow
          label="Execution guardrail"
          description="Skills describe browser automation intent; they do not grant arbitrary Playwright shell execution."
        />
      </SettingsCard>
    </SettingsSection>
  )
}
