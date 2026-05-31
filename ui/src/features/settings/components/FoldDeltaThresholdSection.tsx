import * as React from 'react'
import { Save, RotateCcw, RotateCw } from 'lucide-react'
import { useFoldDeltaThreshold } from '../hooks/useFoldDeltaThreshold'
import {
  FOLD_DELTA_THRESHOLD_DEFAULT,
  FOLD_DELTA_THRESHOLD_MIN,
  FOLD_DELTA_THRESHOLD_MAX,
} from '@/lib/fold-delta-threshold'

/**
 * Bundle 17-B — `/compact` fold-delta threshold setting.
 *
 * When you trigger `/compact` on a session that already has a structured
 * fold from a prior compaction, the new fold's drift (added+removed+
 * changed across all 8 axes) is compared to this threshold:
 *
 * - drift < threshold → render a `<context_changes_since_last_fold>`
 *   delta block on top of the byte-stable prior fold (prompt-cache
 *   friendly).
 * - drift ≥ threshold → emit a fresh full re-render (current default
 *   behavior).
 *
 * Loose default of 50 favors the delta path while telemetry from Bundle
 * 17-C `FoldDeltaStats` accumulates data for a principled retune.
 *
 * Takes effect on the next `/compact` — no restart needed (dispatcher
 * reads `cfg.context.fold_delta_threshold` afresh on each invocation).
 */
export function FoldDeltaThresholdSection(): React.ReactElement {
  const {
    value, setValue,
    loading,
    saving,
    error,
    toast,
    dirty,
    handleSave,
    handleReset,
    handleResetToDefaults,
  } = useFoldDeltaThreshold()

  return (
    <div className="border border-border rounded-lg p-4 space-y-3">
      <div>
        <h3 className="text-sm font-semibold">/compact 折叠 delta 阈值 (Bundle 17-B)</h3>
        <p className="text-[11px] text-muted-foreground mt-0.5">
          /compact 二次触发时,新 StructuredFold 与上次 baseline 的差量(8 个轴的 added+removed+changed 之和)
          低于此阈值就走 delta-rendered 路径(在 byte-stable 上一份 fold 上方追加变更块,
          下回合 LLM prompt cache 命中率更高);否则全量重写。
          松默认 50,后续靠 FoldDeltaStats 遥测做数据驱动调优。改完下次 /compact 立即生效,无需重启。
        </p>
      </div>

      {loading && <p className="text-[11px] text-muted-foreground">读取中...</p>}

      {!loading && (
        <Field
          label="fold_delta_threshold"
          description={`drift 累计变更数。1 = 几乎全部走重写(等于禁用 delta);50 = 几乎都走 delta。后端 clamp 至 [${FOLD_DELTA_THRESHOLD_MIN}, ${FOLD_DELTA_THRESHOLD_MAX}]。`}
          value={value}
          onChange={setValue}
          placeholder="50"
          min={FOLD_DELTA_THRESHOLD_MIN}
          max={FOLD_DELTA_THRESHOLD_MAX}
        />
      )}

      <div className="flex items-center gap-2 pt-2">
        <button
          onClick={handleSave}
          disabled={!dirty || saving || loading}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-[11px] font-medium bg-primary text-primary-foreground disabled:opacity-50"
        >
          <Save size={11} />
          {saving ? '保存中...' : '保存'}
        </button>
        <button
          onClick={handleReset}
          disabled={!dirty || saving}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-[11px] bg-muted text-muted-foreground hover:bg-accent disabled:opacity-50"
        >
          <RotateCcw size={11} />
          撤销
        </button>
        <button
          onClick={handleResetToDefaults}
          disabled={saving || loading}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-[11px] bg-muted text-muted-foreground hover:bg-accent disabled:opacity-50"
          title={`恢复默认 ${FOLD_DELTA_THRESHOLD_DEFAULT}, 还需要点保存才落盘`}
        >
          <RotateCw size={11} />
          恢复默认
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
  value: number
  onChange: (v: number) => void
  placeholder?: string
  min?: number
  max?: number
}

function Field({ label, description, value, onChange, placeholder, min, max }: FieldProps): React.ReactElement {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[11px] font-medium">{label}</span>
      <input
        type="number"
        value={Number.isFinite(value) ? value : ''}
        onChange={(e) => {
          const n = parseInt(e.target.value, 10)
          onChange(Number.isFinite(n) ? n : 0)
        }}
        placeholder={placeholder}
        min={min}
        max={max}
        className="px-2 py-1.5 rounded border border-input bg-background text-[11px] font-mono"
      />
      <span className="text-[10px] text-muted-foreground">{description}</span>
    </label>
  )
}
