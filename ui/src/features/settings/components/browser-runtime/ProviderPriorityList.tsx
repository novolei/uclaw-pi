import * as React from 'react'
import { Bug, Download, Power, RotateCcw, Settings2 } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type {
  BrowserRuntimeProviderId,
} from '@/lib/startup/startup-doctor'
import type {
  BrowserRuntimeProviderRowViewModel,
} from '@/lib/browser-runtime/browser-runtime-control-center'
import { SettingsCard, SettingsSection } from '../primitives'

interface ProviderPriorityListProps {
  rows: BrowserRuntimeProviderRowViewModel[]
  priority: BrowserRuntimeProviderId[]
  pendingAction: string | null
  probePendingProviderId: BrowserRuntimeProviderId | null
  disabled: boolean
  onEnable: (providerId: BrowserRuntimeProviderId) => void
  onSetFirst: (providerId: BrowserRuntimeProviderId, priority: BrowserRuntimeProviderId[]) => void
  onRunProbe: (providerId: BrowserRuntimeProviderId) => void
  onRunSetup: () => void
  onConfigureMcp: () => void
}

export function ProviderPriorityList({
  rows,
  priority,
  pendingAction,
  probePendingProviderId,
  disabled,
  onEnable,
  onSetFirst,
  onRunProbe,
  onRunSetup,
  onConfigureMcp,
}: ProviderPriorityListProps): React.ReactElement {
  return (
    <SettingsSection title="Provider Priority">
      <SettingsCard divided={false}>
        <div className="divide-y divide-border">
          {rows.length > 0 ? (
            rows.map((row) => (
              <ProviderPriorityRow
                key={row.lane.providerId}
                row={row}
                priority={priority}
                pendingAction={pendingAction}
                probePendingProviderId={probePendingProviderId}
                disabled={disabled}
                onEnable={onEnable}
                onSetFirst={onSetFirst}
                onRunProbe={onRunProbe}
                onRunSetup={onRunSetup}
                onConfigureMcp={onConfigureMcp}
              />
            ))
          ) : (
            <div className="p-4 text-sm text-muted-foreground">
              等待 Rust Browser Automation 报告。
            </div>
          )}
        </div>
      </SettingsCard>
    </SettingsSection>
  )
}

interface ProviderPriorityRowProps {
  row: BrowserRuntimeProviderRowViewModel
  priority: BrowserRuntimeProviderId[]
  pendingAction: string | null
  probePendingProviderId: BrowserRuntimeProviderId | null
  disabled: boolean
  onEnable: (providerId: BrowserRuntimeProviderId) => void
  onSetFirst: (providerId: BrowserRuntimeProviderId, priority: BrowserRuntimeProviderId[]) => void
  onRunProbe: (providerId: BrowserRuntimeProviderId) => void
  onRunSetup: () => void
  onConfigureMcp: () => void
}

function ProviderPriorityRow({
  row,
  priority,
  pendingAction,
  probePendingProviderId,
  disabled,
  onEnable,
  onSetFirst,
  onRunProbe,
  onRunSetup,
  onConfigureMcp,
}: ProviderPriorityRowProps): React.ReactElement {
  const enablePending = pendingAction === `enable:${row.lane.providerId}`
  const firstPending = pendingAction === `first:${row.lane.providerId}`
  const setupPending = pendingAction === 'setup:auto'
  const probePending = probePendingProviderId === row.lane.providerId

  return (
    <div className="grid gap-3 p-4 md:grid-cols-[minmax(0,1fr)_auto]">
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm font-medium">{row.lane.displayName}</span>
          <Badge variant={row.lane.routeRole === 'active' ? 'default' : 'outline'}>
            {row.statusLabel}
          </Badge>
          {row.isFirst ? <Badge variant="secondary">第一优先级</Badge> : null}
          {row.lane.providerId === 'browser.playwright_mcp' ? (
            <Badge variant="outline">Advanced backup</Badge>
          ) : null}
        </div>
        <div className="mt-1 text-xs text-muted-foreground">
          #{row.lane.priorityRank} · readiness {row.lane.readiness} · probe {row.lane.probeState}
        </div>
        {row.routeHintLabel ? (
          <div className="mt-1 text-xs text-muted-foreground">
            {row.routeHintLabel}
          </div>
        ) : null}
      </div>
      <div className="flex min-h-11 flex-wrap items-center gap-2 md:justify-end">
        {row.configureMcpClickable ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            aria-label="Configure Playwright MCP"
            onClick={onConfigureMcp}
          >
            <Settings2 />
            Configure MCP
          </Button>
        ) : null}
        {row.canEnable ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={disabled || enablePending}
            onClick={() => onEnable(row.lane.providerId)}
          >
            <Power />
            {enablePending ? '启用中' : row.nextActionLabel}
          </Button>
        ) : row.canRunPlaywrightSetup ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={disabled || setupPending}
            onClick={onRunSetup}
          >
            <Download />
            {setupPending ? 'Setting up' : row.nextActionLabel}
          </Button>
        ) : row.canRunProbe ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={disabled || probePending}
            aria-label={`Run ${row.lane.displayName} probe`}
            onClick={() => onRunProbe(row.lane.providerId)}
          >
            <Bug />
            {probePending ? 'Running probe' : 'Run probe'}
          </Button>
        ) : (
          <Button type="button" variant="outline" size="sm" disabled>
            {row.nextActionLabel}
          </Button>
        )}
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={disabled || row.isFirst || firstPending}
          onClick={() => onSetFirst(row.lane.providerId, priority)}
        >
          <RotateCcw />
          {firstPending ? '更新中' : '设为第一'}
        </Button>
      </div>
    </div>
  )
}
