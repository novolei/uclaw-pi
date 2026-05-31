// Owns the EmbeddingEndpointSection side effects — the config load on mount +
// the save/test/reset handlers + dirty tracking. Extracted out of the component
// during the migration. The IPC lives in @/lib/embedding-endpoint (a gbrain/memU
// domain helper, NOT settings-domain — so it stays there rather than moving into
// settingsBridge; the component imports no @tauri-apps/api). Error/toast state +
// the re-read-after-save flow are preserved verbatim.
import * as React from 'react'
import {
  getEmbeddingConfig,
  setEmbeddingConfig,
  testEmbeddingEndpoint,
  type EmbeddingEndpointConfig,
} from '@/lib/embedding-endpoint'

const DEFAULT_CONFIG: EmbeddingEndpointConfig = {
  base_url: 'http://localhost:7337/v1',
  model: 'llama-server:bge-small-en-v1.5',
  dimensions: 384,
  fastembed_model: 'BAAI/bge-small-en-v1.5',
}

export function useEmbeddingEndpoint() {
  const [config, setConfig] = React.useState<EmbeddingEndpointConfig>(DEFAULT_CONFIG)
  const [pristine, setPristine] = React.useState<EmbeddingEndpointConfig>(DEFAULT_CONFIG)
  const [loading, setLoading] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  const [testing, setTesting] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [toast, setToast] = React.useState<string | null>(null)

  React.useEffect(() => {
    setLoading(true)
    getEmbeddingConfig()
      .then((c) => {
        setConfig(c)
        setPristine(c)
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false))
  }, [])

  const dirty = React.useMemo(
    () =>
      config.base_url !== pristine.base_url ||
      config.model !== pristine.model ||
      config.dimensions !== pristine.dimensions ||
      config.fastembed_model !== pristine.fastembed_model,
    [config, pristine],
  )

  const handleSave = React.useCallback(async () => {
    setSaving(true)
    setError(null)
    setToast(null)
    try {
      const updated = await setEmbeddingConfig(config)
      setConfig(updated)
      setPristine(updated)
      setToast('已保存。如修改了 FastEmbed 模型，memU 已自动重启。')
    } catch (e) {
      setError(String(e))
    } finally {
      setSaving(false)
    }
  }, [config])

  const handleReset = React.useCallback(() => {
    setConfig(pristine)
    setError(null)
    setToast(null)
  }, [pristine])

  const handleTest = React.useCallback(async () => {
    setTesting(true)
    setError(null)
    setToast(null)
    try {
      await testEmbeddingEndpoint(config.base_url)
      setToast(`✓ ${config.base_url} 可达`)
    } catch (e) {
      setError(String(e))
    } finally {
      setTesting(false)
    }
  }, [config.base_url])

  return {
    config, setConfig,
    loading,
    saving,
    testing,
    error,
    toast,
    dirty,
    handleSave,
    handleReset,
    handleTest,
  }
}
