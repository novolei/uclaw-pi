// Owns the FoldDeltaThresholdSection side effects — the threshold load on mount +
// the save (with re-read of the post-clamp value) + dirty tracking. Extracted out
// of the component during the migration. The IPC lives in @/lib/fold-delta-threshold
// (a Bundle-17-B domain helper, NOT settings-domain — so it stays there rather than
// moving into settingsBridge; the component imports no @tauri-apps/api). The
// FOLD_DELTA_THRESHOLD_* constants stay in the component (used in field labels).
import * as React from 'react'
import {
  getFoldDeltaThreshold,
  setFoldDeltaThreshold,
  FOLD_DELTA_THRESHOLD_DEFAULT,
} from '@/lib/fold-delta-threshold'

export function useFoldDeltaThreshold() {
  const [value, setValue] = React.useState<number>(FOLD_DELTA_THRESHOLD_DEFAULT)
  const [pristine, setPristine] = React.useState<number>(FOLD_DELTA_THRESHOLD_DEFAULT)
  const [loading, setLoading] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [toast, setToast] = React.useState<string | null>(null)

  React.useEffect(() => {
    setLoading(true)
    getFoldDeltaThreshold()
      .then((v) => {
        setValue(v)
        setPristine(v)
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false))
  }, [])

  const dirty = value !== pristine

  const handleSave = React.useCallback(async () => {
    setSaving(true)
    setError(null)
    setToast(null)
    try {
      await setFoldDeltaThreshold(value)
      // Re-read post-clamp value so the UI reflects what the backend
      // actually persisted.
      const updated = await getFoldDeltaThreshold()
      setValue(updated)
      setPristine(updated)
      setToast(`已保存。下一次 /compact 触发时按 drift < ${updated} 走 delta 路径。`)
    } catch (e) {
      setError(String(e))
    } finally {
      setSaving(false)
    }
  }, [value])

  const handleReset = React.useCallback(() => {
    setValue(pristine)
    setError(null)
    setToast(null)
  }, [pristine])

  const handleResetToDefaults = React.useCallback(() => {
    setValue(FOLD_DELTA_THRESHOLD_DEFAULT)
    setError(null)
    setToast(null)
  }, [])

  return {
    value, setValue,
    loading,
    saving,
    error,
    toast,
    dirty,
    handleSave,
    handleReset,
    handleResetToDefaults,
  }
}
