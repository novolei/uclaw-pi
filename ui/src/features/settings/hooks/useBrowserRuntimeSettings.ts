// Owns every BrowserRuntimeSettings side effect — live-status / control-center /
// identity polling, provider enable/priority, probe, Playwright setup, raw-MCP
// toggle, identity revocation, and the "open Playwright MCP integration" nav.
// Extracted out of the component during the P3 split; all IPC goes through
// `settingsBridge` (no `@tauri-apps/api` here). Behavior — generation guards,
// mounted guards, error-swallowing on manual refresh — preserved exactly.
import * as React from 'react'
import { useSetAtom } from 'jotai'
import { kaleidoscopeModuleAtom, selectedBuiltinIntegrationAtom } from '@/atoms/kaleidoscope'
import { topLevelViewAtom } from '@/atoms/top-level-view'
import { priorityWithProviderFirst } from '@/lib/browser-runtime/browser-runtime-control-center'
import type { BrowserRuntimeSettingsInput } from '@/lib/browser-runtime/browser-runtime-settings'
import type {
  BrowserRuntimeControlCenterReport,
  BrowserRuntimeProviderId,
} from '@/lib/startup/startup-doctor'
import type {
  BrowserIdentityStatusReport,
  PlaywrightSetupExecutionReport,
} from '@/lib/tauri-bridge'
import { settingsBridge } from '../../../lib/bridge/settings'

export function useBrowserRuntimeSettings(status?: BrowserRuntimeSettingsInput) {
  const setTopLevelView = useSetAtom(topLevelViewAtom)
  const setKaleidoscopeModule = useSetAtom(kaleidoscopeModuleAtom)
  const setSelectedBuiltinIntegration = useSetAtom(selectedBuiltinIntegrationAtom)
  const [liveStatus, setLiveStatus] = React.useState<BrowserRuntimeSettingsInput | undefined>()
  const [identityStatus, setIdentityStatus] = React.useState<BrowserIdentityStatusReport | undefined>()
  const [revokingProfileId, setRevokingProfileId] = React.useState<string | null>(null)
  const [controlCenter, setControlCenter] = React.useState<BrowserRuntimeControlCenterReport | undefined>(
    status?.report?.controlCenter,
  )
  const [controlCenterError, setControlCenterError] = React.useState<string | undefined>()
  const [controlCenterPendingAction, setControlCenterPendingAction] = React.useState<string | null>(null)
  const [probePendingProviderId, setProbePendingProviderId] =
    React.useState<BrowserRuntimeProviderId | null>(null)
  const [setupReport, setSetupReport] = React.useState<PlaywrightSetupExecutionReport | undefined>()
  const [rawReportOpen, setRawReportOpen] = React.useState(false)
  const refreshGenerationRef = React.useRef(0)
  const identityGenerationRef = React.useRef(0)
  const controlCenterGenerationRef = React.useRef(0)
  const mountedRef = React.useRef(false)

  React.useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      refreshGenerationRef.current += 1
      identityGenerationRef.current += 1
      controlCenterGenerationRef.current += 1
    }
  }, [])

  const refreshLiveStatus = React.useCallback(async () => {
    if (status) return

    const generation = refreshGenerationRef.current + 1
    refreshGenerationRef.current = generation

    try {
      const report = await settingsBridge.getBrowserRuntimeStatus()
      if (mountedRef.current && refreshGenerationRef.current === generation) {
        setLiveStatus({
          report,
          lastCheckedAtMs: Date.now(),
        })
        if (report.controlCenter) {
          setControlCenter(report.controlCenter)
          setControlCenterError(undefined)
        }
      }
    } catch {
      // Keep the last displayed status when a manual refresh fails.
    }
  }, [status])

  const refreshControlCenter = React.useCallback(async () => {
    if (status?.report?.controlCenter) {
      setControlCenter(status.report.controlCenter)
      setControlCenterError(undefined)
      return
    }

    const generation = controlCenterGenerationRef.current + 1
    controlCenterGenerationRef.current = generation

    try {
      const report = await settingsBridge.getBrowserRuntimeControlCenter()
      if (mountedRef.current && controlCenterGenerationRef.current === generation) {
        setControlCenter(report)
        setControlCenterError(undefined)
      }
    } catch (error) {
      if (mountedRef.current && controlCenterGenerationRef.current === generation) {
        setControlCenterError(error instanceof Error ? error.message : String(error))
      }
    }
  }, [status])

  const enableProvider = React.useCallback(async (providerId: BrowserRuntimeProviderId) => {
    if (status) return

    setControlCenterPendingAction(`enable:${providerId}`)
    try {
      const report = await settingsBridge.setBrowserRuntimeProviderEnabled(providerId, true)
      if (mountedRef.current) {
        setControlCenter(report)
        setControlCenterError(undefined)
      }
    } catch (error) {
      if (mountedRef.current) {
        setControlCenterError(error instanceof Error ? error.message : String(error))
      }
    } finally {
      if (mountedRef.current) {
        setControlCenterPendingAction(null)
      }
    }
  }, [status])

  const setProviderFirst = React.useCallback(async (
    providerId: BrowserRuntimeProviderId,
    currentPriority: BrowserRuntimeProviderId[],
  ) => {
    if (status) return

    setControlCenterPendingAction(`first:${providerId}`)
    try {
      const report = await settingsBridge.setBrowserRuntimeProviderPriority(
        priorityWithProviderFirst(currentPriority, providerId),
      )
      if (mountedRef.current) {
        setControlCenter(report)
        setControlCenterError(undefined)
      }
    } catch (error) {
      if (mountedRef.current) {
        setControlCenterError(error instanceof Error ? error.message : String(error))
      }
    } finally {
      if (mountedRef.current) {
        setControlCenterPendingAction(null)
      }
    }
  }, [status])

  const runProbe = React.useCallback(async (providerId: BrowserRuntimeProviderId) => {
    if (status || probePendingProviderId) return

    setProbePendingProviderId(providerId)
    setControlCenterError(undefined)
    try {
      await settingsBridge.runBrowserRuntimeProviderProbe(providerId)
      await refreshControlCenter()
    } catch (error) {
      if (mountedRef.current) {
        setControlCenterError(error instanceof Error ? error.message : String(error))
      }
    } finally {
      if (mountedRef.current) {
        setProbePendingProviderId(null)
      }
    }
  }, [probePendingProviderId, refreshControlCenter, status])

  const runSetup = React.useCallback(async () => {
    if (status || controlCenterPendingAction) return

    setControlCenterPendingAction('setup:auto')
    setControlCenterError(undefined)
    try {
      const report = await settingsBridge.runPlaywrightSetup('auto_setup')
      if (mountedRef.current) {
        setSetupReport(report)
      }
      await refreshLiveStatus()
      await refreshControlCenter()
    } catch (error) {
      if (mountedRef.current) {
        setControlCenterError(error instanceof Error ? error.message : String(error))
      }
    } finally {
      if (mountedRef.current) {
        setControlCenterPendingAction(null)
      }
    }
  }, [controlCenterPendingAction, refreshControlCenter, refreshLiveStatus, status])

  const setRawMcpToolsExposed = React.useCallback(async (exposed: boolean) => {
    if (status || controlCenterPendingAction) return

    setControlCenterPendingAction('mcp:raw-tools')
    setControlCenterError(undefined)
    try {
      const report = await settingsBridge.setBrowserRuntimeMcpRawToolsExposed(exposed)
      if (mountedRef.current) {
        setControlCenter(report)
        setControlCenterError(undefined)
      }
    } catch (error) {
      if (mountedRef.current) {
        setControlCenterError(error instanceof Error ? error.message : String(error))
      }
    } finally {
      if (mountedRef.current) {
        setControlCenterPendingAction(null)
      }
    }
  }, [controlCenterPendingAction, status])

  const openPlaywrightMcpIntegration = React.useCallback(() => {
    setTopLevelView('kaleidoscope')
    setKaleidoscopeModule('integrations')
    setSelectedBuiltinIntegration('playwright_mcp')
  }, [setKaleidoscopeModule, setSelectedBuiltinIntegration, setTopLevelView])

  const refreshIdentityStatus = React.useCallback(async () => {
    const generation = identityGenerationRef.current + 1
    identityGenerationRef.current = generation

    try {
      const report = await settingsBridge.listBrowserIdentities()
      if (mountedRef.current && identityGenerationRef.current === generation) {
        setIdentityStatus(report)
      }
    } catch {
      // Keep the last displayed identity status when a refresh fails.
    }
  }, [])

  const revokeIdentity = React.useCallback(async (profileId: string) => {
    if (revokingProfileId) return

    setRevokingProfileId(profileId)
    try {
      await settingsBridge.revokeBrowserIdentity(profileId)
      await refreshIdentityStatus()
    } catch {
      // Keep the current profile list if revocation fails.
    } finally {
      if (mountedRef.current) {
        setRevokingProfileId(null)
      }
    }
  }, [refreshIdentityStatus, revokingProfileId])

  React.useEffect(() => {
    if (status) {
      refreshGenerationRef.current += 1
      setLiveStatus(undefined)
      setControlCenter(status.report?.controlCenter)
      return
    }

    void refreshLiveStatus()
  }, [refreshLiveStatus, status])

  React.useEffect(() => {
    void refreshControlCenter()
  }, [refreshControlCenter])

  React.useEffect(() => {
    void refreshIdentityStatus()
  }, [refreshIdentityStatus])

  return {
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
  }
}
