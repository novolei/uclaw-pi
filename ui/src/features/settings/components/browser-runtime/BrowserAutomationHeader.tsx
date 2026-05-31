import * as React from 'react'
import { Activity, RefreshCw } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { SettingsCard, SettingsRow, SettingsSection } from '../primitives'

interface BrowserAutomationHeaderProps {
  desiredLabel: string
  activeLabel: string
  reasonLabel: string
  primaryActionLabel: string
  error?: string
  disabled: boolean
  onRefresh: () => void
}

export function BrowserAutomationHeader({
  desiredLabel,
  activeLabel,
  reasonLabel,
  primaryActionLabel,
  error,
  disabled,
  onRefresh,
}: BrowserAutomationHeaderProps): React.ReactElement {
  return (
    <SettingsSection
      title="Browser Automation"
      description="Official Playwright CLI first, built-in Playwright MCP backup, Local Chromium fallback"
    >
      <SettingsCard>
        <SettingsRow
          label="Desired route"
          icon={<Activity size={16} />}
          description={desiredLabel}
        >
          <Badge variant="outline">{primaryActionLabel}</Badge>
        </SettingsRow>
        <SettingsRow label="Active route" description={reasonLabel}>
          <Badge variant={activeLabel === 'Local Chromium' ? 'secondary' : 'default'}>
            {activeLabel}
          </Badge>
        </SettingsRow>
        <SettingsRow
          label="Control state"
          description={error ?? '读取 Rust Browser Automation route evidence。'}
        >
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={disabled}
            onClick={onRefresh}
          >
            <RefreshCw />
            刷新
          </Button>
        </SettingsRow>
      </SettingsCard>
    </SettingsSection>
  )
}
