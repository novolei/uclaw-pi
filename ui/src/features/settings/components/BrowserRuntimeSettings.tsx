// BrowserRuntimeSettings — Settings → Browser Runtime tab. Thin shell: composes
// the browser-runtime presentation sub-components + the identity section; all
// state + side effects live in `useBrowserRuntimeSettings`; all IPC goes through
// `settingsBridge`. Pure label/badge formatting lives in
// `lib/browser-runtime-format`. Migrated + split out of
// `components/settings/BrowserRuntimeSettings.tsx` (607 lines) during P3.
import * as React from 'react'
import { Activity } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { deriveBrowserRuntimeControlCenterViewModel } from '@/lib/browser-runtime/browser-runtime-control-center'
import {
  deriveBrowserRuntimeSettingsViewModel,
  type BrowserRuntimeSettingsInput,
} from '@/lib/browser-runtime/browser-runtime-settings'
import { BrowserAutomationDiagnostics } from '@/components/settings/browser-runtime/BrowserAutomationDiagnostics'
import { BrowserAutomationHeader } from '@/components/settings/browser-runtime/BrowserAutomationHeader'
import { PlaywrightSetupProgress } from '@/components/settings/browser-runtime/PlaywrightSetupProgress'
import { PlaywrightSkillsPanel } from '@/components/settings/browser-runtime/PlaywrightSkillsPanel'
import { ProviderPriorityList } from '@/components/settings/browser-runtime/ProviderPriorityList'
import {
  SettingsCard,
  SettingsRow,
  SettingsSection,
  SettingsToggle,
} from '@/components/settings/primitives'
import { BrowserIdentitySection } from './browser-runtime/BrowserIdentitySection'
import { badgeVariant } from '../lib/browser-runtime-format'
import { useBrowserRuntimeSettings } from '../hooks/useBrowserRuntimeSettings'

interface BrowserRuntimeSettingsProps {
  status?: BrowserRuntimeSettingsInput
}

export function BrowserRuntimeSettings({
  status,
}: BrowserRuntimeSettingsProps): React.ReactElement {
  const {
    liveStatus,
    identityStatus,
    revokingProfileId,
    controlCenter,
    controlCenterError,
    controlCenterPendingAction,
    probePendingProviderId,
    setupReport,
    rawReportOpen,
    setRawReportOpen,
    refreshControlCenter,
    enableProvider,
    setProviderFirst,
    runProbe,
    runSetup,
    setRawMcpToolsExposed,
    openPlaywrightMcpIntegration,
    revokeIdentity,
  } = useBrowserRuntimeSettings(status)

  const model = deriveBrowserRuntimeSettingsViewModel(status ?? liveStatus)
  const activeControlCenter = controlCenter ?? status?.report?.controlCenter ?? liveStatus?.report?.controlCenter
  const controlModel = deriveBrowserRuntimeControlCenterViewModel(activeControlCenter)

  return (
    <div className="space-y-8">
      <BrowserAutomationHeader
        desiredLabel={controlModel.routeSummary.desiredLabel}
        activeLabel={controlModel.routeSummary.activeLabel}
        reasonLabel={controlModel.routeSummary.reasonLabel}
        primaryActionLabel={controlModel.routeSummary.primaryActionLabel}
        error={controlCenterError}
        disabled={Boolean(status)}
        onRefresh={() => {
          void refreshControlCenter()
        }}
      />

      <ProviderPriorityList
        rows={controlModel.providerRows}
        priority={activeControlCenter?.desiredProviderPriority ?? []}
        pendingAction={controlCenterPendingAction}
        probePendingProviderId={probePendingProviderId}
        disabled={Boolean(status)}
        onEnable={enableProvider}
        onSetFirst={setProviderFirst}
        onRunProbe={runProbe}
        onRunSetup={() => {
          void runSetup()
        }}
        onConfigureMcp={openPlaywrightMcpIntegration}
      />

      <PlaywrightSetupProgress
        statusLabel={controlModel.setupSummary.statusLabel}
        detailLabel={controlModel.setupSummary.detailLabel}
        needsNode={controlModel.setupSummary.needsNode}
        canAutoSetup={controlModel.setupSummary.canAutoSetup}
        pending={controlCenterPendingAction === 'setup:auto'}
        report={setupReport}
        onRunSetup={() => {
          void runSetup()
        }}
      />

      <PlaywrightSkillsPanel enabled={controlModel.setupSummary.statusLabel === 'Ready'} />

      <SettingsSection title="开发者 Guardrails" description="Advanced Browser Runtime controls">
        <SettingsCard>
          <SettingsToggle
            label="Expose raw Playwright MCP tools"
            description="默认关闭。开启后只把 uClaw allowlist 内的 Playwright MCP 原始工具暴露给 LLM；普通浏览器动作仍优先走 Browser Runtime Adapter。"
            checked={Boolean(activeControlCenter?.mcpIntegrationSummary.rawToolsExposed)}
            disabled={Boolean(status) || controlCenterPendingAction === 'mcp:raw-tools'}
            onCheckedChange={(checked) => {
              void setRawMcpToolsExposed(checked)
            }}
          />
        </SettingsCard>
      </SettingsSection>

      <BrowserAutomationDiagnostics
        report={activeControlCenter}
        model={controlModel}
        rawOpen={rawReportOpen}
        onToggleRaw={() => setRawReportOpen((open) => !open)}
      />

      <SettingsSection title="运行时 Supervisor" description="Rust Browser Runtime Supervisor">
        <SettingsCard>
          <SettingsRow
            label="Supervisor"
            icon={<Activity size={16} />}
            description={model.supervisorDetailLabel}
          >
            <Badge variant={badgeVariant(model.supervisorStatusKind)}>
              {model.supervisorStateLabel}
            </Badge>
          </SettingsRow>
          <SettingsRow label="Provider" description={model.supervisorProviderLabel} />
          <SettingsRow label="Doctor" description={model.supervisorDoctorLabel} />
          <SettingsRow label="活跃上下文" description={model.supervisorActiveContextsLabel} />
          <SettingsRow label="Local Chromium" description={model.localProviderLabel} />
          <SettingsRow label="Playwright CLI" description={model.playwrightCliProviderLabel} />
          <SettingsRow label="Playwright MCP" description={model.playwrightMcpProviderLabel} />
        </SettingsCard>
      </SettingsSection>

      <BrowserIdentitySection
        identityStatus={identityStatus}
        revokingProfileId={revokingProfileId}
        onRevoke={(profileId) => {
          void revokeIdentity(profileId)
        }}
      />
    </div>
  )
}
