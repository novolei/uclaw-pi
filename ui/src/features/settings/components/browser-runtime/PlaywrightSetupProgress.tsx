import * as React from 'react'
import { Download, Terminal } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type { PlaywrightSetupExecutionReport } from '@/lib/tauri-bridge'
import { SettingsCard, SettingsRow, SettingsSection } from '../primitives'

interface PlaywrightSetupProgressProps {
  statusLabel: string
  detailLabel: string
  needsNode: boolean
  canAutoSetup: boolean
  pending: boolean
  report?: PlaywrightSetupExecutionReport
  onRunSetup: () => void
}

export function PlaywrightSetupProgress({
  statusLabel,
  detailLabel,
  needsNode,
  canAutoSetup,
  pending,
  report,
  onRunSetup,
}: PlaywrightSetupProgressProps): React.ReactElement {
  const reportSummary = report
    ? `Last setup ${report.status}; ${report.stepReports.length} step(s).`
    : detailLabel

  return (
    <SettingsSection title="Playwright Setup">
      <SettingsCard>
        <SettingsRow
          label="Official tooling"
          icon={<Download size={16} />}
          description={reportSummary}
        >
          <Badge variant={statusLabel === 'Ready' ? 'default' : 'outline'}>
            {pending ? 'Running' : statusLabel}
          </Badge>
        </SettingsRow>
        {needsNode ? (
          <SettingsRow
            label="Node.js"
            icon={<Terminal size={16} />}
            description="uClaw will not run sudo. Install Node.js/npm/npx in Terminal, then return here."
          />
        ) : (
          <SettingsRow
            label="Setup command"
            description="Runs the official global Playwright CLI setup, refreshes built-in skills, and probes the built-in MCP server."
          >
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={!canAutoSetup || pending}
              onClick={onRunSetup}
            >
              <Download />
              {pending ? 'Setting up' : 'Set up'}
            </Button>
          </SettingsRow>
        )}
      </SettingsCard>
    </SettingsSection>
  )
}
