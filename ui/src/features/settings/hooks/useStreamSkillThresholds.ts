// Owns the StreamSkillThresholdsSection side effects — the thresholds load on
// mount + the (per-field-conditional) save + dirty tracking. Extracted out of the
// component during the migration. The IPC lives in @/lib/stream-skill-thresholds
// (a Bundle-26/27 domain helper, NOT settings-domain — so it stays there rather
// than moving into settingsBridge; the component imports no @tauri-apps/api). The
// "fire only the setters that actually changed" + re-read-after-save logic is
// preserved verbatim.
import * as React from 'react'
import {
  getStreamSkillThresholds,
  setStreamIdleTimeoutSecs,
  setSkillPruneMinUnusedDays,
  setSkillPromoteMinReturnedCount,
  STREAM_SKILL_DEFAULTS,
  type StreamSkillThresholds,
} from '@/lib/stream-skill-thresholds'

export function useStreamSkillThresholds() {
  const [config, setConfig] = React.useState<StreamSkillThresholds>(STREAM_SKILL_DEFAULTS)
  const [pristine, setPristine] = React.useState<StreamSkillThresholds>(STREAM_SKILL_DEFAULTS)
  const [loading, setLoading] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [toast, setToast] = React.useState<string | null>(null)

  React.useEffect(() => {
    setLoading(true)
    getStreamSkillThresholds()
      .then((c) => {
        setConfig(c)
        setPristine(c)
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false))
  }, [])

  const dirty = React.useMemo(
    () =>
      config.stream_idle_timeout_secs !== pristine.stream_idle_timeout_secs ||
      config.skill_prune_min_unused_days !== pristine.skill_prune_min_unused_days ||
      config.skill_promote_min_returned_count !== pristine.skill_promote_min_returned_count,
    [config, pristine],
  )

  const handleSave = React.useCallback(async () => {
    setSaving(true)
    setError(null)
    setToast(null)
    try {
      // Three independent commands — fire only the ones that
      // actually changed so we don't trigger an unnecessary
      // proactive restart when the user only edited the LLM
      // timeout.
      const promises: Array<Promise<void>> = []
      if (config.stream_idle_timeout_secs !== pristine.stream_idle_timeout_secs) {
        promises.push(setStreamIdleTimeoutSecs(config.stream_idle_timeout_secs))
      }
      if (config.skill_prune_min_unused_days !== pristine.skill_prune_min_unused_days) {
        promises.push(setSkillPruneMinUnusedDays(config.skill_prune_min_unused_days))
      }
      if (config.skill_promote_min_returned_count !== pristine.skill_promote_min_returned_count) {
        promises.push(setSkillPromoteMinReturnedCount(config.skill_promote_min_returned_count))
      }
      await Promise.all(promises)
      // Re-read so we see the post-clamp value (backend clamps each
      // setter to a sane range and the clamped value is what gets
      // persisted).
      const updated = await getStreamSkillThresholds()
      setConfig(updated)
      setPristine(updated)
      setToast('已保存。stream 超时立即生效；技能阈值已触发 proactive 服务静默重启,下一个 tick 生效。')
    } catch (e) {
      setError(String(e))
    } finally {
      setSaving(false)
    }
  }, [config, pristine])

  const handleReset = React.useCallback(() => {
    setConfig(pristine)
    setError(null)
    setToast(null)
  }, [pristine])

  const handleResetToDefaults = React.useCallback(() => {
    setConfig(STREAM_SKILL_DEFAULTS)
    setError(null)
    setToast(null)
  }, [])

  return {
    config, setConfig,
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
