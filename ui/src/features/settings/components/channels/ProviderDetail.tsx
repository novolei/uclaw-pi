// Model-provider detail panel — presentation only. All state + the provider IPC
// (config load, model load, test, save, delete) live in useProviderDetail (IPC
// via the typed `@/lib/tauri-bridge` provider helpers; no Tauri API here). Split
// out of the 455-line ChannelSettings during the migration. SettingsSecretInput
// is a migrated feature-internal primitive (../primitives), imported relatively.
import { Check, RefreshCw, Trash2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { ProviderInfo } from '@/lib/types'
import { SettingsSecretInput } from '../primitives/SettingsSecretInput'
import { useProviderDetail } from '../../hooks/useProviderDetail'

const API_TYPE_OPTIONS = [
  { value: 'openai-completions', label: 'OpenAI Compatible' },
  { value: 'anthropic-messages', label: 'Anthropic Messages' },
  { value: 'openai-responses', label: 'OpenAI Responses' },
]

interface ProviderDetailProps {
  provider: ProviderInfo
  isConfigured: boolean
  onSaved: () => void
}

export function ProviderDetail({ provider, isConfigured, onSaved }: ProviderDetailProps) {
  const {
    apiKey, setApiKey, hasApiKey, maskedKey,
    baseUrl, setBaseUrl, apiType, setApiType,
    availableModels, selectedModelIds, busy,
    handleLoadModels, toggleModel, handleTest, handleSave, handleDelete,
  } = useProviderDetail(provider, onSaved)

  return (
    <div className="flex flex-col gap-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-[15px] font-medium">{provider.displayName}</h3>
          <p className="text-[11px] text-muted-foreground">
            {provider.id} · {provider.serviceCategory}
          </p>
        </div>
        {isConfigured && (
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="gap-1 text-destructive hover:text-destructive"
            disabled={busy}
            onClick={() => void handleDelete()}
          >
            <Trash2 className="h-3.5 w-3.5" />
            删除
          </Button>
        )}
      </div>

      {/* Credentials grid */}
      <div className="grid grid-cols-[80px_1fr] items-center gap-x-3 gap-y-2 text-[12px]">
        <label className="text-muted-foreground">API Key</label>
        {provider.authType === 'oauth' || provider.authType === 'OAuth' ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled
            className="justify-self-start text-[11px]"
          >
            通过 OAuth 连接（即将上线）
          </Button>
        ) : (
          <SettingsSecretInput
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            autoComplete="off"
            spellCheck={false}
            disabled={provider.authType === 'none' || provider.authType === 'None'}
            placeholder={
              provider.authType === 'none' || provider.authType === 'None'
                ? '无需 API Key'
                : hasApiKey && !apiKey
                  ? `已配置 ••••${maskedKey ?? ''}（输入以更新）`
                  : 'sk-…'
            }
            className="w-full rounded-md border border-input bg-background px-2.5 py-1.5 text-[12px] outline-none placeholder:text-muted-foreground/50 focus:ring-1 focus:ring-ring disabled:opacity-50"
          />
        )}

        <label className="text-muted-foreground">Base URL</label>
        <input
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          autoComplete="off"
          spellCheck={false}
          placeholder={provider.defaultBaseUrl}
          className="w-full rounded-md border border-input bg-background px-2.5 py-1.5 text-[12px] outline-none placeholder:text-muted-foreground/50 focus:ring-1 focus:ring-ring"
        />

        <label className="text-muted-foreground">API 类型</label>
        <select
          value={apiType}
          onChange={(e) => setApiType(e.target.value)}
          className="rounded-md border border-input bg-background px-2 py-1.5 text-[12px] outline-none focus:ring-1 focus:ring-ring"
        >
          {API_TYPE_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
      </div>

      {/* Models section */}
      <div className="border-t border-border pt-3">
        <div className="mb-2 flex items-center justify-between">
          <div className="text-[12px] text-muted-foreground">
            已添加的模型{' '}
            <span className="text-muted-foreground/50">{selectedModelIds.size}</span>
          </div>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="gap-1"
              disabled={busy || provider.authType === 'oauth' || provider.authType === 'OAuth'}
              onClick={() => void handleLoadModels()}
            >
              <RefreshCw className={cn('h-3.5 w-3.5', busy && 'animate-spin')} />
              读取模型
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={busy}
              onClick={() => void handleTest()}
            >
              测试连接
            </Button>
          </div>
        </div>

        {availableModels.length === 0 ? (
          <p className="rounded-md border border-dashed border-border bg-muted/30 px-3 py-4 text-center text-[11px] text-muted-foreground">
            暂无已保存模型。点击「读取模型」从供应商加载可用模型。
          </p>
        ) : (
          <ul className="divide-y divide-border rounded-md border border-border">
            {availableModels.map((model) => {
              const checked = selectedModelIds.has(model.id)
              return (
                <li key={model.id}>
                  <button
                    type="button"
                    onClick={() => toggleModel(model.id)}
                    className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-[12px] hover:bg-accent/30"
                  >
                    <div className="flex min-w-0 items-center gap-2">
                      <span
                        className={cn(
                          'flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded border',
                          checked
                            ? 'border-primary bg-primary text-primary-foreground'
                            : 'border-muted-foreground/30 bg-background',
                        )}
                      >
                        {checked ? <Check className="h-2.5 w-2.5" /> : null}
                      </span>
                      <span className="truncate font-medium">{model.name}</span>
                      {model.id !== model.name && (
                        <span className="truncate text-[10.5px] text-muted-foreground/50">
                          {model.id}
                        </span>
                      )}
                    </div>
                    <div className="flex shrink-0 items-center gap-1.5">
                      {model.reasoning && (
                        <span className="rounded bg-primary/10 px-1.5 text-[9.5px] text-primary">
                          thinking
                        </span>
                      )}
                      {model.contextWindow ? (
                        <span className="rounded bg-muted px-1.5 text-[9.5px] text-muted-foreground">
                          {(model.contextWindow / 1000).toFixed(0)}K
                        </span>
                      ) : null}
                    </div>
                  </button>
                </li>
              )
            })}
          </ul>
        )}
      </div>

      <div className="flex justify-end pt-2">
        <Button type="button" size="sm" disabled={busy} onClick={() => void handleSave()}>
          保存
        </Button>
      </div>
    </div>
  )
}
