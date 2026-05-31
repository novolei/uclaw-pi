import * as React from 'react'
import { Save, RotateCcw, RotateCw } from 'lucide-react'
import { useStreamSkillThresholds } from '../hooks/useStreamSkillThresholds'

/**
 * Bundle 26-B / 26-D / 27-B settings section.
 *
 * Surfaces three thresholds that were originally hardcoded constants:
 * - `stream_idle_timeout_secs` (Bundle 27-B) — applies on next message,
 *   no restart needed.
 * - `skill_prune_min_unused_days` (Bundle 26-B) — applies on next
 *   prune tick (~2h cadence). Save triggers a silent proactive
 *   restart so the new value lands in the runtime snapshot.
 * - `skill_promote_min_returned_count` (Bundle 26-D) — same as above
 *   on the promotion tick (~30min cadence).
 *
 * Presentation only; the load/save/dirty side effects live in
 * useStreamSkillThresholds (IPC stays in @/lib/stream-skill-thresholds).
 */
export function StreamSkillThresholdsSection(): React.ReactElement {
  const {
    config, setConfig,
    loading,
    saving,
    error,
    toast,
    dirty,
    handleSave,
    handleReset,
    handleResetToDefaults,
  } = useStreamSkillThresholds()

  return (
    <div className="border border-border rounded-lg p-4 space-y-3">
      <div>
        <h3 className="text-sm font-semibold">流式与技能蒸馏阈值 (Bundle 26/27)</h3>
        <p className="text-[11px] text-muted-foreground mt-0.5">
          调整 LLM 流式响应空闲超时,以及自动提取技能的归档/升级判定阈值。
          stream 超时改完下一条消息立即生效;技能阈值改完会静默重启 proactive 服务以便下一个 tick 生效。
        </p>
      </div>

      {loading && <p className="text-[11px] text-muted-foreground">读取中...</p>}

      {!loading && (
        <div className="space-y-2">
          <Field
            label="流式响应空闲超时 (秒)"
            description="上游 LLM 在此时间内未发送新 chunk 则触发重试。默认 90s。Kimi K 等丢包频繁的提供方建议 60s;Sonnet/DeepSeek-R1 等慢推理模型保持 90s 或更高。后端 clamp 至 [5, 600]。"
            value={config.stream_idle_timeout_secs}
            onChange={(v) =>
              setConfig({ ...config, stream_idle_timeout_secs: v })
            }
            placeholder="90"
            min={5}
            max={600}
          />
          <Field
            label="技能归档闲置天数"
            description="未使用超过此天数且返回次数 ≤ 1 的自动提取技能将被归档(不删除,移到 _archive/ 目录,可还原)。默认 30 天。后端 clamp 至 [1, 365]。"
            value={config.skill_prune_min_unused_days}
            onChange={(v) =>
              setConfig({ ...config, skill_prune_min_unused_days: v })
            }
            placeholder="30"
            min={1}
            max={365}
          />
          <Field
            label="技能升级为基因的最小返回次数"
            description="被 skill_search 推荐过此次数以上的技能将被推入 GEP gene_candidate_pool 升级为 Gene。默认 3。后端 clamp 至 [1, 100]。"
            value={config.skill_promote_min_returned_count}
            onChange={(v) =>
              setConfig({ ...config, skill_promote_min_returned_count: v })
            }
            placeholder="3"
            min={1}
            max={100}
          />
        </div>
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
          title="把三个字段恢复到出厂默认 (90 / 30 / 3),还需要点保存才会落盘"
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
