// Owns the AboutSettings data: the app version + platform info, loaded once on
// mount. Extracted out of the component during the features/settings migration
// (code-organization ADR 2026-05-31). The typed `@/lib/tauri-bridge`
// getVersion/getPlatform helpers stay in the hook (precedent: useWorkspaceSkillTags
// keeps its typed IPC in the hook too). Behavior preserved verbatim — the fire-and-
// forget loads with no error handling, matching the original useEffect.
import { useState, useEffect } from 'react'
import { getVersion, getPlatform } from '@/lib/tauri-bridge'
import type { VersionInfo, PlatformInfo } from '@/lib/types'

export function useAboutInfo() {
  const [version, setVersion] = useState<VersionInfo | null>(null)
  const [platform, setPlatform] = useState<PlatformInfo | null>(null)

  useEffect(() => {
    getVersion().then(setVersion)
    getPlatform().then(setPlatform)
  }, [])

  return { version, platform }
}
