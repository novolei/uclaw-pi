// Model-provider settings — the grouped provider list + detail pane. Thin
// shell: list data (providers, configured ids, model counts, refresh) lives in
// useChannelSettings; the detail panel is ProviderDetail (its own hook). Split
// out of the 455-line legacy settings module version during the migration. IPC
// stays in the typed `@/lib/tauri-bridge` provider helpers (no Tauri API here).
import { cn } from '@/lib/utils'
import { useChannelSettings } from '../../hooks/useChannelSettings'
import { ProviderDetail } from './ProviderDetail'

// Re-exported so existing consumers/tests that import `ProviderDetail` from
// `ChannelSettings` keep working after the split.
export { ProviderDetail } from './ProviderDetail'

const CATEGORY_ORDER: { key: string; label: string }[] = [
  { key: 'OAuth', label: 'OAUTH' },
  { key: 'CodingPlan', label: 'CODING PLAN' },
  { key: 'Api', label: 'API' },
]

export function ChannelSettings() {
  const { selectedId, setSelectedId, configuredIds, modelCounts, selected, grouped, refreshData } =
    useChannelSettings()

  return (
    <div className="flex-1 min-h-0 grid grid-cols-[220px_1fr] grid-rows-1 overflow-hidden">
      {/* Left: grouped provider list */}
      <div className="overflow-y-auto border-r border-border bg-muted/20">
        {CATEGORY_ORDER.map(({ key, label }) => {
          const items = grouped.get(key) ?? []
          if (items.length === 0) return null
          return (
            <div key={key} className="py-1.5">
              <div className="px-3 py-1 text-[10px] font-semibold uppercase tracking-widest text-muted-foreground/50">
                {label}
              </div>
              {items.map((p) => {
                const isConfigured = configuredIds.has(p.id)
                const isSelected = selectedId === p.id
                const count = modelCounts.get(p.id) ?? 0
                return (
                  <button
                    key={p.id}
                    type="button"
                    onClick={() => setSelectedId(p.id)}
                    className={cn(
                      'flex w-full items-center justify-between gap-2 px-3 py-1.5 text-left text-[12px] transition-colors hover:bg-accent/50',
                      isSelected && 'bg-accent text-accent-foreground',
                    )}
                  >
                    <div className="flex min-w-0 items-center gap-2">
                      <span
                        className={cn(
                          'h-1.5 w-1.5 shrink-0 rounded-full',
                          isConfigured ? 'bg-green-500' : 'bg-muted-foreground/25',
                        )}
                        aria-hidden
                      />
                      <span className="truncate">{p.displayName}</span>
                    </div>
                    <span className="shrink-0 text-[10.5px] text-muted-foreground/50">
                      {count > 0 ? count : ''}
                    </span>
                  </button>
                )
              })}
            </div>
          )
        })}
      </div>

      {/* Right: detail panel */}
      <div className="overflow-y-auto px-6 py-5">
        {selected ? (
          <ProviderDetail
            provider={selected}
            isConfigured={configuredIds.has(selected.id)}
            onSaved={() => void refreshData()}
          />
        ) : (
          <ProviderEmptyState />
        )}
      </div>
    </div>
  )
}

function ProviderEmptyState() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 text-[12px] text-muted-foreground">
      <span>从左侧选择一个服务商以配置 API Key、Base URL 与可用模型。</span>
      <span className="text-[10.5px] text-muted-foreground/60">
        三个分组：OAuth · Coding Plan（订阅制）· API（标准 Key 服务）
      </span>
    </div>
  )
}
