// Owns the ProviderDetail panel: credential/model state, the load-on-provider
// change effect, and the load-models / toggle / test / save / delete handlers.
// Extracted out of the 455-line ChannelSettings during the split. IPC stays in
// the typed `@/lib/tauri-bridge` provider helpers (model-provider domain, not
// settings-domain; the component imports no Tauri API). All toasts + the
// reset-on-provider-switch are preserved verbatim.
import { useCallback, useEffect, useState } from 'react'
import { toast } from 'sonner'
import {
  getProviderConfig,
  configureProviderWithModels,
  removeProviderConfig,
  testProviderConnection,
  listProviderModels,
  getConfiguredModels,
} from '@/lib/tauri-bridge'
import type { ProviderInfo, ModelInfo } from '@/lib/types'

export function useProviderDetail(provider: ProviderInfo, onSaved: () => void) {
  const [apiKey, setApiKey] = useState('')
  const [hasApiKey, setHasApiKey] = useState(false)
  const [maskedKey, setMaskedKey] = useState<string | null>(null)
  const [baseUrl, setBaseUrl] = useState(provider.defaultBaseUrl)
  const [apiType, setApiType] = useState(provider.defaultApi || 'openai-completions')
  const [availableModels, setAvailableModels] = useState<ModelInfo[]>([])
  const [selectedModelIds, setSelectedModelIds] = useState<Set<string>>(new Set())
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    setBaseUrl(provider.defaultBaseUrl)
    setApiType(provider.defaultApi || 'openai-completions')
    setApiKey('')
    setHasApiKey(false)
    setMaskedKey(null)
    setAvailableModels([])
    setSelectedModelIds(new Set())

    void (async () => {
      const [cfg, savedModelIds] = await Promise.all([
        getProviderConfig(provider.id),
        getConfiguredModels(provider.id),
      ])
      if (cfg) {
        setBaseUrl(cfg.baseUrl ?? provider.defaultBaseUrl)
        if (cfg.api) setApiType(cfg.api)
        setHasApiKey(cfg.hasApiKey)
        setMaskedKey(cfg.maskedKey ?? null)
      }
      if (savedModelIds.length > 0) {
        setAvailableModels(
          savedModelIds.map((id) => ({
            id,
            name: id,
            modality: 'Text',
            reasoning: false,
            supportsReasoningEffort: false,
          })),
        )
        setSelectedModelIds(new Set(savedModelIds))
      }
    })()
  }, [provider.id, provider.defaultBaseUrl, provider.defaultApi])

  const handleLoadModels = useCallback(async () => {
    setBusy(true)
    try {
      const models = await listProviderModels({
        providerId: provider.id,
        baseUrl: baseUrl || provider.defaultBaseUrl,
        apiKey: apiKey || null,
      })
      setAvailableModels(models)
      if (models.length === 0) {
        toast.warning('未拉取到模型，请确认 Base URL / API Key 正确。')
      }
    } catch (e) {
      toast.error(`读取模型失败: ${(e as Error).message ?? e}`)
    } finally {
      setBusy(false)
    }
  }, [provider.id, provider.defaultBaseUrl, baseUrl, apiKey])

  const toggleModel = useCallback((id: string) => {
    setSelectedModelIds((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }, [])

  const handleTest = useCallback(async () => {
    if (provider.authType === 'apikey' && !apiKey) {
      toast.warning('请先填写 API Key。')
      return
    }
    setBusy(true)
    try {
      const result = await testProviderConnection({
        providerId: provider.id,
        baseUrl: baseUrl || provider.defaultBaseUrl,
        apiKey: apiKey || null,
      })
      if (result.success) {
        toast.success(`连接成功${result.latencyMs ? ` (${result.latencyMs}ms)` : ''}`)
      } else {
        toast.error(`连接失败: ${result.message}`)
      }
    } catch (e) {
      toast.error(`连接失败: ${(e as Error).message ?? e}`)
    } finally {
      setBusy(false)
    }
  }, [provider, apiKey, baseUrl])

  const handleSave = useCallback(async () => {
    if (selectedModelIds.size === 0 && availableModels.length > 0) {
      toast.warning('请至少选择一个模型。')
      return
    }
    setBusy(true)
    try {
      await configureProviderWithModels({
        providerId: provider.id,
        displayName: provider.displayName,
        apiKey: apiKey || null,
        baseUrl: baseUrl || null,
        api: apiType,
        modelIds: Array.from(selectedModelIds),
      })
      toast.success('已保存')
      onSaved()
    } catch (e) {
      toast.error(`保存失败: ${(e as Error).message ?? e}`)
    } finally {
      setBusy(false)
    }
  }, [provider, apiKey, baseUrl, apiType, availableModels, selectedModelIds, onSaved])

  const handleDelete = useCallback(async () => {
    setBusy(true)
    try {
      await removeProviderConfig(provider.id)
      toast.success('已删除')
      onSaved()
    } catch (e) {
      toast.error(`删除失败: ${(e as Error).message ?? e}`)
    } finally {
      setBusy(false)
    }
  }, [provider.id, onSaved])

  return {
    apiKey, setApiKey,
    hasApiKey, maskedKey,
    baseUrl, setBaseUrl,
    apiType, setApiType,
    availableModels,
    selectedModelIds,
    busy,
    handleLoadModels,
    toggleModel,
    handleTest,
    handleSave,
    handleDelete,
  }
}
