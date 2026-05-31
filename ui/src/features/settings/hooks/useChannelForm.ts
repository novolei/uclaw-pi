// Owns the ChannelForm side effects: load the existing provider config on
// provider change + the submit (configureProvider) handler. Extracted out of the
// 86-line ChannelForm during the migration. IPC stays in the typed
// `@/lib/tauri-bridge` provider helpers (model-provider domain, not settings; the
// component imports no Tauri API). Behavior — masked key not pre-filled, error
// logged to console, submitting flag — preserved verbatim. (No in-tree consumer
// renders this form today.)
import { useEffect, useState } from 'react'
import { configureProvider, getProviderConfig } from '@/lib/tauri-bridge'

export function useChannelForm(providerId: string | null, onSaved: () => void) {
  const [apiKey, setApiKey] = useState('')
  const [baseUrl, setBaseUrl] = useState('')
  const [submitting, setSubmitting] = useState(false)

  useEffect(() => {
    if (providerId) {
      getProviderConfig(providerId).then((config) => {
        if (config) {
          setBaseUrl(config.baseUrl || '')
          // API key is masked, don't pre-fill
        }
      })
    }
  }, [providerId])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!providerId) return

    setSubmitting(true)
    try {
      await configureProvider({
        providerId,
        displayName: providerId,
        apiKey,
        baseUrl: baseUrl || undefined,
      })
      onSaved()
    } catch (err) {
      console.error('Failed to configure provider:', err)
    } finally {
      setSubmitting(false)
    }
  }

  return { apiKey, setApiKey, baseUrl, setBaseUrl, submitting, handleSubmit }
}
