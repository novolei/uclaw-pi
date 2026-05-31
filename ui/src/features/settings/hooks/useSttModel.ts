// Owns the SttSettings model-side effects: the on-mount model-status probe, the
// audio-input device enumeration, and the download action. Extracted out of the
// component during the features/settings migration (code-organization ADR
// 2026-05-31). The raw `invoke('stt_model_status')` / `invoke('stt_download_model')`
// calls moved behind settingsBridge.sttModelStatus / .sttDownloadModel so no
// settings component imports `@tauri-apps/api`. The modelStatusAtom read/write
// lives here (it tracks the download lifecycle); the sttSettingsAtom prefs stay in
// the component. Behavior preserved verbatim — same status mapping, the same
// swallowed errors on probe/enumerate, the same download state transitions.
import * as React from 'react'
import { useAtom } from 'jotai'
import { settingsBridge } from '../../../lib/bridge/settings'
import { modelStatusAtom } from '@/atoms/stt-atoms'

export function useSttModel() {
  const [modelStatus, setModelStatus] = useAtom(modelStatusAtom)
  const [devices, setDevices] = React.useState<MediaDeviceInfo[]>([])

  React.useEffect(() => {
    void settingsBridge
      .sttModelStatus()
      .then((status) => {
        setModelStatus(
          status.openflow_ready
            ? { kind: 'ready', modelDir: status.openflow_model_dir }
            : { kind: 'not-downloaded', expectedDir: status.openflow_model_dir },
        )
      })
      .catch(() => {})
  }, [setModelStatus])

  React.useEffect(() => {
    if (navigator.mediaDevices?.enumerateDevices) {
      void navigator.mediaDevices
        .enumerateDevices()
        .then((d) => setDevices(d.filter((x) => x.kind === 'audioinput')))
        .catch(() => {})
    }
  }, [])

  const handleDownload = React.useCallback(async () => {
    setModelStatus({
      kind: 'downloading',
      file: 'model_quant.onnx',
      downloaded: 0,
      total: null,
      percent: 0,
    })
    try {
      const dir = await settingsBridge.sttDownloadModel({ preset: 'quantized', force: false })
      setModelStatus({ kind: 'ready', modelDir: dir })
    } catch (e) {
      setModelStatus({
        kind: 'error',
        message: String((e as Error)?.message ?? e),
      })
    }
  }, [setModelStatus])

  return { modelStatus, devices, handleDownload }
}
