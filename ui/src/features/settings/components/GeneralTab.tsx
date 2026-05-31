/**
 * GeneralTab — composes GeneralSettings + PromptSettings + AppearanceSettings.
 */
import * as React from 'react'
// Sub-sections still live under components/settings/ (migrated in a later phase);
// imported via the @/ alias so behavior is preserved exactly after the P2 move.
// Migrated siblings (P3) — imported relatively (intra-feature wiring), not via the
// public barrel, to avoid a self-referential barrel import.
import { GeneralSettings } from './GeneralSettings'
import { PromptSettings } from './PromptSettings'
import { AppearanceSettings } from './AppearanceSettings'

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
