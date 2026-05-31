import * as React from 'react'
import { Save, RotateCcw, Plug } from 'lucide-react'
import { useEmbeddingEndpoint } from '../hooks/useEmbeddingEndpoint'

export function EmbeddingEndpointSection(): React.ReactElement {
  const {
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
  } = useEmbeddingEndpoint()

  return (
    <div className="border border-border rounded-lg p-4 space-y-3">
      <div>
        <h3 className="text-sm font-semibold">Embedding 端点配置</h3>
        <p className="text-[11px] text-muted-foreground mt-0.5">
          gbrain 把内容向量化时调用的 endpoint + 模型。默认指向 uClaw 自带的
          <code className="mx-1 px-1 py-0.5 rounded bg-muted text-[10px]">/v1/embeddings</code>
          (由 memU FastEmbed 后端) — 无需外部 API key 即可工作。
        </p>
      </div>

      {loading && <p className="text-[11px] text-muted-foreground">读取中...</p>}

      {!loading && (
        <div className="space-y-2">
          <Field
            label="Base URL"
            description="gbrain config base_urls.llama-server"
            value={config.base_url}
            onChange={(v) => setConfig({ ...config, base_url: v })}
            placeholder="http://localhost:7337/v1"
          />
          <Field
            label="模型 (gbrain embedding_model)"
            description="格式 <recipe>:<model>，例如 llama-server:bge-small-en-v1.5"
            value={config.model}
            onChange={(v) => setConfig({ ...config, model: v })}
            placeholder="llama-server:bge-small-en-v1.5"
          />
          <Field
            label="向量维度 (gbrain embedding_dimensions)"
            description="必须跟 FastEmbed 模型的输出维度一致 (bge-small=384, bge-m3=1024)"
            value={String(config.dimensions)}
            onChange={(v) => {
              const n = parseInt(v, 10)
              setConfig({ ...config, dimensions: Number.isFinite(n) ? n : 0 })
            }}
            placeholder="384"
            type="number"
          />
          <Field
            label="FastEmbed 模型 (memU)"
            description="memU bridge 加载的 FastEmbed 模型 id (例如 BAAI/bge-m3 多语言)。变更会触发 memU 重启。"
            value={config.fastembed_model}
            onChange={(v) => setConfig({ ...config, fastembed_model: v })}
            placeholder="BAAI/bge-small-en-v1.5"
          />
        </div>
      )}

      <div className="flex items-center gap-2 pt-2">
        <button
          onClick={handleSave}
          disabled={!dirty || saving || loading || testing}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-[11px] font-medium bg-primary text-primary-foreground disabled:opacity-50"
        >
          <Save size={11} />
          {saving ? '保存中...' : '保存'}
        </button>
        <button
          onClick={handleTest}
          disabled={saving || loading || testing || !config.base_url}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-[11px] bg-muted text-foreground hover:bg-accent disabled:opacity-50"
          title="2 秒内 GET <base_url>/models 验证端点可达"
        >
          <Plug size={11} />
          {testing ? '测试中...' : '测试连接'}
        </button>
        <button
          onClick={handleReset}
          disabled={!dirty || saving || testing}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-[11px] bg-muted text-muted-foreground hover:bg-accent disabled:opacity-50"
        >
          <RotateCcw size={11} />
          重置
        </button>
        {dirty && !saving && (
          <span className="text-[10px] text-yellow-500">未保存的更改</span>
        )}
      </div>

      {error && (
        <p className="text-[11px] text-destructive bg-destructive/10 px-2 py-1.5 rounded">
          {error}
        </p>
      )}
      {toast && (
        <p className="text-[11px] text-green-500 bg-green-500/10 px-2 py-1.5 rounded">
          {toast}
        </p>
      )}
    </div>
  )
}

interface FieldProps {
  label: string
  description: string
  value: string
  onChange: (v: string) => void
  placeholder?: string
  type?: 'text' | 'number'
}

function Field({ label, description, value, onChange, placeholder, type = 'text' }: FieldProps): React.ReactElement {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[11px] font-medium">{label}</span>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="px-2 py-1.5 rounded border border-input bg-background text-[11px] font-mono"
      />
      <span className="text-[10px] text-muted-foreground">{description}</span>
    </label>
  )
}
