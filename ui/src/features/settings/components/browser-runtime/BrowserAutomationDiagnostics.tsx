import * as React from 'react'
import { Bug } from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  artifactLabel,
  rawControlCenterJson,
  type BrowserRuntimeControlCenterViewModel,
} from '@/lib/browser-runtime/browser-runtime-control-center'
import type { BrowserRuntimeControlCenterReport } from '@/lib/startup/startup-doctor'
import { SettingsCard, SettingsRow, SettingsSection } from '../primitives'

interface BrowserAutomationDiagnosticsProps {
  report?: BrowserRuntimeControlCenterReport
  model: BrowserRuntimeControlCenterViewModel
  rawOpen: boolean
  onToggleRaw: () => void
}

export function BrowserAutomationDiagnostics({
  report,
  model,
  rawOpen,
  onToggleRaw,
}: BrowserAutomationDiagnosticsProps): React.ReactElement {
  return (
    <SettingsSection title="Diagnostics">
      <SettingsCard>
        <SettingsRow
          label="Route evidence"
          icon={<Bug size={16} />}
          description={model.routeSummary.reasonLabel}
        />
        <SettingsRow
          label="Probe artifacts"
          description={
            model.providerRows.length > 0
              ? model.providerRows
                  .map((row) => artifactLabel(row.lane.lastProbeArtifact))
                  .join(' · ')
              : 'No artifact yet'
          }
        />
        <SettingsRow
          label="Probe history"
          description={
            model.providerRows
              .map((row) => `${row.lane.displayName}: ${row.lane.probeHistory?.length ?? 0}`)
              .join(' · ') || 'No probe history yet'
          }
        />
        <div className="p-4">
          <Button
            type="button"
            variant="outline"
            size="sm"
            aria-label={rawOpen ? 'Hide raw Browser Runtime report' : 'Show raw Browser Runtime report'}
            onClick={onToggleRaw}
          >
            {rawOpen ? 'Hide raw report' : 'Show raw report'}
          </Button>
          {rawOpen ? (
            <pre className="mt-3 max-h-80 overflow-auto rounded-md bg-muted p-4 text-xs">
              {rawControlCenterJson(report)}
            </pre>
          ) : null}
        </div>
      </SettingsCard>
    </SettingsSection>
  )
}
