// Settings-only NON-IPC constants + helpers for the MemoryRecall config form,
// shared by useMemoryRecallSettings (the hook) and the memory-recall/ cards.
// Extracted verbatim out of the 474-line components/settings/MemoryRecallSettings
// during the features/settings split (code-organization ADR 2026-05-31). Kept in
// sync with the Rust MemoryRecallConfig::default() + patch_memory_recall_config.
import type { MemoryRecallConfigDto } from '@/lib/tauri-bridge'

// ─── 默认值（与 Rust MemoryRecallConfig::default() 保持同步）──────────
export const DEFAULTS: Required<MemoryRecallConfigDto> = {
  bootLimit: 8,
  triggerLimit: 6,
  seedLimit: 8,
  expansionLimit: 6,
  recentLimit: 3,
  fusionStrategy: 'rrf',
  rrfK: 60,
  ftsWeight: 0.5,
  vectorWeight: 0.5,
  bootLearnedSkillsLimit: 3,
  tokenBudget: 5000,
  layerExpandedSeedTake: 5,
  layerExpandedMaxDepth: 2,
  timeDecayHalfLifeDays: 7.0,
  ftsFallbackLimitMultiplier: 2.0,
  bootUserProfileLimit: 5,
}

// ─── 验证范围（与 Rust patch_memory_recall_config 保持同步）────────────
export const RANGES = {
  bootLimit: { min: 0, max: 50, label: '启动层召回数' },
  triggerLimit: { min: 0, max: 50, label: '触发层召回数' },
  seedLimit: { min: 0, max: 50, label: '种子层召回数' },
  expansionLimit: { min: 0, max: 50, label: '扩展层召回数' },
  recentLimit: { min: 0, max: 30, label: '近期层召回数' },
  rrfK: { min: 1, max: 200, label: 'RRF 融合参数 k' },
  ftsWeight: { min: 0, max: 1, label: '全文搜索权重' },
  vectorWeight: { min: 0, max: 1, label: '向量搜索权重' },
  bootLearnedSkillsLimit: { min: 0, max: 20, label: '自动挂载技能数' },
  tokenBudget: { min: 100, max: 20000, label: 'Token 预算' },
  layerExpandedSeedTake: { min: 1, max: 20, label: '图扩展种子数' },
  layerExpandedMaxDepth: { min: 1, max: 5, label: '图扩展深度' },
  timeDecayHalfLifeDays: { min: 0.5, max: 90, label: '时间衰减半衰期 (天)' },
  ftsFallbackLimitMultiplier: { min: 1.0, max: 5.0, label: 'FTS 降级倍率' },
  bootUserProfileLimit: { min: 0, max: 20, label: '用户档案挂载数' },
} as const

export const FUSION_OPTIONS = [
  { value: 'rrf', label: 'RRF（倒数排名融合）' },
  { value: 'weighted', label: 'Weighted（加权融合）' },
]

export function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max)
}
