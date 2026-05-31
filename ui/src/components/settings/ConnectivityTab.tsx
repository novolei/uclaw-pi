/**
 * ConnectivityTab — composes ChannelSettings + UsageSettings as
 * vertically-stacked sub-sections within a single tab.
 *
 * Each child component already provides its own SettingsSection
 * headings, so this wrapper is pure composition + data-section
 * anchor markers for the breadcrumb's IntersectionObserver.
 */
import * as React from 'react'
// ChannelSettings migrated into the settings feature; consumed from the barrel.
import { ChannelSettings } from '@/features/settings'
import { UsageSettings } from './UsageSettings'

export function ConnectivityTab(): React.ReactElement {
  return (
    <div className="space-y-8">
      <section data-settings-section="服务商">
        <ChannelSettings />
      </section>
      <section data-settings-section="用量与预算">
        <UsageSettings />
      </section>
    </div>
  )
}
