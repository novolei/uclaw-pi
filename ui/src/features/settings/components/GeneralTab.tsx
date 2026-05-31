/**
 * GeneralTab — composes GeneralSettings + PromptSettings + AppearanceSettings.
 */
import * as React from 'react'
// Sub-sections still live under components/settings/ (migrated in a later phase);
// imported via the @/ alias so behavior is preserved exactly after the P2 move.
import { GeneralSettings } from '@/components/settings/GeneralSettings'
import { PromptSettings } from '@/components/settings/PromptSettings'
import { AppearanceSettings } from '@/components/settings/AppearanceSettings'

export function GeneralTab(): React.ReactElement {
  return (
    <div className="space-y-8">
      <section data-settings-section="通用偏好">
        <GeneralSettings />
      </section>
      <section data-settings-section="系统提示词">
        <PromptSettings />
      </section>
      <section data-settings-section="主题与字体">
        <AppearanceSettings />
      </section>
    </div>
  )
}
