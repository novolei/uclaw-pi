// 系统诊断 tab — thin shell composing the system/ cards. All state + IPC live
// inside each card's hook (code-organization ADR 2026-05-31); this shell only
// owns the shared error banner and the layout. Split out of the legacy
// `legacy settings/SystemTab.tsx` (865 lines) during P1.
import * as React from 'react'
import { DiagnosticsCard } from './system/DiagnosticsCard'
import { ServicesCard } from './system/ServicesCard'
import { HttpApiToggleCard } from './system/HttpApiToggleCard'
import { EvalsCard } from './system/EvalsCard'
// Tunable sections (migrated into the feature in P3a). Feature-local imports.
import { EmbeddingEndpointSection } from './EmbeddingEndpointSection'
import { StreamSkillThresholdsSection } from './StreamSkillThresholdsSection'
import { FoldDeltaThresholdSection } from './FoldDeltaThresholdSection'
import { DeveloperOptionsSection } from './DeveloperOptionsSection'

export function SystemTab() {
  const [error, setError] = React.useState<string | null>(null)
  return (
    <div className="flex flex-col gap-4 p-4 max-w-2xl" data-settings-section="系统诊断">
      {error && (
        <div className="text-xs text-red-400 bg-red-400/10 rounded-lg px-3 py-2">{error}</div>
      )}
      <DiagnosticsCard onError={setError} />
      <HttpApiToggleCard onError={setError} />
      <EvalsCard onError={setError} />
      <ServicesCard onError={setError} />

      {/* Sprint 2.2 followon #4 — embedding endpoint configuration */}
      <EmbeddingEndpointSection />

      {/* Bundle 26-B / 26-D / 27-B — stream idle timeout + skill
          distillation thresholds, originally hardcoded, now user-tunable. */}
      <StreamSkillThresholdsSection />

      {/* Bundle 17-B — /compact fold delta threshold. */}
      <FoldDeltaThresholdSection />

      {/* Sprint 2.2 followon #4 — developer options (collapsed by default) */}
      <DeveloperOptionsSection />
    </div>
  )
}
