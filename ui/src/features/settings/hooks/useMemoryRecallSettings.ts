// Owns the MemoryRecall config form state: the config, loading/saving/dirty flags,
// the load effect, and the updateField/save/reset actions. Extracted out of the
// 474-line component during the features/settings split (code-organization ADR
// 2026-05-31). IPC stays in the typed `@/lib/tauri-bridge`
// get/patchMemoryRecallConfig helpers (precedent: useChannelSettings). The
// merge-with-DEFAULTS, dirty-tracking, and error-logging behavior is preserved
// verbatim.
import { useCallback, useEffect, useState } from 'react'
import {
  getMemoryRecallConfig,
  patchMemoryRecallConfig,
  type MemoryRecallConfigDto,
} from '@/lib/tauri-bridge'
import { DEFAULTS } from '../lib/memory-recall'

export function useMemoryRecallSettings() {
  const [config, setConfig] = useState<MemoryRecallConfigDto>({ ...DEFAULTS })
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [dirty, setDirty] = useState(false)

  // 加载当前配置
  useEffect(() => {
    getMemoryRecallConfig()
      .then((cfg) => {
        // 合并：未设置的字段回退到默认值
        setConfig({ ...DEFAULTS, ...cfg })
      })
      .catch((err) => console.error('加载记忆召回配置失败:', err))
      .finally(() => setLoading(false))
  }, [])

  // 更新单个字段
  const updateField = useCallback(
    <K extends keyof MemoryRecallConfigDto>(key: K, value: MemoryRecallConfigDto[K]) => {
      setConfig((prev) => ({ ...prev, [key]: value }))
      setDirty(true)
    },
    [],
  )

  // 保存
  const handleSave = useCallback(async () => {
    setSaving(true)
    try {
      const saved = await patchMemoryRecallConfig(config)
      setConfig({ ...DEFAULTS, ...saved })
      setDirty(false)
    } catch (err) {
      console.error('保存记忆召回配置失败:', err)
    } finally {
      setSaving(false)
    }
  }, [config])

  // 恢复默认值
  const handleReset = useCallback(() => {
    setConfig({ ...DEFAULTS })
    setDirty(true)
  }, [])

  return { config, loading, saving, dirty, updateField, handleSave, handleReset }
}
