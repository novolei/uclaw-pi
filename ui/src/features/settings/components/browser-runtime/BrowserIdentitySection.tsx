// 浏览器身份 — managed browser-identity status + per-profile revoke + active-task
// rows. Split out of `BrowserRuntimeSettings` during the P3 move; presentation
// only (the revoke side effect lives in `useBrowserRuntimeSettings`). Never
// calls IPC directly.
import * as React from 'react'
import { KeyRound, LogOut } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { SettingsCard, SettingsRow, SettingsSection } from '@/components/settings/primitives'
import type { BrowserIdentityStatusReport } from '@/lib/tauri-bridge'
import {
  formatIdentityTimestamp,
  identityActiveTaskLabel,
  identityBadgeVariant,
  identityCountLabel,
  identityProfileStatusLabel,
  identityProviderLabel,
  identityScopeLabel,
  identityStatusDetail,
  identityStatusLabel,
  identityTaskStatusLabel,
  latestIdentityLastUsedLabel,
} from '../../lib/browser-runtime-format'

interface BrowserIdentitySectionProps {
  identityStatus: BrowserIdentityStatusReport | undefined
  revokingProfileId: string | null
  onRevoke: (profileId: string) => void
}

export function BrowserIdentitySection({
  identityStatus,
  revokingProfileId,
  onRevoke,
}: BrowserIdentitySectionProps): React.ReactElement {
  return (
    <SettingsSection title="浏览器身份" description="uClaw-managed browser identity">
      <SettingsCard>
        <SettingsRow
          label="状态"
          icon={<KeyRound size={16} />}
          description={identityStatusDetail(identityStatus)}
        >
          <Badge variant={identityBadgeVariant(identityStatus)}>
            {identityStatusLabel(identityStatus)}
          </Badge>
        </SettingsRow>
        <SettingsRow label="授权身份" description={identityCountLabel(identityStatus)} />
        <SettingsRow label="上次使用" description={latestIdentityLastUsedLabel(identityStatus)} />
        <SettingsRow label="活跃任务" description={identityActiveTaskLabel(identityStatus)} />
      </SettingsCard>

      {identityStatus?.profiles.length ? (
        <SettingsCard divided={false}>
          <div className="divide-y divide-border">
            {identityStatus.profiles.map((profile) => (
              <div
                key={profile.id}
                className="flex items-center justify-between gap-4 p-4"
              >
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium">{profile.label}</span>
                    <Badge variant={profile.revoked ? 'secondary' : 'outline'}>
                      {identityProfileStatusLabel(profile)}
                    </Badge>
                  </div>
                  <div className="mt-1 truncate text-xs text-muted-foreground">
                    {profile.originPattern} · {identityProviderLabel(profile.provider)} · {identityScopeLabel(profile.scope)}
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    上次使用 {formatIdentityTimestamp(profile.lastUsedAtMs)}
                  </div>
                </div>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={profile.revoked || revokingProfileId === profile.id}
                  aria-label={profile.revoked ? `已撤销 ${profile.label}` : `撤销 ${profile.label}`}
                  onClick={() => {
                    onRevoke(profile.id)
                  }}
                >
                  <LogOut />
                  {profile.revoked ? '已撤销' : '撤销'}
                </Button>
              </div>
            ))}
          </div>
        </SettingsCard>
      ) : null}

      {identityStatus?.activeTasks.length ? (
        <SettingsCard divided={false}>
          <div className="divide-y divide-border">
            {identityStatus.activeTasks.map((task) => (
              <div
                key={task.runId}
                className="grid gap-3 p-4 md:grid-cols-[minmax(0,1fr)_auto]"
              >
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="truncate text-sm font-medium">{task.task}</span>
                    <Badge variant={task.drainDeadlineMs ? 'secondary' : 'outline'}>
                      {identityTaskStatusLabel(task.status)}
                    </Badge>
                  </div>
                  <div className="mt-1 truncate text-xs text-muted-foreground">
                    {task.sessionId} · {task.runId}
                  </div>
                </div>
                <div className="text-left text-xs text-muted-foreground md:text-right">
                  <div>更新 {formatIdentityTimestamp(task.updatedAtMs)}</div>
                  {task.drainDeadlineMs ? (
                    <div>撤销 drain 至 {formatIdentityTimestamp(task.drainDeadlineMs)}</div>
                  ) : null}
                </div>
              </div>
            ))}
          </div>
        </SettingsCard>
      ) : null}
    </SettingsSection>
  )
}
