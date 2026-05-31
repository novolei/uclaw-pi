// Provider quick-configure modal — presentation only. The config load + submit
// (configureProvider) live in useChannelForm (IPC via the typed
// `@/lib/tauri-bridge` provider helpers; no Tauri API here). Moved out of
// components/settings/ during the migration; the Settings* primitives still live
// under components/settings/. No in-tree consumer renders this form today.
import { SettingsSection } from '@/components/settings/primitives/SettingsSection'
import { SettingsInput } from '@/components/settings/primitives/SettingsInput'
import { SettingsSecretInput } from '@/components/settings/primitives/SettingsSecretInput'
import { Button } from '@/components/ui/button'
import { useChannelForm } from '../../hooks/useChannelForm'

interface ChannelFormProps {
  providerId: string | null
  onClose: () => void
  onSaved: () => void
}

export function ChannelForm({ providerId, onClose, onSaved }: ChannelFormProps) {
  const { apiKey, setApiKey, baseUrl, setBaseUrl, submitting, handleSubmit } =
    useChannelForm(providerId, onSaved)

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-background border border-border rounded-xl p-6 w-[480px] max-w-[90vw] space-y-4">
        <h3 className="text-base font-semibold">
          配置 Provider: {providerId}
        </h3>

        <form onSubmit={handleSubmit} className="space-y-4">
          <SettingsSection>
            <SettingsSecretInput
              label="API Key"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="sk-..."
              required
            />
            <SettingsInput
              label="Base URL（可选）"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="https://api.openai.com/v1"
            />
          </SettingsSection>

          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onClose}>
              取消
            </Button>
            <Button type="submit" disabled={submitting}>
              {submitting ? '保存中...' : '保存'}
            </Button>
          </div>
        </form>
      </div>
    </div>
  )
}
